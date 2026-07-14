use std::collections::HashMap;
use std::sync::Arc;

use domain::entities::{MarketId, Outcome};
use domain::repositories::{MarketFilter, MarketRepository};

use super::MarketView;
use crate::ApplicationError;

/// Lists markets matching a filter, each paired with its outcomes. Outcomes are
/// fetched in a single batched query to avoid an N+1 lookup.
pub struct ListMarkets {
    markets: Arc<dyn MarketRepository>,
}

impl ListMarkets {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(
        &self,
        filter: &MarketFilter,
    ) -> Result<Vec<MarketView>, ApplicationError> {
        let markets = self.markets.list(filter).await?;
        if markets.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<MarketId> = markets.iter().map(|m| m.id()).collect();
        let outcomes = self.markets.outcomes_for_markets(&ids).await?;

        // Group outcomes by market, preserving the order the repository returned.
        let mut by_market: HashMap<MarketId, Vec<Outcome>> = HashMap::new();
        for outcome in outcomes {
            by_market
                .entry(outcome.market_id())
                .or_default()
                .push(outcome);
        }

        Ok(markets
            .into_iter()
            .map(|market| {
                let outcomes = by_market.remove(&market.id()).unwrap_or_default();
                MarketView { market, outcomes }
            })
            .collect())
    }
}
