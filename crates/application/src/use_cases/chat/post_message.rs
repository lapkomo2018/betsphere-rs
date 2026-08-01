use std::sync::Arc;

use domain::entities::{ChatChannel, ChatMessage, MessageId, UserId};
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};
use domain::value_objects::chat::MessageBody;

use super::{ChatMessageView, RepliedMessage};
use crate::ApplicationError;

/// Persists a new chat message from `author_id` into one channel and returns
/// it enriched with the author's current profile.
pub struct PostMessage {
    messages: Arc<dyn ChatMessageRepository>,
    users: Arc<dyn UserRepository>,
    markets: Arc<dyn MarketRepository>,
}

impl PostMessage {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        markets: Arc<dyn MarketRepository>,
    ) -> Self {
        Self {
            messages,
            users,
            markets,
        }
    }

    /// `reply_to`, when given, must name a message of this same `channel`.
    pub async fn execute(
        &self,
        author_id: UserId,
        channel: ChatChannel,
        body: impl Into<String>,
        reply_to: Option<MessageId>,
    ) -> Result<ChatMessageView, ApplicationError> {
        let author = self
            .users
            .find_by_id(author_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {author_id}")))?;

        // A market room only exists as long as its market does.
        if let ChatChannel::Market(market_id) = channel {
            self.markets
                .find_by_id(market_id)
                .await?
                .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;
        }

        let quoted = match reply_to {
            Some(id) => Some(self.quoted(id, channel).await?),
            None => None,
        };

        let body = MessageBody::new(body)?;
        let message = ChatMessage::new(author_id, channel, body, reply_to);
        self.messages.save(&message).await?;

        Ok(ChatMessageView {
            message,
            author,
            reply_to: quoted,
            // Nobody can have reacted to a message that did not exist a moment
            // ago.
            reactions: Vec::new(),
        })
    }

    /// Loads the message a reply quotes, rejecting one from another room:
    /// a reply and its quote are rendered together, so a cross-room quote
    /// would leak a market discussion into the global feed.
    async fn quoted(
        &self,
        id: MessageId,
        channel: ChatChannel,
    ) -> Result<RepliedMessage, ApplicationError> {
        let unknown = || ApplicationError::NotFound(format!("message {}", id.as_uuid()));

        let message = self
            .messages
            .find_by_id(id)
            .await?
            .filter(|message| message.channel() == channel)
            .ok_or_else(unknown)?;
        let author = self
            .users
            .find_by_id(message.author_id())
            .await?
            .ok_or_else(unknown)?;

        Ok(RepliedMessage { message, author })
    }
}
