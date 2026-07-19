use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::RepositoryError;
use crate::entities::{ChatChannel, ChatMessage, MessageId};

/// A message's position in the chat ordering. Messages are ordered by
/// `(created_at, id)`; the id breaks ties between messages sharing a
/// timestamp so paging can never skip or repeat one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAnchor {
    pub id: MessageId,
    pub created_at: DateTime<Utc>,
}

impl From<&ChatMessage> for MessageAnchor {
    fn from(message: &ChatMessage) -> Self {
        Self {
            id: message.id(),
            created_at: message.created_at(),
        }
    }
}

/// Which side of an anchor message to page towards. Both directions return
/// the `limit` messages *adjacent* to the anchor, excluding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCursor {
    /// The newest messages strictly older than the anchor.
    Before(MessageAnchor),
    /// The oldest messages strictly newer than the anchor.
    After(MessageAnchor),
}

/// Port for chat message persistence. Implementations live in the
/// infrastructure layer.
#[async_trait]
pub trait ChatMessageRepository: Send + Sync {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: MessageId) -> Result<Option<ChatMessage>, RepositoryError>;

    /// Up to `limit` messages of one channel, returned oldest-first so callers
    /// can render them in chronological order. Without a cursor these are the
    /// most recent messages; with one they are the page adjacent to its anchor.
    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
        cursor: Option<MessageCursor>,
    ) -> Result<Vec<ChatMessage>, RepositoryError>;
}
