use std::sync::Arc;

use application::use_cases::chat::ListRecentMessages;
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};

/// How many recent messages to replay to a client, over REST and on a
/// WebSocket chat subscribe alike.
pub const HISTORY_LIMIT: i64 = 50;

/// State of chat's REST surface. Live chat runs over the general WebSocket
/// (see [`super::WsState`]).
#[derive(Clone)]
pub struct ChatState {
    pub list_recent: Arc<ListRecentMessages>,
}

impl ChatState {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        markets: Arc<dyn MarketRepository>,
    ) -> Self {
        Self {
            list_recent: Arc::new(ListRecentMessages::new(messages, users, markets)),
        }
    }
}
