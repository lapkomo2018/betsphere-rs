use crate::ports::Broadcast;
use chrono::{DateTime, Utc};
use domain::entities::{MarketId, OutcomeId};
use domain::value_objects::market::Price;
use serde::{Deserialize, Serialize};

/// One outcome's price at a moment in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTick {
    pub outcome_id: OutcomeId,
    pub price: Price,
    pub recorded_at: DateTime<Utc>,
}

/// One market's recalculated prices, broadcast whenever a bet moves them —
/// a bet moves every outcome of its market, so the batch covers all of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdateBroadcast {
    pub ticks: Vec<PriceTick>,
}

impl Broadcast for PriceUpdateBroadcast {
    type Scope = MarketId;

    /// One market's live feed. Deliberately identical to the wire channel
    /// name WS clients subscribe with.
    fn channel(market_id: &MarketId) -> String {
        format!("market:{market_id}")
    }
}
