use std::sync::Arc;

use domain::repositories::{BetFilter, BetRepository, MarketRepository, UserRepository};

use super::{BetView, enrich};
use crate::ApplicationError;

/// The public cross-market feed of recently placed (or biggest) bets.
pub struct ListBetFeed {
    bets: Arc<dyn BetRepository>,
    markets: Arc<dyn MarketRepository>,
    users: Arc<dyn UserRepository>,
}

impl ListBetFeed {
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

    pub async fn execute(&self, filter: &BetFilter) -> Result<Vec<BetView>, ApplicationError> {
        let bets = self.bets.feed(filter).await?;
        enrich(bets, self.markets.as_ref(), self.users.as_ref()).await
    }
}
