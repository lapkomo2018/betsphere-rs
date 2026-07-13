use std::sync::Arc;

use application::ports::AccessTokenService;
use application::use_cases::chat::{ListRecentMessages, PostMessage};
use domain::repositories::{ChatMessageRepository, UserRepository};
use tokio::sync::broadcast;

/// How many recent messages to replay to a client on connect.
pub const HISTORY_LIMIT: i64 = 50;

/// Broadcast buffer size. If a slow client falls this far behind it is
/// disconnected rather than stalling the whole room.
const BROADCAST_CAPACITY: usize = 256;

/// In-memory fan-out for the global chat. Every posted message is serialized
/// once and published to all live WebSocket subscribers. Cloning shares the
/// same underlying channel.
#[derive(Clone)]
pub struct ChatHub {
    tx: broadcast::Sender<String>,
}

impl ChatHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx }
    }

    /// Subscribes a new connection to the live message stream.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Publishes a pre-serialized message to all subscribers. Fails silently
    /// when no one is connected.
    pub fn publish(&self, json: String) {
        let _ = self.tx.send(json);
    }
}

impl Default for ChatHub {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct ChatState {
    pub post_message: Arc<PostMessage>,
    pub list_recent: Arc<ListRecentMessages>,
    /// Verifies the access token passed as a WebSocket query parameter, since
    /// browsers cannot set the `Authorization` header on the WS handshake.
    pub access_tokens: Arc<dyn AccessTokenService>,
    pub hub: ChatHub,
}

impl ChatState {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        access_tokens: Arc<dyn AccessTokenService>,
    ) -> Self {
        Self {
            post_message: Arc::new(PostMessage::new(messages.clone(), users.clone())),
            list_recent: Arc::new(ListRecentMessages::new(messages, users)),
            access_tokens,
            hub: ChatHub::new(),
        }
    }
}
