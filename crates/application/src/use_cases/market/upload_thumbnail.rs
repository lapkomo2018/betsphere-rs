use std::sync::Arc;

use domain::entities::MarketId;
use domain::repositories::MarketRepository;
use domain::services::authorization;

use super::MarketView;
use crate::ports::FileStorage;
use crate::use_cases::image;
use crate::{Actor, ApplicationError};

/// Maximum thumbnail size in bytes.
pub const MAX_THUMBNAIL_BYTES: usize = 2 * 1024 * 1024;

/// Storage folder market thumbnails live in.
const FOLDER: &str = "thumbnails";

/// Stores a market's cover image and points its `thumbnail_url` at it. Only
/// actors allowed by [`authorization::can_manage_markets`] may call this.
pub struct UploadMarketThumbnail {
    markets: Arc<dyn MarketRepository>,
    storage: Arc<dyn FileStorage>,
}

impl UploadMarketThumbnail {
    pub fn new(markets: Arc<dyn MarketRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { markets, storage }
    }

    /// Returns the updated market with its outcomes.
    pub async fn execute(
        &self,
        actor: &Actor,
        market_id: MarketId,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<MarketView, ApplicationError> {
        if !authorization::can_manage_markets(actor.role) {
            return Err(ApplicationError::Forbidden("admin role required".into()));
        }

        let ext = image::validate("thumbnail", content_type, bytes, MAX_THUMBNAIL_BYTES)?;

        let mut market = self
            .markets
            .find_by_id(market_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;

        let url = image::store(&*self.storage, FOLDER, &market_id.to_string(), ext, bytes).await?;
        market.set_thumbnail_url(Some(url));
        self.markets.update_thumbnail(&market).await?;

        let outcomes = self.markets.outcomes_for(market_id).await?;
        Ok(MarketView { market, outcomes })
    }
}
