use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::ChatMessage;
use domain::repositories::{ChatMessageRepository, RepositoryError};

/// Thread-safe in-memory chat store, ordered by insertion. Useful for
/// development and tests.
#[derive(Default)]
pub struct InMemoryChatMessageRepository {
    messages: RwLock<Vec<ChatMessage>>,
}

impl InMemoryChatMessageRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ChatMessageRepository for InMemoryChatMessageRepository {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        self.messages.write().await.push(message.clone());
        Ok(())
    }

    async fn list_recent(&self, limit: i64) -> Result<Vec<ChatMessage>, RepositoryError> {
        let messages = self.messages.read().await;
        let mut recent: Vec<ChatMessage> = messages.clone();
        recent.sort_by_key(|m| m.created_at());
        let limit = limit.max(0) as usize;
        // Keep the newest `limit`, still oldest-first.
        let start = recent.len().saturating_sub(limit);
        Ok(recent.split_off(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::UserId;
    use domain::value_objects::chat::MessageBody;

    fn message(text: &str) -> ChatMessage {
        ChatMessage::new(UserId::new(), MessageBody::new(text).unwrap())
    }

    #[tokio::test]
    async fn list_recent_returns_newest_limited_oldest_first() {
        let repo = InMemoryChatMessageRepository::new();
        for text in ["a", "b", "c"] {
            repo.save(&message(text)).await.unwrap();
        }

        let recent = repo.list_recent(2).await.unwrap();
        let bodies: Vec<&str> = recent.iter().map(|m| m.body().as_str()).collect();
        assert_eq!(bodies, ["b", "c"]);
    }
}
