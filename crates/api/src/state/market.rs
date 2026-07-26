use std::sync::Arc;

use application::ports::FileStorage;
use application::use_cases::market::{
    CreateMarket, GetFeaturedMarket, GetMarket, GetPriceHistory, ListMarkets, ResolveMarket,
    UploadMarketThumbnail, UploadOutcomeThumbnail,
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
    pub upload_thumbnail: Arc<UploadMarketThumbnail>,
    pub upload_outcome_thumbnail: Arc<UploadOutcomeThumbnail>,
}

impl MarketState {
    pub fn new(
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        storage: Arc<dyn FileStorage>,
    ) -> Self {
        Self {
            create: Arc::new(CreateMarket::new(markets.clone())),
            list: Arc::new(ListMarkets::new(markets.clone())),
            get: Arc::new(GetMarket::new(markets.clone())),
            featured: Arc::new(GetFeaturedMarket::new(markets.clone())),
            price_history: Arc::new(GetPriceHistory::new(markets.clone())),
            resolve: Arc::new(ResolveMarket::new(markets.clone(), bets)),
            upload_thumbnail: Arc::new(UploadMarketThumbnail::new(
                markets.clone(),
                storage.clone(),
            )),
            upload_outcome_thumbnail: Arc::new(UploadOutcomeThumbnail::new(markets, storage)),
        }
    }
}
