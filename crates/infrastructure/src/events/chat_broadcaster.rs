use std::sync::Arc;

use async_trait::async_trait;

use application::ports::{MessageBroker, MessageBrokerExt};
use application::realtime::{ChatAuthor, ChatMessageBroadcast};
use domain::events::ChatMessagePosted;
use domain::repositories::{ChatMessageRepository, UserRepository};

use super::EventHandler;

/// Fans a posted chat message out to live WebSocket subscribers: on each
/// `ChatMessagePosted` event, loads the message and its author's current
/// profile and publishes them to the room's broker channel. The event carries
/// only the message id; re-reading at delivery time keeps the handler
/// idempotent (messages are immutable) and broadcasts the author's freshest
/// profile, as at-least-once delivery requires.
pub struct ChatMessageBroadcaster {
    messages: Arc<dyn ChatMessageRepository>,
    users: Arc<dyn UserRepository>,
    broker: Arc<dyn MessageBroker>,
}

impl ChatMessageBroadcaster {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        broker: Arc<dyn MessageBroker>,
    ) -> Self {
        Self {
            messages,
            users,
            broker,
        }
    }
}

#[async_trait]
impl EventHandler<ChatMessagePosted> for ChatMessageBroadcaster {
    async fn handle(&self, event: &ChatMessagePosted) -> Result<(), String> {
        let message_id = event.message_id;

        let Some(message) = self
            .messages
            .find_by_id(message_id)
            .await
            .map_err(|e| format!("could not load chat message {message_id}: {e}"))?
        else {
            // The message existed when the event committed; it can only be
            // absent now if it was since deleted. Nothing left to broadcast.
            tracing::warn!("chat message {message_id} vanished before broadcast");
            return Ok(());
        };
        let Some(author) = self
            .users
            .find_by_id(message.author_id())
            .await
            .map_err(|e| format!("could not load author of chat message {message_id}: {e}"))?
        else {
            tracing::warn!("author of chat message {message_id} vanished before broadcast");
            return Ok(());
        };

        let broadcast = ChatMessageBroadcast {
            id: message.id().as_uuid(),
            author: ChatAuthor {
                id: author.id().as_uuid(),
                username: author.username().to_string(),
                avatar_url: author.avatar_url().map(str::to_owned),
            },
            body: message.body().as_str().to_owned(),
            created_at: message.created_at(),
        };
        self.broker
            .broadcast(&message.channel(), &broadcast)
            .await
            .map_err(|e| format!("could not broadcast chat message {message_id}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use domain::entities::{ChatChannel, ChatMessage, MessageId, User};
    use domain::value_objects::chat::MessageBody;
    use domain::value_objects::user::{Email, PasswordHash, Username};

    use super::*;
    use crate::messaging::InMemoryMessageBroker;
    use crate::persistence::in_memory::{InMemoryChatMessageRepository, InMemoryUserRepository};

    #[tokio::test]
    async fn broadcasts_the_message_with_its_author() {
        let messages = Arc::new(InMemoryChatMessageRepository::new());
        let users = Arc::new(InMemoryUserRepository::new());
        let broker = Arc::new(InMemoryMessageBroker::new());

        let author = User::new(
            Username::new("alice").unwrap(),
            Email::new("alice@example.com").unwrap(),
            PasswordHash::new("$argon2id$fake"),
        );
        users.save(&author).await.unwrap();
        let message = ChatMessage::new(
            author.id(),
            ChatChannel::Global,
            MessageBody::new("hello").unwrap(),
        );
        messages.save(&message).await.unwrap();

        let mut feed = broker
            .subscribe_broadcast::<ChatMessageBroadcast>(&ChatChannel::Global)
            .await
            .unwrap();

        let handler = ChatMessageBroadcaster::new(messages, users, broker.clone());
        handler
            .handle(&ChatMessagePosted {
                message_id: message.id(),
            })
            .await
            .unwrap();

        let broadcast = feed.next().await.unwrap();
        assert_eq!(broadcast.id, message.id().as_uuid());
        assert_eq!(broadcast.body, "hello");
        assert_eq!(broadcast.author.username, "alice");
    }

    #[tokio::test]
    async fn vanished_message_is_a_no_op() {
        let handler = ChatMessageBroadcaster::new(
            Arc::new(InMemoryChatMessageRepository::new()),
            Arc::new(InMemoryUserRepository::new()),
            Arc::new(InMemoryMessageBroker::new()),
        );
        handler
            .handle(&ChatMessagePosted {
                message_id: MessageId::new(),
            })
            .await
            .unwrap();
    }
}
