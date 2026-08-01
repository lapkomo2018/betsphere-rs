use crate::ports::Broadcast;
use chrono::{DateTime, Utc};
use domain::entities::{BetId, MarketId, OutcomeId, UserId};
use domain::value_objects::market::Price;
use serde::{Deserialize, Serialize};

/// Which live bet feed a [`BetPlacedBroadcast`] is destined for. Every bet goes
/// out on both: its own market's feed and the cross-market one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetFeed {
    /// Every market's bets, in commit order.
    Global,
    /// One market's bets.
    Market(MarketId),
}

/// A bet committed on a market, broadcast to the live bet feeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetPlacedBroadcast {
    pub id: BetId,
    pub user_id: UserId,
    /// Which market the bet was placed on. Redundant on a market feed, but the
    /// global feed mixes markets, so subscribers there need it.
    pub market_id: MarketId,
    pub outcome_id: OutcomeId,
    pub amount: i64,
    pub price: Price,
    pub created_at: DateTime<Utc>,
}

impl Broadcast for BetPlacedBroadcast {
    type Scope = BetFeed;

    /// One bet feed. Deliberately identical to the wire channel name WS
    /// clients subscribe with.
    fn channel(feed: &BetFeed) -> String {
        match feed {
            BetFeed::Global => "global_bets".to_owned(),
            BetFeed::Market(market_id) => format!("market_bets:{market_id}"),
        }
    }
}
