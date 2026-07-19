//! Chat's REST surface: message history. Live chat runs over the general
//! WebSocket endpoint (see [`super::ws`]).

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, ChatState, HISTORY_LIMIT};
use application::realtime::ChatMessageBroadcast;
use application::use_cases::chat::{ChatMessageView, HistoryWindow};
use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use domain::entities::ChatChannel;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_messages))
}

// --- DTOs ---

/// Author fields embedded in a chat message (public profile only).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(super) struct MessageAuthor {
    id: Uuid,
    username: String,
    avatar_url: Option<String>,
}

/// A chat message as sent to clients over both REST and the WebSocket.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(super) struct ChatMessageResponse {
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

impl From<ChatMessageBroadcast> for ChatMessageResponse {
    fn from(broadcast: ChatMessageBroadcast) -> Self {
        Self {
            id: broadcast.id.as_uuid(),
            author: MessageAuthor {
                id: broadcast.author.id.as_uuid(),
                username: broadcast.author.username.to_string(),
                avatar_url: broadcast.author.avatar_url,
            },
            body: broadcast.body,
            created_at: broadcast.created_at,
        }
    }
}

/// Query string on the history endpoint.
#[derive(Debug, Deserialize, IntoParams)]
struct HistoryQuery {
    /// Return this market's chat room instead of the global one.
    market_id: Option<Uuid>,
    /// Page backwards: the messages immediately older than this one.
    /// Mutually exclusive with `after_id`.
    before_id: Option<Uuid>,
    /// Page forwards: the messages immediately newer than this one.
    /// Mutually exclusive with `before_id`.
    after_id: Option<Uuid>,
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/messages",
    tag = "chat",
    params(HistoryQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "One page of a chat room's messages, oldest first", body = [ChatMessageResponse]),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Unknown market, or anchor message not in this room", body = ErrorResponse),
        (status = 422, description = "before_uuid and after_uuid both set", body = ErrorResponse),
    )
)]
async fn get_messages(
    State(state): State<ChatState>,
    _: CurrentUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<ChatMessageResponse>>, ApiError> {
    let channel = ChatChannel::from(query.market_id.map(Into::into));
    let window = HistoryWindow {
        before: query.before_id.map(Into::into),
        after: query.after_id.map(Into::into),
    };
    let views = state
        .list_recent
        .execute(channel, HISTORY_LIMIT, window)
        .await?;
    let messages = views.iter().map(ChatMessageResponse::from).collect();
    Ok(Json(messages))
}
