use std::collections::HashMap;
use std::sync::Arc;

use domain::entities::{User, UserId};
use domain::repositories::{ChatMessageRepository, UserRepository};

use super::ChatMessageView;
use crate::ApplicationError;

/// Loads the most recent chat messages (oldest-first) enriched with each
/// author's current profile.
pub struct ListRecentMessages {
    messages: Arc<dyn ChatMessageRepository>,
    users: Arc<dyn UserRepository>,
}

impl ListRecentMessages {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self { messages, users }
    }

    pub async fn execute(&self, limit: i64) -> Result<Vec<ChatMessageView>, ApplicationError> {
        let messages = self.messages.list_recent(limit).await?;

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
