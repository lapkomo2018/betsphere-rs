use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::{MarketId, UserId};
use crate::value_objects::chat::MessageBody;

/// The room a chat message belongs to: the single global room or the
/// discussion attached to one market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatChannel {
    Global,
    Market(MarketId),
}

impl ChatChannel {
    /// The market this channel is scoped to, or `None` for the global room.
    pub fn market_id(&self) -> Option<MarketId> {
        match self {
            Self::Global => None,
            Self::Market(id) => Some(*id),
        }
    }
}

impl From<Option<MarketId>> for ChatChannel {
    fn from(market_id: Option<MarketId>) -> Self {
        match market_id {
            Some(id) => Self::Market(id),
            None => Self::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

/// A single message posted to one chat room (global or per-market).
#[derive(Debug, Clone)]
pub struct ChatMessage {
    id: MessageId,
    author_id: UserId,
    channel: ChatChannel,
    body: MessageBody,
    /// The message this one replies to, always in the same channel. Clears
    /// itself if that message is ever deleted: the reply survives, unquoted.
    reply_to: Option<MessageId>,
    created_at: DateTime<Utc>,
}

impl ChatMessage {
    /// Creates a brand-new message authored now. Callers are responsible for
    /// checking that `reply_to` names a message of the same `channel`.
    pub fn new(
        author_id: UserId,
        channel: ChatChannel,
        body: MessageBody,
        reply_to: Option<MessageId>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            author_id,
            channel,
            body,
            reply_to,
            created_at: Utc::now(),
        }
    }

    /// Reconstructs a message from persisted state. Only repositories should call this.
    pub fn from_parts(
        id: MessageId,
        author_id: UserId,
        channel: ChatChannel,
        body: MessageBody,
        reply_to: Option<MessageId>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            author_id,
            channel,
            body,
            reply_to,
            created_at,
        }
    }

    pub fn id(&self) -> MessageId {
        self.id
    }

    pub fn author_id(&self) -> UserId {
        self.author_id
    }

    pub fn channel(&self) -> ChatChannel {
        self.channel
    }

    pub fn body(&self) -> &MessageBody {
        &self.body
    }

    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply_to
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
        let message = ChatMessage::new(
            author,
            ChatChannel::Global,
            MessageBody::new("hi").unwrap(),
            None,
        );
        assert_eq!(message.author_id(), author);
        assert_eq!(message.channel(), ChatChannel::Global);
        assert_eq!(message.body().as_str(), "hi");
        assert_eq!(message.reply_to(), None);
    }

    #[test]
    fn a_reply_remembers_the_message_it_answers() {
        let parent = MessageId::new();
        let message = ChatMessage::new(
            UserId::new(),
            ChatChannel::Global,
            MessageBody::new("agreed").unwrap(),
            Some(parent),
        );
        assert_eq!(message.reply_to(), Some(parent));
    }

    #[test]
    fn channel_round_trips_through_market_id() {
        let market = MarketId::new();
        assert_eq!(ChatChannel::Global.market_id(), None);
        assert_eq!(ChatChannel::Market(market).market_id(), Some(market));
        assert_eq!(ChatChannel::from(Some(market)), ChatChannel::Market(market));
        assert_eq!(ChatChannel::from(None), ChatChannel::Global);
    }
}
