use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::ChatMessage;

/// Port for chat message persistence. Implementations live in the
/// infrastructure layer.
#[async_trait]
pub trait ChatMessageRepository: Send + Sync {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError>;

    /// The most recent `limit` messages, returned oldest-first so callers can
    /// render them in chronological order.
    async fn list_recent(&self, limit: i64) -> Result<Vec<ChatMessage>, RepositoryError>;
}
