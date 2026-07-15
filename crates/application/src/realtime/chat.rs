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

/// A chat message enriched with its author's profile, broadcast when posted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageBroadcast {
    pub id: MessageId,
    pub author: ChatAuthor,
    pub body: String,
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
