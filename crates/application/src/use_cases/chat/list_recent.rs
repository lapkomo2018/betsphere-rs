use std::collections::HashMap;
use std::sync::Arc;

use domain::DomainError;
use domain::entities::{ChatChannel, ChatMessage, MessageId, User, UserId};
use domain::repositories::{
    ChatMessageRepository, MarketRepository, MessageAnchor, MessageCursor, UserRepository,
};

use super::{ChatMessageView, RepliedMessage};
use crate::ApplicationError;

/// Which page of a channel's history to load, relative to a message the client
/// already holds. Both directions exclude the anchor itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct HistoryWindow {
    pub before: Option<MessageId>,
    pub after: Option<MessageId>,
}

/// Loads one page of a channel's chat messages (oldest-first) enriched with
/// each author's current profile, the message each reply quotes, and the
/// reactions as they stand for the reader.
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

    /// `viewer` is whose reactions the page is tallied against — the `reacted`
    /// flag is per-reader, so a page is only correct for the account it was
    /// loaded for.
    pub async fn execute(
        &self,
        viewer: UserId,
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

        // Both of these are one query for the whole page: a room where everyone
        // is replying to the same message, or reacting to it, would otherwise
        // cost a query per row.
        let ids: Vec<MessageId> = messages.iter().map(ChatMessage::id).collect();
        let mut reactions = self.messages.reactions_for(&ids, viewer).await?;
        let quoted = self.quoted_messages(&messages).await?;

        // Resolve each distinct author once; the user repository is cached, so
        // this stays cheap even when many messages share an author.
        let mut authors: HashMap<UserId, User> = HashMap::new();
        let mut views = Vec::with_capacity(messages.len());
        for message in messages {
            // Skip messages whose author has since been deleted.
            let Some(author) = self.author(&mut authors, message.author_id()).await? else {
                continue;
            };
            let reply_to = match message.reply_to().and_then(|id| quoted.get(&id)).cloned() {
                Some(parent) => {
                    self.author(&mut authors, parent.author_id())
                        .await?
                        .map(|author| RepliedMessage {
                            message: parent,
                            author,
                        })
                }
                None => None,
            };
            views.push(ChatMessageView {
                reactions: reactions.remove(&message.id()).unwrap_or_default(),
                message,
                author,
                reply_to,
            });
        }

        Ok(views)
    }

    /// The messages quoted by the replies in `page`, indexed by id. Ones that
    /// have since been deleted are simply absent.
    async fn quoted_messages(
        &self,
        page: &[ChatMessage],
    ) -> Result<HashMap<MessageId, ChatMessage>, ApplicationError> {
        let mut ids: Vec<MessageId> = page.iter().filter_map(ChatMessage::reply_to).collect();
        ids.sort_by_key(MessageId::as_uuid);
        ids.dedup();

        let quoted = self.messages.find_by_ids(&ids).await?;
        Ok(quoted.into_iter().map(|m| (m.id(), m)).collect())
    }

    /// Resolves one author through `cache`, which spans a whole page so an
    /// author appearing on several messages (and on the ones they quote) costs
    /// a single lookup.
    async fn author(
        &self,
        cache: &mut HashMap<UserId, User>,
        id: UserId,
    ) -> Result<Option<User>, ApplicationError> {
        if let Some(author) = cache.get(&id) {
            return Ok(Some(author.clone()));
        }
        let Some(author) = self.users.find_by_id(id).await? else {
            return Ok(None);
        };
        cache.insert(id, author.clone());
        Ok(Some(author))
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
