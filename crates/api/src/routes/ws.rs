//! The platform's single WebSocket endpoint (`GET /ws`).
//!
//! Every real-time stream is multiplexed over this one socket via
//! `subscribe` / `unsubscribe` frames carrying a channel name:
//!
//! - `global_chat` — the global chat room;
//! - `market_chat:<market uuid>` — one market's chat room;
//! - `market:<market uuid>` — one market's live feed (price updates).
//! - `market_bets:<market uuid>` — one market's live feed of placed bets.

use std::collections::HashMap;

use crate::error::ApiError;
use crate::state::{AppState, WsState, HISTORY_LIMIT};
use application::ports::{Broadcast, MessageBrokerExt, TypedStream};
use application::realtime::{
    BetPlacedBroadcast, ChatMessageBroadcast, PriceTick, PriceUpdateBroadcast,
};
use application::ApplicationError;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use chrono::{DateTime, Utc};
use domain::entities::{Bet, ChatChannel, MarketId, OutcomeId, UserId};
use domain::repositories::BetFilter;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

use super::chat::ChatMessageResponse;

pub fn router() -> OpenApiRouter<AppState> {
    // The WebSocket upgrade isn't expressible in OpenAPI, so it's a plain
    // route rather than a documented one.
    OpenApiRouter::new().route("/ws", get(ws_upgrade))
}

// --- Channel names on the wire ---

/// Wire name of the global chat room.
const GLOBAL_CHAT: &str = "global_chat";

/// Wire-name prefix of a market's chat room: `market_chat:<market uuid>`.
const MARKET_CHAT_PREFIX: &str = "market_chat:";

/// Wire-name prefix of a market's live feed: `market:<market uuid>`.
const MARKET_FEED_PREFIX: &str = "market:";

/// Wire-name prefix of a market's bet feed: `market_bets:<market uuid>`.
const MARKET_BETS_PREFIX: &str = "market_bets:";

/// A stream a client can subscribe to, parsed from its wire name.
#[derive(Debug, Clone, Copy)]
enum Channel {
    /// A chat room; carries `chat_message` frames both ways.
    Chat(ChatChannel),
    /// A market's live feed; server-to-client only (`price_update` frames).
    MarketFeed(MarketId),
    /// A market's live bet feed; server-to-client only (`bet_placed` frames).
    MarketBets(MarketId),
}

fn parse_channel(name: &str) -> Option<Channel> {
    if name == GLOBAL_CHAT {
        return Some(Channel::Chat(ChatChannel::Global));
    }
    if let Some(id) = name.strip_prefix(MARKET_CHAT_PREFIX) {
        let id = Uuid::parse_str(id).ok()?;
        return Some(Channel::Chat(ChatChannel::Market(id.into())));
    }
    if let Some(id) = name.strip_prefix(MARKET_FEED_PREFIX) {
        let id = Uuid::parse_str(id).ok()?;
        return Some(Channel::MarketFeed(id.into()));
    }
    let id = name.strip_prefix(MARKET_BETS_PREFIX)?;
    let id = Uuid::parse_str(id).ok()?;
    Some(Channel::MarketBets(id.into()))
}

// --- Frames ---

/// Frames a client may send. `channel` is a wire channel name (see the module
/// docs).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    /// Join a channel: replays its current state (chat history / market
    /// prices), then streams live frames.
    Subscribe { channel: String },
    /// Leave a channel.
    Unsubscribe { channel: String },
    /// Post a message to a chat room (subscribing first is not required).
    ChatMessage { channel: String, body: String },
}

/// Frames the server sends.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame {
    /// One chat room's recent messages (oldest first), sent once per subscribe.
    History {
        channel: String,
        data: Vec<ChatMessageResponse>,
    },
    /// One market's recent bets (oldest first), sent once per bet-feed
    /// subscribe. Shares the `history` wire tag with the chat variant; the
    /// channel name tells the client which payload shape to expect.
    #[serde(rename = "history")]
    BetHistory {
        channel: String,
        data: Vec<BetPlacedResponse>,
    },
    /// A live message posted to a chat room this client is subscribed to.
    ChatMessage {
        channel: String,
        data: ChatMessageResponse,
    },
    /// One outcome's new price on a market feed. A snapshot of every outcome
    /// is sent on subscribe; live frames follow as bets move the prices.
    PriceUpdate { channel: String, data: PriceUpdateResponse },
    /// One newly committed bet on a market's bet feed.
    BetPlaced {
        channel: String,
        data: BetPlacedResponse,
    },
    /// A problem with the client's last frame; the connection stays open.
    Error { message: String },
}

#[derive(Debug, Serialize)]
struct PriceUpdateResponse {
    outcome_id: Uuid,
    price: f64,
    recorded_at: DateTime<Utc>,
}

impl From<PriceTick> for PriceUpdateResponse {
    fn from(value: PriceTick) -> Self {
        Self {
            outcome_id: value.outcome_id.as_uuid(),
            price: value.price.as_fraction(),
            recorded_at: value.recorded_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct BetPlacedResponse {
    id: Uuid,
    user_id: Uuid,
    outcome_id: OutcomeId,
    amount: i64,
    price: f64,
    created_at: DateTime<Utc>,
}

impl From<BetPlacedBroadcast> for BetPlacedResponse {
    fn from(value: BetPlacedBroadcast) -> Self {
        Self {
            id: value.id.as_uuid(),
            user_id: value.user_id.as_uuid(),
            outcome_id: value.outcome_id,
            amount: value.amount,
            price: value.price.as_fraction(),
            created_at: value.created_at,
        }
    }
}

impl From<&Bet> for BetPlacedResponse {
    fn from(bet: &Bet) -> Self {
        Self {
            id: bet.id().as_uuid(),
            user_id: bet.user_id().as_uuid(),
            outcome_id: bet.outcome_id(),
            amount: bet.amount(),
            price: bet.price().as_fraction(),
            created_at: bet.created_at(),
        }
    }
}

/// Query string on the WebSocket handshake. Browsers can't set the
/// `Authorization` header on a WS request, so the access token rides here.
#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

// --- Handlers ---

/// Upgrades to the multiplexed WebSocket. Connect to `/ws?token=<access
/// token>`, then subscribe to channels with `subscribe` frames.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
    Query(query): Query<WsQuery>,
) -> Result<Response, ApiError> {
    let claims = state
        .access_tokens
        .verify(&query.token)
        .map_err(ApplicationError::from)?;
    let user_id = claims.user_id;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, user_id)))
}

/// One forwarding task per subscribed channel, keyed by wire channel name.
type Subscriptions = HashMap<String, JoinHandle<()>>;

/// Drives one WebSocket connection. Channels are multiplexed: each subscribe
/// spawns a task forwarding that channel's broker stream into a single queue,
/// which this loop drains, so the socket is only ever written from here.
async fn handle_socket(mut socket: WebSocket, state: WsState, user_id: UserId) {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut subs: Subscriptions = HashMap::new();

    loop {
        tokio::select! {
            // Live frame from one of the subscribed channels -> forward.
            Some(frame) = rx.recv() => {
                if socket.send(Message::Text(frame.into())).await.is_err() {
                    break;
                }
            }
            // Frame from this client -> handle.
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if handle_frame(&mut socket, &state, user_id, text.as_str(), &tx, &mut subs)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                // Ping/Pong are handled by axum; ignore anything else.
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    tracing::debug!(user_id = %user_id, error = %e, "ws socket error");
                    break;
                }
            },
        }
    }

    for handle in subs.into_values() {
        handle.abort();
    }
}

/// Handles one inbound text frame. Validation and parse errors are reported
/// back to the sender only; `Err` means the socket itself is dead.
async fn handle_frame(
    socket: &mut WebSocket,
    state: &WsState,
    user_id: UserId,
    text: &str,
    tx: &mpsc::Sender<String>,
    subs: &mut Subscriptions,
) -> Result<(), ()> {
    let frame = match serde_json::from_str::<ClientFrame>(text) {
        Ok(frame) => frame,
        Err(_) => {
            let hint = "expected {\"type\": \"subscribe|unsubscribe|chat_message\", \"channel\": \
                        \"global_chat\" | \"market_chat:<id>\" | \"market:<id>\" | \"market_bets:<id>\", ...}";
            return send_error(socket, hint).await;
        }
    };

    match frame {
        ClientFrame::Subscribe { channel: name } => {
            let Some(channel) = parse_channel(&name) else {
                return send_error(socket, &format!("unknown channel {name:?}")).await;
            };
            if subs.contains_key(&name) {
                return send_error(socket, &format!("already subscribed to {name}")).await;
            }
            // Per kind: subscribe first, then replay current state, so
            // nothing published in between is lost; clients deduplicate any
            // overlap (chat and bets by id, price frames by being idempotent).
            let forward = match channel {
                Channel::Chat(chat) => {
                    let Some(live) =
                        subscribe::<ChatMessageBroadcast>(socket, state, &chat).await?
                    else {
                        return Ok(());
                    };
                    let views = match state.list_recent.execute(chat, HISTORY_LIMIT).await {
                        Ok(views) => views,
                        Err(e) => return send_error(socket, &e.to_string()).await,
                    };
                    send_frame(
                        socket,
                        &ServerFrame::History {
                            channel: name.clone(),
                            data: views.iter().map(ChatMessageResponse::from).collect(),
                        },
                    )
                        .await?;

                    let frame_channel = name.clone();
                    spawn_forwarder(live, tx.clone(), move |message| {
                        vec![ServerFrame::ChatMessage {
                            channel: frame_channel.clone(),
                            data: message.into(),
                        }]
                    })
                }
                Channel::MarketFeed(market_id) => {
                    // A feed of a market that doesn't exist would just stay
                    // silent forever; reject it up front instead.
                    match state.markets.find_by_id(market_id).await {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            return send_error(socket, &format!("unknown market {market_id}"))
                                .await;
                        }
                        Err(e) => return send_error(socket, &e.to_string()).await,
                    }
                    let Some(live) =
                        subscribe::<PriceUpdateBroadcast>(socket, state, &market_id).await?
                    else {
                        return Ok(());
                    };
                    let outcomes = match state.markets.outcomes_for(market_id).await {
                        Ok(outcomes) => outcomes,
                        Err(e) => return send_error(socket, &e.to_string()).await,
                    };
                    let now = Utc::now();
                    for outcome in &outcomes {
                        send_frame(
                            socket,
                            &ServerFrame::PriceUpdate {
                                channel: name.clone(),
                                data: PriceUpdateResponse {
                                    outcome_id: outcome.id().as_uuid(),
                                    price: outcome.current_price().as_fraction(),
                                    recorded_at: now,
                                },
                            },
                        )
                            .await?;
                    }

                    // One batch of ticks per price move; fan out one frame
                    // per outcome.
                    let frame_channel = name.clone();
                    spawn_forwarder(live, tx.clone(), move |message| {
                        message
                            .ticks
                            .into_iter()
                            .map(|tick| ServerFrame::PriceUpdate {
                                channel: frame_channel.clone(),
                                data: PriceUpdateResponse::from(tick),
                            })
                            .collect()
                    })
                }
                Channel::MarketBets(market_id) => {
                    match state.markets.find_by_id(market_id).await {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            return send_error(socket, &format!("unknown market {market_id}"))
                                .await;
                        }
                        Err(e) => return send_error(socket, &e.to_string()).await,
                    }
                    let Some(live) =
                        subscribe::<BetPlacedBroadcast>(socket, state, &market_id).await?
                    else {
                        return Ok(());
                    };
                    let filter = BetFilter {
                        limit: HISTORY_LIMIT,
                        ..BetFilter::default()
                    };
                    let mut bets = match state.bets.find_by_market(market_id, &filter).await {
                        Ok(bets) => bets,
                        Err(e) => return send_error(socket, &e.to_string()).await,
                    };
                    // The repo lists newest first; history replays oldest
                    // first, like chat.
                    bets.reverse();
                    send_frame(
                        socket,
                        &ServerFrame::BetHistory {
                            channel: name.clone(),
                            data: bets.iter().map(BetPlacedResponse::from).collect(),
                        },
                    )
                        .await?;

                    let frame_channel = name.clone();
                    spawn_forwarder(live, tx.clone(), move |message| {
                        vec![ServerFrame::BetPlaced {
                            channel: frame_channel.clone(),
                            data: BetPlacedResponse::from(message),
                        }]
                    })
                }
            };
            subs.insert(name, forward);
        }

        ClientFrame::Unsubscribe { channel: name } => match subs.remove(&name) {
            Some(handle) => handle.abort(),
            None => return send_error(socket, &format!("not subscribed to {name}")).await,
        },

        ClientFrame::ChatMessage {
            channel: name,
            body,
        } => {
            let Some(channel) = parse_channel(&name) else {
                return send_error(socket, &format!("unknown channel {name:?}")).await;
            };
            let Channel::Chat(chat) = channel else {
                return send_error(socket, &format!("{name} is not a chat channel")).await;
            };
            // Persisting the message also records its broadcast event, so
            // delivery to every subscriber — including this sender, who
            // thereby receives the server-assigned id and timestamp — rides
            // the outbox -> broker pipeline.
            if let Err(e) = state.post_message.execute(user_id, chat, body).await {
                return send_error(socket, &e.to_string()).await;
            }
        }
    }

    Ok(())
}

/// Subscribes to the broker channel carrying `M` for `scope`, reporting a
/// failure to the client. `Ok(None)` means the subscription failed but the
/// socket is still alive; `Err` means the socket itself is dead.
async fn subscribe<M>(
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

/// Spawns the task that forwards one channel's typed broker stream into the
/// connection's write queue, expanding each message into wire frames.
fn spawn_forwarder<M>(
    mut live: TypedStream<M>,
    tx: mpsc::Sender<String>,
    frames: impl Fn(M) -> Vec<ServerFrame> + Send + 'static,
) -> JoinHandle<()>
where
    M: Send + 'static,
{
    tokio::spawn(async move {
        while let Some(message) = live.next().await {
            for frame in frames(message) {
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

/// Sends one frame; `Err` means the socket is dead and the connection loop
/// should stop.
async fn send_frame(socket: &mut WebSocket, frame: &ServerFrame) -> Result<(), ()> {
    let text = match serde_json::to_string(frame) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize ws frame");
            return Ok(());
        }
    };
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

async fn send_error(socket: &mut WebSocket, message: &str) -> Result<(), ()> {
    send_frame(
        socket,
        &ServerFrame::Error {
            message: message.to_owned(),
        },
    )
        .await
}
