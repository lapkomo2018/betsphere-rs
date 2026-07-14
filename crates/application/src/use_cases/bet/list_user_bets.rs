use std::sync::Arc;

use domain::entities::UserId;
use domain::repositories::{BetFilter, BetRepository, MarketRepository, UserRepository};

use super::{BetView, enrich};
use crate::ApplicationError;

/// One user's bet history (their profile page). Public, like the profile
/// itself — stakes and results carry no private data.
pub struct ListUserBets {
    bets: Arc<dyn BetRepository>,
    markets: Arc<dyn MarketRepository>,
    users: Arc<dyn UserRepository>,
}

impl ListUserBets {
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
        user_id: UserId,
        filter: &BetFilter,
    ) -> Result<Vec<BetView>, ApplicationError> {
        if self.users.find_by_id(user_id).await?.is_none() {
            return Err(ApplicationError::NotFound(format!("user {user_id}")));
        }
        let bets = self.bets.find_by_user(user_id, filter).await?;
        enrich(bets, self.markets.as_ref(), self.users.as_ref()).await
    }
}
