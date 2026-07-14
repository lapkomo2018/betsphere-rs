use std::sync::Arc;

use application::use_cases::market::{
    CreateMarket, GetFeaturedMarket, GetMarket, GetPriceHistory, ListMarkets, ResolveMarket,
};
use domain::repositories::{BetRepository, MarketRepository};

/// Market use cases, sharing one repository.
#[derive(Clone)]
pub struct MarketState {
    pub create: Arc<CreateMarket>,
    pub list: Arc<ListMarkets>,
    pub get: Arc<GetMarket>,
    pub featured: Arc<GetFeaturedMarket>,
    pub price_history: Arc<GetPriceHistory>,
    pub resolve: Arc<ResolveMarket>,
}

impl MarketState {
    pub fn new(markets: Arc<dyn MarketRepository>, bets: Arc<dyn BetRepository>) -> Self {
        Self {
            create: Arc::new(CreateMarket::new(markets.clone())),
            list: Arc::new(ListMarkets::new(markets.clone())),
            get: Arc::new(GetMarket::new(markets.clone())),
            featured: Arc::new(GetFeaturedMarket::new(markets.clone())),
            price_history: Arc::new(GetPriceHistory::new(markets.clone())),
            resolve: Arc::new(ResolveMarket::new(markets, bets)),
        }
    }
}
