//! The per-connection loop: one task per socket, owning the write side.

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket};
use domain::entities::UserId;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::state::WsState;

use super::channel::{self, Channel};
use super::frames::{ClientFrame, send_error};
use super::subscribe;

/// One forwarding task per subscribed channel, keyed by wire channel name.
type Subscriptions = HashMap<String, JoinHandle<()>>;

/// Drives one WebSocket connection. Channels are multiplexed: each subscribe
/// spawns a task forwarding that channel's broker stream into a single queue,
/// which this loop drains, so the socket is only ever written from here.
pub(super) async fn handle_socket(mut socket: WebSocket, state: WsState, user_id: UserId) {
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
    let Ok(frame) = serde_json::from_str::<ClientFrame>(text) else {
        return send_error(socket, channel::HINT).await;
    };

    match frame {
        ClientFrame::Subscribe { channel: name } => {
            let Some(channel) = channel::parse(&name) else {
                return send_error(socket, &format!("unknown channel {name:?}")).await;
            };
            if subs.contains_key(&name) {
                return send_error(socket, &format!("already subscribed to {name}")).await;
            }
            let joined = match channel {
                Channel::Chat(room) => {
                    subscribe::chat(socket, state, user_id, room, &name, tx).await?
                }
                Channel::Bets(feed) => subscribe::bets(socket, state, feed, &name, tx).await?,
                Channel::MarketFeed(market_id) => {
                    subscribe::market_feed(socket, state, market_id, &name, tx).await?
                }
            };
            // `None` means the join failed and the client already knows why.
            if let Some(forward) = joined {
                subs.insert(name, forward);
            }
        }

        ClientFrame::Unsubscribe { channel: name } => match subs.remove(&name) {
            Some(handle) => handle.abort(),
            None => return send_error(socket, &format!("not subscribed to {name}")).await,
        },

        ClientFrame::ChatMessage {
            channel: name,
            body,
            reply_to,
        } => {
            let Some(channel) = channel::parse(&name) else {
                return send_error(socket, &format!("unknown channel {name:?}")).await;
            };
            let Channel::Chat(room) = channel else {
                return send_error(socket, &format!("{name} is not a chat channel")).await;
            };
            // Persisting the message also records its broadcast event, so
            // delivery to every subscriber — including this sender, who
            // thereby receives the server-assigned id and timestamp — rides
            // the outbox -> broker pipeline.
            let posted = state
                .post_message
                .execute(user_id, room, body, reply_to.map(Into::into))
                .await;
            if let Err(e) = posted {
                return send_error(socket, &e.to_string()).await;
            }
        }

        // Both directions reach every subscriber the same way a message does:
        // the write records an event, and the broadcaster turns it into a
        // `reaction_update` frame carrying the resulting count.
        ClientFrame::AddReaction { message_id, emoji } => {
            if let Err(e) = state.react.add(user_id, message_id.into(), emoji).await {
                return send_error(socket, &e.to_string()).await;
            }
        }

        ClientFrame::RemoveReaction { message_id, emoji } => {
            if let Err(e) = state.react.remove(user_id, message_id.into(), emoji).await {
                return send_error(socket, &e.to_string()).await;
            }
        }
    }

    Ok(())
}
