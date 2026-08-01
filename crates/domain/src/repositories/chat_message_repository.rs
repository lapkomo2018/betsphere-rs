use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::RepositoryError;
use crate::entities::{ChatChannel, ChatMessage, MessageId, UserId};
use crate::value_objects::chat::ReactionEmoji;

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

/// How one emoji stands on one message: how many users hold it, and whether
/// the reader is among them. Reactors are counted rather than listed — the
/// list is unbounded, and the only reactor a client needs to name is itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionTally {
    pub emoji: ReactionEmoji,
    pub count: i64,
    pub reacted: bool,
}

/// Port for chat message persistence. Implementations live in the
/// infrastructure layer.
#[async_trait]
pub trait ChatMessageRepository: Send + Sync {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: MessageId) -> Result<Option<ChatMessage>, RepositoryError>;

    /// The messages among `ids` that still exist, in no particular order.
    /// Batches the lookups a page of replies would otherwise make one at a
    /// time, so callers index the result themselves.
    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<ChatMessage>, RepositoryError>;

    /// Up to `limit` messages of one channel, returned oldest-first so callers
    /// can render them in chronological order. Without a cursor these are the
    /// most recent messages; with one they are the page adjacent to its anchor.
    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
        cursor: Option<MessageCursor>,
    ) -> Result<Vec<ChatMessage>, RepositoryError>;

    /// Records `user_id`'s reaction to a message. Returns whether it was new:
    /// reacting twice with the same emoji is a no-op, not a conflict, because
    /// a client retrying a lost frame must not be punished for it.
    async fn add_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &ReactionEmoji,
    ) -> Result<bool, RepositoryError>;

    /// Takes `user_id`'s reaction back. Returns whether one was there to take.
    async fn remove_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &ReactionEmoji,
    ) -> Result<bool, RepositoryError>;

    /// The reactions on each of `message_ids`, tallied from `viewer`'s point of
    /// view and ordered by when each emoji first appeared on its message, so a
    /// message's reaction row keeps a stable order as counts move. Messages
    /// with no reactions are absent from the map rather than mapped to an
    /// empty list.
    async fn reactions_for(
        &self,
        message_ids: &[MessageId],
        viewer: UserId,
    ) -> Result<HashMap<MessageId, Vec<ReactionTally>>, RepositoryError>;
}
