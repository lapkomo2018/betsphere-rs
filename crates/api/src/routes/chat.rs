use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, ChatState, GLOBAL_CHANNEL, HISTORY_LIMIT};
use application::ApplicationError;
use application::ports::MessageBrokerExt;
use application::use_cases::chat::ChatMessageView;
use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use chrono::{DateTime, Utc};
use domain::entities::UserId;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
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

/// Frame a client sends to post a message: `{"body": "hello"}`.
#[derive(Debug, Deserialize)]
struct IncomingMessage {
    body: String,
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
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Recent global-chat messages, oldest first", body = [ChatMessageResponse]),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_messages(
    State(state): State<ChatState>,
    _: CurrentUser,
) -> Result<Json<Vec<ChatMessageResponse>>, ApiError> {
    let views = state.list_recent.execute(HISTORY_LIMIT).await?;
    let messages = views.iter().map(ChatMessageResponse::from).collect();
    Ok(Json(messages))
}

/// Upgrades to a WebSocket for the global chat. Connect to
/// `/api/chat/ws?token=<access token>`.
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

/// Drives one WebSocket connection: replays recent history, then relays live
/// messages both ways until either side closes.
async fn handle_socket(mut socket: WebSocket, state: ChatState, user_id: UserId) {
    // Subscribe before loading history so nothing published in between is lost;
    // any overlap is deduplicated client-side by message id.
    let mut live = match state.broker.subscribe(GLOBAL_CHANNEL).await {
        Ok(live) => live,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to chat channel");
            return;
        }
    };

    match state.list_recent.execute(HISTORY_LIMIT).await {
        Ok(views) => {
            for view in &views {
                let frame = match serde_json::to_string(&ChatMessageResponse::from(view)) {
                    Ok(frame) => frame,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to serialize chat history");
                        return;
                    }
                };
                if socket.send(Message::Text(frame.into())).await.is_err() {
                    return;
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load chat history");
            return;
        }
    }

    loop {
        tokio::select! {
            // Live message published to the room -> forward to this client.
            payload = live.next() => match payload {
                // Payloads are the JSON frames we publish, so forward as text.
                Some(payload) => {
                    let text = match String::from_utf8(payload) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!(error = %e, "skipping non-UTF-8 chat frame");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                // Broker stream ended (backend closed); stop the loop.
                None => break,
            },
            // Frame from this client -> post to the room.
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    handle_incoming(&mut socket, &state, user_id, text.as_str()).await;
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
}

/// Parses, persists, and broadcasts one inbound text frame. Validation and
/// parse errors are reported back to the sender only.
async fn handle_incoming(socket: &mut WebSocket, state: &ChatState, user_id: UserId, text: &str) {
    let body = match serde_json::from_str::<IncomingMessage>(text) {
        Ok(incoming) => incoming.body,
        Err(_) => {
            send_error(socket, "expected JSON of the form {\"body\": \"...\"}").await;
            return;
        }
    };

    let view = match state.post_message.execute(user_id, body).await {
        Ok(view) => view,
        Err(e) => {
            send_error(socket, &e.to_string()).await;
            return;
        }
    };

    // Publish to everyone, including this sender, so they receive the
    // server-assigned id and timestamp. The message is already persisted, so a
    // broadcast failure only means live delivery was missed.
    if let Err(e) = state
        .broker
        .publish_json(GLOBAL_CHANNEL, &ChatMessageResponse::from(&view))
        .await
    {
        tracing::error!(error = %e, "failed to broadcast chat message");
        send_error(socket, "message saved but could not be delivered live").await;
    }
}

async fn send_error(socket: &mut WebSocket, message: &str) {
    let frame = serde_json::json!({ "error": message }).to_string();
    let _ = socket.send(Message::Text(frame.into())).await;
}
