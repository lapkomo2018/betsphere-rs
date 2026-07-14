use std::sync::Arc;

use domain::repositories::MarketRepository;

use super::MarketView;
use crate::ApplicationError;

/// Loads the single most popular market (with its outcomes) for the homepage.
pub struct GetFeaturedMarket {
    markets: Arc<dyn MarketRepository>,
}

impl GetFeaturedMarket {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(&self) -> Result<MarketView, ApplicationError> {
        let market = self
            .markets
            .featured()
            .await?
            .ok_or_else(|| ApplicationError::NotFound("featured market".into()))?;
        let outcomes = self.markets.outcomes_for(market.id()).await?;
        Ok(MarketView { market, outcomes })
    }
}
