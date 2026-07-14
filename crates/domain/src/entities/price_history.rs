use chrono::{DateTime, Utc};

use crate::entities::OutcomeId;
use crate::value_objects::market::Price;

/// A single recorded price for one outcome at a point in time — the raw
/// material for the market's price chart. The database assigns a `BIGSERIAL`
/// primary key that never surfaces in the domain.
#[derive(Debug, Clone)]
pub struct PricePoint {
    outcome_id: OutcomeId,
    price: Price,
    recorded_at: DateTime<Utc>,
}

impl PricePoint {
    /// Records `price` for `outcome_id` as of now.
    pub fn new(outcome_id: OutcomeId, price: Price) -> Self {
        Self {
            outcome_id,
            price,
            recorded_at: Utc::now(),
        }
    }

    /// Reconstructs a point from persisted state. Only repositories should call this.
    pub fn from_parts(outcome_id: OutcomeId, price: Price, recorded_at: DateTime<Utc>) -> Self {
        Self {
            outcome_id,
            price,
            recorded_at,
        }
    }

    pub fn outcome_id(&self) -> OutcomeId {
        self.outcome_id
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn recorded_at(&self) -> DateTime<Utc> {
        self.recorded_at
    }
}
