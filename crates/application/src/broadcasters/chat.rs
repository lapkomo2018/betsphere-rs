use std::sync::Arc;

use async_trait::async_trait;

use crate::ports::{EventHandler, MessageBroker, MessageBrokerExt};
use crate::realtime::{ChatAuthor, ChatMessageBroadcast};
use domain::events::ChatMessagePosted;
use domain::repositories::{ChatMessageRepository, UserRepository};

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
            id: message.id(),
            author: ChatAuthor {
                id: author.id(),
                username: author.username().clone(),
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
