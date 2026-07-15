use std::sync::Arc;

use application::ports::{AccessTokenService, MessageBroker};
use application::use_cases::chat::{ListRecentMessages, PostMessage};
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};

/// How many recent messages to replay to a client on connect.
pub const HISTORY_LIMIT: i64 = 50;

#[derive(Clone)]
pub struct ChatState {
    pub post_message: Arc<PostMessage>,
    pub list_recent: Arc<ListRecentMessages>,
    /// Verifies the access token passed as a WebSocket query parameter, since
    /// browsers cannot set the `Authorization` header on the WS handshake.
    pub access_tokens: Arc<dyn AccessTokenService>,
    /// Shared cross-instance pub/sub. Keeping fan-out behind a broker (Redis
    /// Pub/Sub in production) is what makes the WebSocket layer stateless: no
    /// messages are buffered in this process, so any instance can serve any
    /// client.
    pub broker: Arc<dyn MessageBroker>,
}

impl ChatState {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        markets: Arc<dyn MarketRepository>,
        access_tokens: Arc<dyn AccessTokenService>,
        broker: Arc<dyn MessageBroker>,
    ) -> Self {
        Self {
            post_message: Arc::new(PostMessage::new(
                messages.clone(),
                users.clone(),
                markets.clone(),
            )),
            list_recent: Arc::new(ListRecentMessages::new(messages, users, markets)),
            access_tokens,
            broker,
        }
    }
}
