use crate::ports::Broadcast;
use chrono::{DateTime, Utc};
use domain::entities::{ChatChannel, MessageId, UserId};
use domain::value_objects::user::Username;
use serde::{Deserialize, Serialize};

/// The author fields carried by a [`ChatMessageBroadcast`] (public profile
/// only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAuthor {
    pub id: UserId,
    pub username: Username,
    pub avatar_url: Option<String>,
}

/// The message a reply quotes, carried along with it so a client can render
/// the quoted line without going back for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotedMessage {
    pub id: MessageId,
    pub author: ChatAuthor,
    pub body: String,
}

/// A chat message enriched with its author's profile, broadcast when posted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageBroadcast {
    pub id: MessageId,
    pub author: ChatAuthor,
    pub body: String,
    /// Set when the message is a reply. Absent both for ordinary messages and
    /// for replies whose quoted message is gone.
    pub reply_to: Option<QuotedMessage>,
    pub created_at: DateTime<Utc>,
}

impl Broadcast for ChatMessageBroadcast {
    type Scope = ChatChannel;

    /// One chat room's live messages, grouped under `chat:` in the broker's
    /// shared namespace.
    fn channel(channel: &ChatChannel) -> String {
        match channel.market_id() {
            None => "chat:global".to_owned(),
            Some(id) => format!("chat:market:{id}"),
        }
    }
}

/// A reaction added to, or taken back from, a message in a chat room.
///
/// Carries the resulting count rather than a delta: at-least-once delivery
/// means a subscriber may see this twice, and a count it can assign is
/// idempotent where `+1` would not be.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReactionBroadcast {
    pub message_id: MessageId,
    pub emoji: String,
    /// How many users hold this emoji on the message now. Zero means the last
    /// one was just taken back and the emoji should disappear from the row.
    pub count: i64,
    /// Whose reaction moved, and which way. History is tallied per reader, so
    /// this is how a client keeps its own "I reacted" flag in step without
    /// refetching the page.
    pub user_id: UserId,
    pub added: bool,
}

impl Broadcast for ChatReactionBroadcast {
    type Scope = ChatChannel;

    /// One chat room's live reaction changes. A channel of their own rather
    /// than a variant on the message channel: the two payloads are unrelated
    /// shapes, and [`Broadcast`] is what keeps a channel from carrying more
    /// than one of them.
    fn channel(channel: &ChatChannel) -> String {
        match channel.market_id() {
            None => "chat_reactions:global".to_owned(),
            Some(id) => format!("chat_reactions:market:{id}"),
        }
    }
}
