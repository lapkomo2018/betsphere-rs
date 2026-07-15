use std::collections::HashMap;
use std::sync::Arc;

use domain::entities::{ChatChannel, User, UserId};
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};

use super::ChatMessageView;
use crate::ApplicationError;

/// Loads one channel's most recent chat messages (oldest-first) enriched with
/// each author's current profile.
pub struct ListRecentMessages {
    messages: Arc<dyn ChatMessageRepository>,
    users: Arc<dyn UserRepository>,
    markets: Arc<dyn MarketRepository>,
}

impl ListRecentMessages {
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
        channel: ChatChannel,
        limit: i64,
    ) -> Result<Vec<ChatMessageView>, ApplicationError> {
        // Reading a market room that doesn't exist is a 404, not an empty list.
        if let ChatChannel::Market(market_id) = channel {
            self.markets
                .find_by_id(market_id)
                .await?
                .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;
        }

        let messages = self.messages.list_recent(channel, limit).await?;

        // Resolve each distinct author once; the user repository is cached, so
        // this stays cheap even when many messages share an author.
        let mut authors: HashMap<UserId, User> = HashMap::new();
        let mut views = Vec::with_capacity(messages.len());
        for message in messages {
            let author = match authors.get(&message.author_id()) {
                Some(author) => author.clone(),
                None => {
                    // Skip messages whose author has since been deleted.
                    let Some(author) = self.users.find_by_id(message.author_id()).await? else {
                        continue;
                    };
                    authors.insert(message.author_id(), author.clone());
                    author
                }
            };
            views.push(ChatMessageView { message, author });
        }

        Ok(views)
    }
}
