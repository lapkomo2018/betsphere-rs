use std::sync::Arc;

use domain::entities::MarketId;
use domain::repositories::{BetFilter, BetRepository, MarketRepository, UserRepository};

use super::{BetView, enrich};
use crate::ApplicationError;

/// The bets placed on one market (its detail page).
pub struct ListMarketBets {
    bets: Arc<dyn BetRepository>,
    markets: Arc<dyn MarketRepository>,
    users: Arc<dyn UserRepository>,
}

impl ListMarketBets {
    pub fn new(
        bets: Arc<dyn BetRepository>,
        markets: Arc<dyn MarketRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            bets,
            markets,
            users,
        }
    }

    pub async fn execute(
        &self,
        market_id: MarketId,
        filter: &BetFilter,
    ) -> Result<Vec<BetView>, ApplicationError> {
        if self.markets.find_by_id(market_id).await?.is_none() {
            return Err(ApplicationError::NotFound(format!("market {market_id}")));
        }
        let bets = self.bets.find_by_market(market_id, filter).await?;
        enrich(bets, self.markets.as_ref(), self.users.as_ref()).await
    }
}
