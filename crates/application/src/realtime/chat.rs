use chrono::{DateTime, Utc};
use domain::entities::ChatChannel;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ports::Broadcast;

/// The author fields carried by a [`ChatMessageBroadcast`] (public profile
/// only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAuthor {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
}

/// A chat message enriched with its author's profile, broadcast when posted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageBroadcast {
    pub id: Uuid,
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
