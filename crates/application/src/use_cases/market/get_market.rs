use std::sync::Arc;

use domain::entities::MarketId;
use domain::repositories::MarketRepository;

use super::MarketView;
use crate::ApplicationError;

/// Loads one market with its outcomes.
pub struct GetMarket {
    markets: Arc<dyn MarketRepository>,
}

impl GetMarket {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(&self, id: MarketId) -> Result<MarketView, ApplicationError> {
        let market = self
            .markets
            .find_by_id(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("market {id}")))?;
        let outcomes = self.markets.outcomes_for(id).await?;
        Ok(MarketView { market, outcomes })
    }
}
