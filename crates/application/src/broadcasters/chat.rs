use std::sync::Arc;

use async_trait::async_trait;

use crate::ports::{EventHandler, MessageBroker, MessageBrokerExt};
use crate::realtime::{ChatAuthor, ChatMessageBroadcast, ChatReactionBroadcast, QuotedMessage};
use domain::entities::{ChatMessage, User};
use domain::events::{ChatMessagePosted, ChatReactionChanged};
use domain::repositories::{ChatMessageRepository, UserRepository};
use domain::value_objects::chat::ReactionEmoji;

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

    /// The quoted message to carry with a reply, or `None` if it (or its
    /// author) is gone — the reply is still worth delivering unquoted.
    async fn quoted(&self, message: &ChatMessage) -> Result<Option<QuotedMessage>, String> {
        let Some(id) = message.reply_to() else {
            return Ok(None);
        };
        let Some(quoted) = self
            .messages
            .find_by_id(id)
            .await
            .map_err(|e| format!("could not load quoted chat message {id}: {e}"))?
        else {
            return Ok(None);
        };
        let Some(author) = self
            .users
            .find_by_id(quoted.author_id())
            .await
            .map_err(|e| format!("could not load author of quoted chat message {id}: {e}"))?
        else {
            return Ok(None);
        };

        Ok(Some(QuotedMessage {
            id: quoted.id(),
            author: chat_author(&author),
            body: quoted.body().as_str().to_owned(),
        }))
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
            author: chat_author(&author),
            body: message.body().as_str().to_owned(),
            reply_to: self.quoted(&message).await?,
            created_at: message.created_at(),
        };
        self.broker
            .broadcast(&message.channel(), &broadcast)
            .await
            .map_err(|e| format!("could not broadcast chat message {message_id}: {e}"))
    }
}

/// Fans a reaction change out to the live subscribers of the room the reacted-to
/// message sits in. Like [`ChatMessageBroadcaster`], the event names only what
/// changed and the resulting tally is re-read here, so a redelivered event
/// publishes the count as it actually stands rather than double-counting.
pub struct ChatReactionBroadcaster {
    messages: Arc<dyn ChatMessageRepository>,
    broker: Arc<dyn MessageBroker>,
}

impl ChatReactionBroadcaster {
    pub fn new(messages: Arc<dyn ChatMessageRepository>, broker: Arc<dyn MessageBroker>) -> Self {
        Self { messages, broker }
    }
}

#[async_trait]
impl EventHandler<ChatReactionChanged> for ChatReactionBroadcaster {
    async fn handle(&self, event: &ChatReactionChanged) -> Result<(), String> {
        let message_id = event.message_id;

        let Some(message) = self
            .messages
            .find_by_id(message_id)
            .await
            .map_err(|e| format!("could not load chat message {message_id}: {e}"))?
        else {
            // Deleting a message takes its reactions with it, so there is no
            // room left to broadcast the change to.
            tracing::warn!("chat message {message_id} vanished before its reaction broadcast");
            return Ok(());
        };
        // The emoji went through this same validation on the way in, so it
        // failing here means the stored value was corrupted.
        let emoji = ReactionEmoji::new(&event.emoji)
            .map_err(|e| format!("unusable emoji on chat message {message_id}: {e}"))?;

        // The reader this is tallied for is irrelevant — only the count is
        // published, and each client applies `added` to its own flag.
        let count = self
            .messages
            .reactions_for(&[message_id], event.user_id)
            .await
            .map_err(|e| format!("could not tally reactions of chat message {message_id}: {e}"))?
            .get(&message_id)
            .and_then(|tallies| tallies.iter().find(|tally| tally.emoji == emoji))
            .map_or(0, |tally| tally.count);

        let broadcast = ChatReactionBroadcast {
            message_id,
            emoji: event.emoji.clone(),
            count,
            user_id: event.user_id,
            added: event.added,
        };
        self.broker
            .broadcast(&message.channel(), &broadcast)
            .await
            .map_err(|e| format!("could not broadcast reaction on chat message {message_id}: {e}"))
    }
}

fn chat_author(author: &User) -> ChatAuthor {
    ChatAuthor {
        id: author.id(),
        username: author.username().clone(),
        avatar_url: author.avatar_url().map(str::to_owned),
    }
}
