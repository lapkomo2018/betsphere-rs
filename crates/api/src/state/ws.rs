use std::sync::Arc;

use application::ports::{AccessTokenService, MessageBroker};
use application::use_cases::chat::{ListRecentMessages, PostMessage, ReactToMessage};
use domain::repositories::{
    BetRepository, ChatMessageRepository, MarketRepository, UserRepository,
};

/// State of the general WebSocket endpoint, which multiplexes every real-time
/// stream (chat rooms, market feeds) over one socket.
#[derive(Clone)]
pub struct WsState {
    pub post_message: Arc<PostMessage>,
    pub react: Arc<ReactToMessage>,
    pub list_recent: Arc<ListRecentMessages>,
    /// Market lookups for feed subscriptions: the existence check and the
    /// price snapshot sent on subscribe.
    pub markets: Arc<dyn MarketRepository>,
    /// Bet lookups for bet-feed subscriptions: the history sent on subscribe.
    pub bets: Arc<dyn BetRepository>,
    /// Verifies the access token passed as a WebSocket query parameter, since
    /// browsers cannot set the `Authorization` header on the WS handshake.
    pub access_tokens: Arc<dyn AccessTokenService>,
    /// Shared cross-instance pub/sub. Keeping fan-out behind a broker (Redis
    /// Pub/Sub in production) is what makes the WebSocket layer stateless: no
    /// messages are buffered in this process, so any instance can serve any
    /// client.
    pub broker: Arc<dyn MessageBroker>,
}

impl WsState {
    pub fn new(
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        access_tokens: Arc<dyn AccessTokenService>,
        broker: Arc<dyn MessageBroker>,
    ) -> Self {
        Self {
            post_message: Arc::new(PostMessage::new(
                messages.clone(),
                users.clone(),
                markets.clone(),
            )),
            react: Arc::new(ReactToMessage::new(messages.clone())),
            list_recent: Arc::new(ListRecentMessages::new(messages, users, markets.clone())),
            markets,
            bets,
            access_tokens,
            broker,
        }
    }
}
