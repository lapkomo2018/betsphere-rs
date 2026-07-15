use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::{ChatChannel, ChatMessage, MessageId};

/// Port for chat message persistence. Implementations live in the
/// infrastructure layer.
#[async_trait]
pub trait ChatMessageRepository: Send + Sync {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: MessageId) -> Result<Option<ChatMessage>, RepositoryError>;

    /// The most recent `limit` messages of one channel, returned oldest-first
    /// so callers can render them in chronological order.
    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, RepositoryError>;
}
