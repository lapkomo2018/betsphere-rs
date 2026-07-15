use std::sync::Arc;

use domain::entities::{ChatChannel, ChatMessage, UserId};
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};
use domain::value_objects::chat::MessageBody;

use super::ChatMessageView;
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

    pub async fn execute(
        &self,
        author_id: UserId,
        channel: ChatChannel,
        body: impl Into<String>,
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

        let body = MessageBody::new(body)?;
        let message = ChatMessage::new(author_id, channel, body);
        self.messages.save(&message).await?;

        Ok(ChatMessageView { message, author })
    }
}
