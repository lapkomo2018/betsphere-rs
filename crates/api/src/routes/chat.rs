use std::collections::HashMap;

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, ChatState, HISTORY_LIMIT};
use application::ApplicationError;
use application::ports::MessageBrokerExt;
use application::use_cases::chat::ChatMessageView;
use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use chrono::{DateTime, Utc};
use domain::entities::{ChatChannel, UserId};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_messages))
        // The WebSocket upgrade isn't expressible in OpenAPI, so it's a plain
        // route rather than a documented one.
        .route("/ws", get(chat_ws))
}

// --- Channel names on the wire ---

/// Wire name of the global chat room.
const GLOBAL_CHAT: &str = "global_chat";

/// Wire-name prefix of a market's chat room: `market_chat:<market uuid>`.
const MARKET_CHAT_PREFIX: &str = "market_chat:";

fn parse_channel(name: &str) -> Option<ChatChannel> {
    if name == GLOBAL_CHAT {
        return Some(ChatChannel::Global);
    }
    let id = name.strip_prefix(MARKET_CHAT_PREFIX)?;
    let id = Uuid::parse_str(id).ok()?;
    Some(ChatChannel::Market(id.into()))
}

/// Broker (pub/sub) channel a chat room fans out over. Distinct from the wire
/// name so all chat traffic stays grouped under `chat:` in the broker's
/// namespace, which other features share.
fn broker_channel(channel: ChatChannel) -> String {
    match channel.market_id() {
        None => "chat:global".to_owned(),
        Some(id) => format!("chat:market:{id}"),
    }
}

// --- DTOs ---

/// Author fields embedded in a chat message (public profile only).
#[derive(Debug, Clone, Serialize, ToSchema)]
struct MessageAuthor {
    id: Uuid,
    username: String,
    avatar_url: Option<String>,
}

/// A chat message as sent to clients over both REST and the WebSocket.
#[derive(Debug, Clone, Serialize, ToSchema)]
struct ChatMessageResponse {
    id: Uuid,
    author: MessageAuthor,
    body: String,
    created_at: DateTime<Utc>,
}

impl From<&ChatMessageView> for ChatMessageResponse {
    fn from(view: &ChatMessageView) -> Self {
        Self {
            id: view.message.id().as_uuid(),
            author: MessageAuthor {
                id: view.author.id().as_uuid(),
                username: view.author.username().to_string(),
                avatar_url: view.author.avatar_url().map(str::to_owned),
            },
            body: view.message.body().as_str().to_owned(),
            created_at: view.message.created_at(),
        }
    }
}

/// Frames a client may send. `channel` is `"global_chat"` or
/// `"market_chat:<market uuid>"`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    /// Join a room: replays its recent history, then streams live messages.
    Subscribe { channel: String },
    /// Leave a room.
    Unsubscribe { channel: String },
    /// Post a message to a room (subscribing first is not required).
    ChatMessage { channel: String, body: String },
}

/// Frames the server sends.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame {
    /// One room's recent messages (oldest first), sent once per subscribe.
    History {
        channel: String,
        data: Vec<ChatMessageResponse>,
    },
    /// A live message posted to a room this client is subscribed to.
    ChatMessage {
        channel: String,
        data: ChatMessageResponse,
    },
    /// A problem with the client's last frame; the connection stays open.
    Error { message: String },
}

/// Query string on the history endpoint.
#[derive(Debug, Deserialize, IntoParams)]
struct HistoryQuery {
    /// Return this market's chat room instead of the global one.
    market_id: Option<Uuid>,
}

/// Query string on the WebSocket handshake. Browsers can't set the
/// `Authorization` header on a WS request, so the access token rides here.
#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/messages",
    tag = "chat",
    params(HistoryQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Recent messages of one chat room, oldest first", body = [ChatMessageResponse]),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Unknown market", body = ErrorResponse),
    )
)]
async fn get_messages(
    State(state): State<ChatState>,
    _: CurrentUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<ChatMessageResponse>>, ApiError> {
    let channel = ChatChannel::from(query.market_id.map(Into::into));
    let views = state.list_recent.execute(channel, HISTORY_LIMIT).await?;
    let messages = views.iter().map(ChatMessageResponse::from).collect();
    Ok(Json(messages))
}

/// Upgrades to the chat WebSocket. Connect to `/api/chat/ws?token=<access
/// token>`, then multiplex rooms over the one socket with `subscribe` /
/// `unsubscribe` frames.
async fn chat_ws(
    ws: WebSocketUpgrade,
    State(state): State<ChatState>,
    Query(query): Query<WsQuery>,
) -> Result<Response, ApiError> {
    let claims = state
        .access_tokens
        .verify(&query.token)
        .map_err(ApplicationError::from)?;
    let user_id = claims.user_id;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, user_id)))
}

/// One forwarding task per subscribed room, keyed by wire channel name.
type Subscriptions = HashMap<String, JoinHandle<()>>;

/// Drives one WebSocket connection. Rooms are multiplexed: each subscribe
/// spawns a task forwarding that room's broker stream into a single queue,
/// which this loop drains, so the socket is only ever written from here.
async fn handle_socket(mut socket: WebSocket, state: ChatState, user_id: UserId) {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut subs: Subscriptions = HashMap::new();

    loop {
        tokio::select! {
            // Live frame from one of the subscribed rooms -> forward.
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
                    tracing::debug!(user_id = %user_id, error = %e, "chat socket error");
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
    state: &ChatState,
    user_id: UserId,
    text: &str,
    tx: &mpsc::Sender<String>,
    subs: &mut Subscriptions,
) -> Result<(), ()> {
    let frame = match serde_json::from_str::<ClientFrame>(text) {
        Ok(frame) => frame,
        Err(_) => {
            let hint = "expected {\"type\": \"subscribe|unsubscribe|chat_message\", \
                        \"channel\": \"global_chat\" | \"market_chat:<id>\", ...}";
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

            // Subscribe before loading history so nothing published in between
            // is lost; any overlap is deduplicated client-side by message id.
            let live = match state.broker.subscribe(&broker_channel(channel)).await {
                Ok(live) => live,
                Err(e) => {
                    tracing::error!(error = %e, "failed to subscribe to chat channel");
                    return send_error(socket, "subscription failed, try again").await;
                }
            };
            let views = match state.list_recent.execute(channel, HISTORY_LIMIT).await {
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

            let tx = tx.clone();
            let forward = tokio::spawn(async move {
                let mut live = live;
                while let Some(payload) = live.next().await {
                    // Payloads are the JSON frames we publish, so forward as-is.
                    match String::from_utf8(payload) {
                        Ok(text) => {
                            if tx.send(text).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "skipping non-UTF-8 chat frame"),
                    }
                }
            });
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
            let view = match state.post_message.execute(user_id, channel, body).await {
                Ok(view) => view,
                Err(e) => return send_error(socket, &e.to_string()).await,
            };

            // Publish to every subscriber, including this sender, so they
            // receive the server-assigned id and timestamp. The message is
            // already persisted, so a broadcast failure only means live
            // delivery was missed.
            let outgoing = ServerFrame::ChatMessage {
                channel: name,
                data: ChatMessageResponse::from(&view),
            };
            if let Err(e) = state
                .broker
                .publish_json(&broker_channel(channel), &outgoing)
                .await
            {
                tracing::error!(error = %e, "failed to broadcast chat message");
                return send_error(socket, "message saved but could not be delivered live").await;
            }
        }
    }

    Ok(())
}

/// Sends one frame; `Err` means the socket is dead and the connection loop
/// should stop.
async fn send_frame(socket: &mut WebSocket, frame: &ServerFrame) -> Result<(), ()> {
    let text = match serde_json::to_string(frame) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize chat frame");
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
