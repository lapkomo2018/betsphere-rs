use std::sync::Arc;

use domain::entities::{ChatMessage, UserId};
use domain::repositories::{ChatMessageRepository, UserRepository};
use domain::value_objects::chat::MessageBody;

use super::ChatMessageView;
use crate::ApplicationError;

/// Persists a new global-chat message from `author_id` and returns it enriched
/// with the author's current profile.
pub struct PostMessage {
    messages: Arc<dyn ChatMessageRepository>,
    users: Arc<dyn UserRepository>,
}

impl PostMessage {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self { messages, users }
    }

    pub async fn execute(
        &self,
        author_id: UserId,
        body: impl Into<String>,
    ) -> Result<ChatMessageView, ApplicationError> {
        let author = self
            .users
            .find_by_id(author_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {author_id}")))?;

        let body = MessageBody::new(body)?;
        let message = ChatMessage::new(author_id, body);
        self.messages.save(&message).await?;

        Ok(ChatMessageView { message, author })
    }
}
