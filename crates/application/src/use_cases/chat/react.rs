use std::sync::Arc;

use domain::entities::{MessageId, UserId};
use domain::repositories::ChatMessageRepository;
use domain::value_objects::chat::ReactionEmoji;

use crate::ApplicationError;

/// Adds and takes back one user's emoji reactions on a chat message.
///
/// Both directions are idempotent: reacting with an emoji already held, or
/// taking back one that isn't, succeeds and changes nothing. A client retrying
/// a frame it isn't sure landed is the normal case over a socket, and it should
/// not have to distinguish that from a real failure.
pub struct ReactToMessage {
    messages: Arc<dyn ChatMessageRepository>,
}

impl ReactToMessage {
    pub fn new(messages: Arc<dyn ChatMessageRepository>) -> Self {
        Self { messages }
    }

    /// Returns whether the reaction was newly added.
    pub async fn add(
        &self,
        user_id: UserId,
        message_id: MessageId,
        emoji: impl Into<String>,
    ) -> Result<bool, ApplicationError> {
        let emoji = self.resolve(message_id, emoji).await?;
        Ok(self
            .messages
            .add_reaction(message_id, user_id, &emoji)
            .await?)
    }

    /// Returns whether a reaction was there to take back.
    pub async fn remove(
        &self,
        user_id: UserId,
        message_id: MessageId,
        emoji: impl Into<String>,
    ) -> Result<bool, ApplicationError> {
        let emoji = self.resolve(message_id, emoji).await?;
        Ok(self
            .messages
            .remove_reaction(message_id, user_id, &emoji)
            .await?)
    }

    /// Validates the emoji and the message it is aimed at. The existence check
    /// is what turns a reaction to a deleted or made-up message into a 404
    /// rather than a row pointing nowhere.
    async fn resolve(
        &self,
        message_id: MessageId,
        emoji: impl Into<String>,
    ) -> Result<ReactionEmoji, ApplicationError> {
        let emoji = ReactionEmoji::new(emoji)?;
        self.messages.find_by_id(message_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("message {}", message_id.as_uuid()))
        })?;
        Ok(emoji)
    }
}
