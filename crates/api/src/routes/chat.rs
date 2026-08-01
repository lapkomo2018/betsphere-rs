//! Chat's REST surface: message history. Live chat runs over the general
//! WebSocket endpoint (see [`super::ws`]).

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, ChatState, HISTORY_LIMIT};
use application::realtime::{ChatAuthor, ChatMessageBroadcast, QuotedMessage};
use application::use_cases::chat::{ChatMessageView, HistoryWindow, RepliedMessage};
use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use domain::entities::{ChatChannel, User};
use domain::repositories::ReactionTally;
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

impl From<&User> for MessageAuthor {
    fn from(author: &User) -> Self {
        Self {
            id: author.id().as_uuid(),
            username: author.username().to_string(),
            avatar_url: author.avatar_url().map(str::to_owned),
        }
    }
}

impl From<ChatAuthor> for MessageAuthor {
    fn from(author: ChatAuthor) -> Self {
        Self {
            id: author.id.as_uuid(),
            username: author.username.to_string(),
            avatar_url: author.avatar_url,
        }
    }
}

/// The message a reply quotes, shown above it. Only what a quote line renders:
/// following the chain further is the client's business, not the payload's.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(super) struct QuotedMessageResponse {
    id: Uuid,
    author: MessageAuthor,
    body: String,
}

impl From<QuotedMessage> for QuotedMessageResponse {
    fn from(quoted: QuotedMessage) -> Self {
        Self {
            id: quoted.id.as_uuid(),
            author: quoted.author.into(),
            body: quoted.body,
        }
    }
}

impl From<&RepliedMessage> for QuotedMessageResponse {
    fn from(quoted: &RepliedMessage) -> Self {
        Self {
            id: quoted.message.id().as_uuid(),
            author: (&quoted.author).into(),
            body: quoted.message.body().as_str().to_owned(),
        }
    }
}

/// One emoji's standing on a message.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(super) struct ReactionResponse {
    emoji: String,
    count: i64,
    /// Whether the account this response was built for is among the reactors.
    reacted: bool,
}

impl From<&ReactionTally> for ReactionResponse {
    fn from(tally: &ReactionTally) -> Self {
        Self {
            emoji: tally.emoji.as_str().to_owned(),
            count: tally.count,
            reacted: tally.reacted,
        }
    }
}

/// A chat message as sent to clients over both REST and the WebSocket.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(super) struct ChatMessageResponse {
    id: Uuid,
    author: MessageAuthor,
    body: String,
    /// The quoted message, when this one is a reply.
    reply_to: Option<QuotedMessageResponse>,
    /// One entry per distinct emoji, in the order the emoji first appeared on
    /// the message. Empty when nobody has reacted.
    reactions: Vec<ReactionResponse>,
    created_at: DateTime<Utc>,
}

impl From<&ChatMessageView> for ChatMessageResponse {
    fn from(view: &ChatMessageView) -> Self {
        Self {
            id: view.message.id().as_uuid(),
            author: (&view.author).into(),
            body: view.message.body().as_str().to_owned(),
            reply_to: view.reply_to.as_ref().map(Into::into),
            reactions: view.reactions.iter().map(Into::into).collect(),
            created_at: view.message.created_at(),
        }
    }
}

impl From<ChatMessageBroadcast> for ChatMessageResponse {
    fn from(broadcast: ChatMessageBroadcast) -> Self {
        Self {
            id: broadcast.id.as_uuid(),
            author: broadcast.author.into(),
            body: broadcast.body,
            reply_to: broadcast.reply_to.map(Into::into),
            // A message this fresh cannot have been reacted to yet; the
            // reactions that follow arrive as their own frames.
            reactions: Vec::new(),
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
    /// Mutually exclusive with `after_uuid`.
    before_uuid: Option<Uuid>,
    /// Page forwards: the messages immediately newer than this one.
    /// Mutually exclusive with `before_uuid`.
    after_uuid: Option<Uuid>,
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
    CurrentUser(claims): CurrentUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<ChatMessageResponse>>, ApiError> {
    let channel = ChatChannel::from(query.market_id.map(Into::into));
    let window = HistoryWindow {
        before: query.before_uuid.map(Into::into),
        after: query.after_uuid.map(Into::into),
    };
    // The reaction tallies are relative to the reader, so the page is built
    // for the account that asked for it.
    let views = state
        .list_recent
        .execute(claims.user_id, channel, HISTORY_LIMIT, window)
        .await?;
    let messages = views.iter().map(ChatMessageResponse::from).collect();
    Ok(Json(messages))
}
