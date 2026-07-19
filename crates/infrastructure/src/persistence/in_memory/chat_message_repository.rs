use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::{ChatChannel, ChatMessage, MessageId};
use domain::events::ChatMessagePosted;
use domain::repositories::{ChatMessageRepository, MessageCursor, RepositoryError};

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
        cursor: Option<MessageCursor>,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        let messages = self.messages.read().await;
        let mut recent: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| m.channel() == channel)
            .cloned()
            .collect();
        // Same ordering key as Postgres: the id breaks timestamp ties so a
        // cursor lands between two adjacent messages, never on both.
        recent.sort_by_key(|m| (m.created_at(), m.id().as_uuid()));

        // Trim to the side of the anchor being paged towards.
        if let Some(cursor) = cursor {
            let (MessageCursor::Before(anchor) | MessageCursor::After(anchor)) = cursor;
            let key = (anchor.created_at, anchor.id.as_uuid());
            let split = recent.partition_point(|m| (m.created_at(), m.id().as_uuid()) < key);
            match cursor {
                MessageCursor::Before(_) => recent.truncate(split),
                // Drop everything up to and including the anchor itself.
                MessageCursor::After(_) => {
                    let anchor_present = recent.get(split).is_some_and(|m| m.id() == anchor.id);
                    let after = if anchor_present { split + 1 } else { split };
                    recent.drain(..after);
                }
            }
        }

        let limit = limit.max(0) as usize;
        Ok(match cursor {
            // Paging forward takes the messages nearest the anchor; every
            // other case keeps the newest. Both stay oldest-first.
            Some(MessageCursor::After(_)) => {
                recent.truncate(limit);
                recent
            }
            _ => {
                let start = recent.len().saturating_sub(limit);
                recent.split_off(start)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::entities::{MarketId, MessageId, UserId};
    use domain::repositories::MessageAnchor;
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

        let recent = repo
            .list_recent(ChatChannel::Global, 2, None)
            .await
            .unwrap();
        let bodies: Vec<&str> = recent.iter().map(|m| m.body().as_str()).collect();
        assert_eq!(bodies, ["b", "c"]);
    }

    async fn global_messages(repo: &InMemoryChatMessageRepository) -> Vec<ChatMessage> {
        repo.list_recent(ChatChannel::Global, 100, None)
            .await
            .unwrap()
    }

    fn bodies(messages: &[ChatMessage]) -> Vec<&str> {
        messages.iter().map(|m| m.body().as_str()).collect()
    }

    #[tokio::test]
    async fn before_cursor_walks_backwards_from_the_anchor() {
        let repo = InMemoryChatMessageRepository::new();
        for text in ["a", "b", "c", "d", "e"] {
            repo.save(&message(ChatChannel::Global, text))
                .await
                .unwrap();
        }
        let all = global_messages(&repo).await;
        let anchor = MessageAnchor::from(&all[3]); // "d"

        let page = repo
            .list_recent(ChatChannel::Global, 2, Some(MessageCursor::Before(anchor)))
            .await
            .unwrap();
        // The two nearest older messages, still oldest-first, anchor excluded.
        assert_eq!(bodies(&page), ["b", "c"]);
    }

    #[tokio::test]
    async fn after_cursor_walks_forwards_from_the_anchor() {
        let repo = InMemoryChatMessageRepository::new();
        for text in ["a", "b", "c", "d", "e"] {
            repo.save(&message(ChatChannel::Global, text))
                .await
                .unwrap();
        }
        let all = global_messages(&repo).await;
        let anchor = MessageAnchor::from(&all[1]); // "b"

        let page = repo
            .list_recent(ChatChannel::Global, 2, Some(MessageCursor::After(anchor)))
            .await
            .unwrap();
        // The messages nearest the anchor, not the newest in the room.
        assert_eq!(bodies(&page), ["c", "d"]);
    }

    #[tokio::test]
    async fn cursors_at_the_ends_return_nothing() {
        let repo = InMemoryChatMessageRepository::new();
        for text in ["a", "b"] {
            repo.save(&message(ChatChannel::Global, text))
                .await
                .unwrap();
        }
        let all = global_messages(&repo).await;

        let before_oldest = repo
            .list_recent(
                ChatChannel::Global,
                10,
                Some(MessageCursor::Before(MessageAnchor::from(&all[0]))),
            )
            .await
            .unwrap();
        assert!(before_oldest.is_empty());

        let after_newest = repo
            .list_recent(
                ChatChannel::Global,
                10,
                Some(MessageCursor::After(MessageAnchor::from(&all[1]))),
            )
            .await
            .unwrap();
        assert!(after_newest.is_empty());
    }

    #[tokio::test]
    async fn cursors_break_ties_on_identical_timestamps() {
        // Messages created in the same instant must still page cleanly: the
        // id decides the order, so neither side repeats or drops the anchor.
        let repo = InMemoryChatMessageRepository::new();
        let created_at = Utc::now();
        for _ in 0..4 {
            let m = ChatMessage::from_parts(
                MessageId::new(),
                UserId::new(),
                ChatChannel::Global,
                MessageBody::new("same instant").unwrap(),
                created_at,
            );
            repo.save(&m).await.unwrap();
        }
        let all = global_messages(&repo).await;
        let anchor = MessageAnchor::from(&all[1]);

        let before = repo
            .list_recent(ChatChannel::Global, 10, Some(MessageCursor::Before(anchor)))
            .await
            .unwrap();
        let after = repo
            .list_recent(ChatChannel::Global, 10, Some(MessageCursor::After(anchor)))
            .await
            .unwrap();

        let ids = |page: &[ChatMessage]| page.iter().map(|m| m.id()).collect::<Vec<_>>();
        assert_eq!(ids(&before), [all[0].id()]);
        assert_eq!(ids(&after), [all[2].id(), all[3].id()]);
    }

    #[tokio::test]
    async fn list_recent_scopes_to_the_requested_channel() {
        let repo = InMemoryChatMessageRepository::new();
        let market = ChatChannel::Market(MarketId::new());
        repo.save(&message(ChatChannel::Global, "global"))
            .await
            .unwrap();
        repo.save(&message(market, "market")).await.unwrap();

        let global = repo
            .list_recent(ChatChannel::Global, 10, None)
            .await
            .unwrap();
        assert_eq!(global.len(), 1);
        assert_eq!(global[0].body().as_str(), "global");

        let scoped = repo.list_recent(market, 10, None).await.unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].body().as_str(), "market");
    }
}
