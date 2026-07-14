use std::sync::Arc;

use domain::entities::{MarketId, PricePoint};
use domain::repositories::{MarketRepository, PriceHistoryQuery};

use crate::ApplicationError;

/// Loads the price history for a market's outcomes, for charting. Points are
/// returned flat (each carries its `outcome_id`); the caller groups per outcome.
pub struct GetPriceHistory {
    markets: Arc<dyn MarketRepository>,
}

impl GetPriceHistory {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(
        &self,
        market_id: MarketId,
        query: &PriceHistoryQuery,
    ) -> Result<Vec<PricePoint>, ApplicationError> {
        // Confirm the market exists so callers get a 404 rather than an empty
        // series for a bogus id.
        if self.markets.find_by_id(market_id).await?.is_none() {
            return Err(ApplicationError::NotFound(format!("market {market_id}")));
        }
        Ok(self.markets.price_history(market_id, query).await?)
    }
}
