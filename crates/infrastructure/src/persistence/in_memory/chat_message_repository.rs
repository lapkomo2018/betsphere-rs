use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::{ChatChannel, ChatMessage, MessageId};
use domain::events::ChatMessagePosted;
use domain::repositories::{ChatMessageRepository, RepositoryError};

use crate::events::InMemoryEventBus;

/// Thread-safe in-memory chat store, ordered by insertion. Useful for
/// development and tests.
#[derive(Default)]
pub struct InMemoryChatMessageRepository {
    messages: RwLock<Vec<ChatMessage>>,
    events: Option<Arc<InMemoryEventBus>>,
}

impl InMemoryChatMessageRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dispatches the events the Postgres implementation records in its
    /// outbox (here synchronously, right after the write) through `events`.
    pub fn with_events(mut self, events: Arc<InMemoryEventBus>) -> Self {
        self.events = Some(events);
        self
    }
}

#[async_trait]
impl ChatMessageRepository for InMemoryChatMessageRepository {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        self.messages.write().await.push(message.clone());
        if let Some(events) = &self.events {
            events
                .dispatch(&ChatMessagePosted {
                    message_id: message.id(),
                })
                .await;
        }
        Ok(())
    }

    async fn find_by_id(&self, id: MessageId) -> Result<Option<ChatMessage>, RepositoryError> {
        Ok(self
            .messages
            .read()
            .await
            .iter()
            .find(|m| m.id() == id)
            .cloned())
    }

    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        let messages = self.messages.read().await;
        let mut recent: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| m.channel() == channel)
            .cloned()
            .collect();
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
    use domain::entities::{MarketId, UserId};
    use domain::value_objects::chat::MessageBody;

    fn message(channel: ChatChannel, text: &str) -> ChatMessage {
        ChatMessage::new(UserId::new(), channel, MessageBody::new(text).unwrap())
    }

    #[tokio::test]
    async fn list_recent_returns_newest_limited_oldest_first() {
        let repo = InMemoryChatMessageRepository::new();
        for text in ["a", "b", "c"] {
            repo.save(&message(ChatChannel::Global, text))
                .await
                .unwrap();
        }

        let recent = repo.list_recent(ChatChannel::Global, 2).await.unwrap();
        let bodies: Vec<&str> = recent.iter().map(|m| m.body().as_str()).collect();
        assert_eq!(bodies, ["b", "c"]);
    }

    #[tokio::test]
    async fn list_recent_scopes_to_the_requested_channel() {
        let repo = InMemoryChatMessageRepository::new();
        let market = ChatChannel::Market(MarketId::new());
        repo.save(&message(ChatChannel::Global, "global"))
            .await
            .unwrap();
        repo.save(&message(market, "market")).await.unwrap();

        let global = repo.list_recent(ChatChannel::Global, 10).await.unwrap();
        assert_eq!(global.len(), 1);
        assert_eq!(global[0].body().as_str(), "global");

        let scoped = repo.list_recent(market, 10).await.unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].body().as_str(), "market");
    }
}
