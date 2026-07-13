use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::UserId;
use crate::value_objects::chat::MessageBody;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageId(Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for MessageId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<MessageId> for Uuid {
    fn from(id: MessageId) -> Self {
        id.0
    }
}

/// A single message posted to the global chat.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    id: MessageId,
    author_id: UserId,
    body: MessageBody,
    created_at: DateTime<Utc>,
}

impl ChatMessage {
    /// Creates a brand-new message authored now.
    pub fn new(author_id: UserId, body: MessageBody) -> Self {
        Self {
            id: MessageId::new(),
            author_id,
            body,
            created_at: Utc::now(),
        }
    }

    /// Reconstructs a message from persisted state. Only repositories should call this.
    pub fn from_parts(
        id: MessageId,
        author_id: UserId,
        body: MessageBody,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            author_id,
            body,
            created_at,
        }
    }

    pub fn id(&self) -> MessageId {
        self.id
    }

    pub fn author_id(&self) -> UserId {
        self.author_id
    }

    pub fn body(&self) -> &MessageBody {
        &self.body
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_message_gets_id_and_timestamp() {
        let author = UserId::new();
        let message = ChatMessage::new(author, MessageBody::new("hi").unwrap());
        assert_eq!(message.author_id(), author);
        assert_eq!(message.body().as_str(), "hi");
    }
}
