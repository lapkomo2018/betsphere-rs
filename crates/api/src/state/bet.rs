use std::sync::Arc;

use application::use_cases::bet::{ListBetFeed, ListMarketBets, ListUserBets, PlaceBet};
use domain::repositories::{BetRepository, MarketRepository, UserRepository};

/// Bet use cases. Listings also carry the market and user repositories to
/// join display names onto the bets.
#[derive(Clone)]
pub struct BetState {
    pub place: Arc<PlaceBet>,
    pub feed: Arc<ListBetFeed>,
    pub user_bets: Arc<ListUserBets>,
    pub market_bets: Arc<ListMarketBets>,
}

impl BetState {
    pub fn new(
        bets: Arc<dyn BetRepository>,
        markets: Arc<dyn MarketRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            place: Arc::new(PlaceBet::new(markets.clone(), bets.clone(), users.clone())),
            feed: Arc::new(ListBetFeed::new(
                bets.clone(),
                markets.clone(),
                users.clone(),
            )),
            user_bets: Arc::new(ListUserBets::new(
                bets.clone(),
                markets.clone(),
                users.clone(),
            )),
            market_bets: Arc::new(ListMarketBets::new(bets, markets, users)),
        }
    }
}
