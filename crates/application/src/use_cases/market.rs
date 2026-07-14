mod create_market;
mod get_featured_market;
mod get_market;
mod get_price_history;
mod list_markets;
mod resolve_market;

pub use create_market::{CreateMarket, NewMarket};
pub use get_featured_market::GetFeaturedMarket;
pub use get_market::GetMarket;
pub use get_price_history::GetPriceHistory;
pub use list_markets::ListMarkets;
pub use resolve_market::ResolveMarket;

use domain::entities::{Market, Outcome};

/// A market paired with its outcomes, ready for presentation. Outcomes are
/// ordered as stored (creation order).
pub struct MarketView {
    pub market: Market,
    pub outcomes: Vec<Outcome>,
}
