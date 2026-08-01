//! Joining one channel: subscribe to the broker, replay the channel's current
//! state, then hand back the task that forwards its live frames.
//!
//! Every function here follows the same order — subscribe *first*, replay
//! second — so nothing published in between is lost. Clients deduplicate any
//! overlap: chat and bets by id, price frames by being idempotent.
//!
//! All three return `Ok(None)` to mean "this subscription failed and the client
//! was told, but the socket is fine"; `Err(())` means the socket itself is dead.

use application::realtime::{
    BetFeed, BetPlacedBroadcast, ChatMessageBroadcast, ChatReactionBroadcast, PriceUpdateBroadcast,
};
use application::use_cases::chat::HistoryWindow;
use axum::extract::ws::WebSocket;
use chrono::Utc;
use domain::entities::{Bet, ChatChannel, MarketId, UserId};
use domain::repositories::{BetFilter, RepositoryError};
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use application::ports::{Broadcast, MessageBrokerExt, TypedStream};

use crate::routes::chat::ChatMessageResponse;
use crate::state::{HISTORY_LIMIT, WsState};

use super::frames::{
    self, BetPlacedResponse, PriceUpdateResponse, ReactionUpdateResponse, ServerFrame, send_error,
};

/// The outcome of a join: the forwarding task, or `None` if the client was sent
/// an error instead.
type Joined = Result<Option<JoinHandle<()>>, ()>;

/// Joins a chat room: replays the newest page of messages, then streams live
/// ones and the reaction changes on them.
///
/// Messages and reactions travel on two broker channels but one wire channel,
/// so the two streams are interleaved into the single forwarding task a
/// subscription owns.
pub(super) async fn chat(
    socket: &mut WebSocket,
    state: &WsState,
    viewer: UserId,
    room: ChatChannel,
    name: &str,
    tx: &mpsc::Sender<String>,
) -> Joined {
    let Some(messages) = broker_stream::<ChatMessageBroadcast>(socket, state, &room).await? else {
        return Ok(None);
    };
    let Some(reactions) = broker_stream::<ChatReactionBroadcast>(socket, state, &room).await?
    else {
        return Ok(None);
    };
    // Live subscribers always replay the newest page, tallied for whoever is
    // on this socket.
    let views = match state
        .list_recent
        .execute(viewer, room, HISTORY_LIMIT, HistoryWindow::default())
        .await
    {
        Ok(views) => views,
        Err(e) => {
            send_error(socket, &e.to_string()).await?;
            return Ok(None);
        }
    };
    frames::send(
        socket,
        &ServerFrame::History {
            channel: name.to_owned(),
            data: views.iter().map(ChatMessageResponse::from).collect(),
        },
    )
    .await?;

    let (channel, reaction_channel) = (name.to_owned(), name.to_owned());
    let live = futures::stream::select(
        messages.map(move |message| ServerFrame::ChatMessage {
            channel: channel.clone(),
            data: message.into(),
        }),
        reactions.map(move |reaction| ServerFrame::ReactionUpdate {
            channel: reaction_channel.clone(),
            data: ReactionUpdateResponse::from(reaction),
        }),
    );
    Ok(Some(spawn_forwarder(live, tx.clone(), |frame| vec![frame])))
}

/// Joins a bet feed, global or market-scoped: replays the newest page of bets,
/// then streams live ones.
pub(super) async fn bets(
    socket: &mut WebSocket,
    state: &WsState,
    feed: BetFeed,
    name: &str,
    tx: &mpsc::Sender<String>,
) -> Joined {
    // Same existence check as the price feed, for the same reason. The global
    // feed has nothing to check: it is always valid, even before the first bet.
    if let BetFeed::Market(market_id) = feed
        && !market_exists(socket, state, market_id).await?
    {
        return Ok(None);
    }
    let Some(live) = broker_stream::<BetPlacedBroadcast>(socket, state, &feed).await? else {
        return Ok(None);
    };
    let filter = BetFilter {
        limit: HISTORY_LIMIT,
        ..BetFilter::default()
    };
    let history: Result<Vec<Bet>, RepositoryError> = match feed {
        BetFeed::Global => state.bets.feed(&filter).await,
        BetFeed::Market(market_id) => state.bets.find_by_market(market_id, &filter).await,
    };
    let mut bets = match history {
        Ok(bets) => bets,
        Err(e) => {
            send_error(socket, &e.to_string()).await?;
            return Ok(None);
        }
    };
    // The repo lists newest first; history replays oldest first, like chat.
    bets.reverse();
    frames::send(
        socket,
        &ServerFrame::BetHistory {
            channel: name.to_owned(),
            data: bets.iter().map(BetPlacedResponse::from).collect(),
        },
    )
    .await?;

    let channel = name.to_owned();
    Ok(Some(spawn_forwarder(live, tx.clone(), move |message| {
        vec![ServerFrame::BetPlaced {
            channel: channel.clone(),
            data: BetPlacedResponse::from(message),
        }]
    })))
}

/// Joins a market's price feed: replays a snapshot of every outcome's current
/// price, then streams the moves.
pub(super) async fn market_feed(
    socket: &mut WebSocket,
    state: &WsState,
    market_id: MarketId,
    name: &str,
    tx: &mpsc::Sender<String>,
) -> Joined {
    if !market_exists(socket, state, market_id).await? {
        return Ok(None);
    }
    let Some(live) = broker_stream::<PriceUpdateBroadcast>(socket, state, &market_id).await? else {
        return Ok(None);
    };
    let outcomes = match state.markets.outcomes_for(market_id).await {
        Ok(outcomes) => outcomes,
        Err(e) => {
            send_error(socket, &e.to_string()).await?;
            return Ok(None);
        }
    };
    let now = Utc::now();
    for outcome in &outcomes {
        frames::send(
            socket,
            &ServerFrame::PriceUpdate {
                channel: name.to_owned(),
                data: PriceUpdateResponse {
                    outcome_id: outcome.id().as_uuid(),
                    price: outcome.current_price().as_fraction(),
                    recorded_at: now,
                },
            },
        )
        .await?;
    }

    // One batch of ticks per price move; fan out one frame per outcome.
    let channel = name.to_owned();
    Ok(Some(spawn_forwarder(live, tx.clone(), move |message| {
        message
            .ticks
            .into_iter()
            .map(|tick| ServerFrame::PriceUpdate {
                channel: channel.clone(),
                data: PriceUpdateResponse::from(tick),
            })
            .collect()
    })))
}

/// A feed of a market that doesn't exist would just stay silent forever, so
/// every market-scoped channel rejects one up front. `Ok(false)` means the
/// client was told why.
async fn market_exists(
    socket: &mut WebSocket,
    state: &WsState,
    market_id: MarketId,
) -> Result<bool, ()> {
    match state.markets.find_by_id(market_id).await {
        Ok(Some(_)) => Ok(true),
        Ok(None) => {
            send_error(socket, &format!("unknown market {market_id}")).await?;
            Ok(false)
        }
        Err(e) => {
            send_error(socket, &e.to_string()).await?;
            Ok(false)
        }
    }
}

/// Subscribes to the broker channel carrying `M` for `scope`, reporting a
/// failure to the client.
async fn broker_stream<M>(
    socket: &mut WebSocket,
    state: &WsState,
    scope: &M::Scope,
) -> Result<Option<TypedStream<M>>, ()>
where
    M: Broadcast,
    M::Scope: Sync,
{
    match state.broker.subscribe_broadcast::<M>(scope).await {
        Ok(live) => Ok(Some(live)),
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to ws channel");
            send_error(socket, "subscription failed, try again").await?;
            Ok(None)
        }
    }
}

/// Spawns the task that forwards one subscription's live stream into the
/// connection's write queue, expanding each message into wire frames.
fn spawn_forwarder<S, M>(
    mut live: S,
    tx: mpsc::Sender<String>,
    to_frames: impl Fn(M) -> Vec<ServerFrame> + Send + 'static,
) -> JoinHandle<()>
where
    S: Stream<Item = M> + Send + Unpin + 'static,
    M: Send + 'static,
{
    tokio::spawn(async move {
        while let Some(message) = live.next().await {
            for frame in to_frames(message) {
                match serde_json::to_string(&frame) {
                    Ok(text) => {
                        if tx.send(text).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "failed to serialize ws frame"),
                }
            }
        }
    })
}
