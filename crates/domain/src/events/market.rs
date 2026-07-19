use serde::{Deserialize, Serialize};

use crate::entities::MarketId;

use super::Event;

/// A market's outcome prices were recalculated (a bet moved the volumes).
/// Carries only the market id: subscribers read the prices current at
/// delivery time, so a redelivered event can never resurrect a stale price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketPricesUpdated {
    pub market_id: MarketId,
}

impl Event for MarketPricesUpdated {
    const TOPIC: &'static str = "market.prices_updated";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::tests::round_trips;

    #[test]
    fn round_trips_through_serde() {
        round_trips(MarketPricesUpdated {
            market_id: MarketId::new(),
        });
    }
}
