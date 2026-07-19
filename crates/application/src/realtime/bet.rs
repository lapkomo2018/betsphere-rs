use crate::ports::Broadcast;
use chrono::{DateTime, Utc};
use domain::entities::{BetId, MarketId, OutcomeId, UserId};
use domain::value_objects::market::Price;
use serde::{Deserialize, Serialize};

/// A bet committed on a market, broadcast to its live bet feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetPlacedBroadcast {
    pub id: BetId,
    pub user_id: UserId,
    pub outcome_id: OutcomeId,
    pub amount: i64,
    pub price: Price,
    pub created_at: DateTime<Utc>,
}

impl Broadcast for BetPlacedBroadcast {
    type Scope = MarketId;

    /// One market's bet feed. Deliberately identical to the wire channel
    /// name WS clients subscribe with.
    fn channel(market_id: &MarketId) -> String {
        format!("market_bets:{market_id}")
    }
}
