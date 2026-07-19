use std::collections::HashMap;
use std::sync::Arc;

use domain::DomainError;
use domain::entities::{ChatChannel, MessageId, User, UserId};
use domain::repositories::{
    ChatMessageRepository, MarketRepository, MessageAnchor, MessageCursor, UserRepository,
};

use super::ChatMessageView;
use crate::ApplicationError;

/// Which page of a channel's history to load, relative to a message the client
/// already holds. Both directions exclude the anchor itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct HistoryWindow {
    pub before: Option<MessageId>,
    pub after: Option<MessageId>,
}

/// Loads one page of a channel's chat messages (oldest-first) enriched with
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
        window: HistoryWindow,
    ) -> Result<Vec<ChatMessageView>, ApplicationError> {
        // Reading a market room that doesn't exist is a 404, not an empty list.
        if let ChatChannel::Market(market_id) = channel {
            self.markets
                .find_by_id(market_id)
                .await?
                .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;
        }

        let cursor = self.resolve_cursor(channel, window).await?;
        let messages = self.messages.list_recent(channel, limit, cursor).await?;

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

    /// Turns a client-supplied message id into an ordering anchor, rejecting
    /// ids that don't name a message of this very channel — paging from
    /// another room's message would silently return an arbitrary page.
    async fn resolve_cursor(
        &self,
        channel: ChatChannel,
        window: HistoryWindow,
    ) -> Result<Option<MessageCursor>, ApplicationError> {
        let (id, backwards) = match (window.before, window.after) {
            (None, None) => return Ok(None),
            (Some(before), None) => (before, true),
            (None, Some(after)) => (after, false),
            (Some(_), Some(_)) => {
                return Err(DomainError::Validation(
                    "before_uuid and after_uuid are mutually exclusive".to_owned(),
                )
                .into());
            }
        };

        let message = self
            .messages
            .find_by_id(id)
            .await?
            .filter(|message| message.channel() == channel)
            .ok_or_else(|| ApplicationError::NotFound(format!("message {}", id.as_uuid())))?;

        let anchor = MessageAnchor::from(&message);
        Ok(Some(if backwards {
            MessageCursor::Before(anchor)
        } else {
            MessageCursor::After(anchor)
        }))
    }
}
