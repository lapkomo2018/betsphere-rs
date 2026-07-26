use std::sync::Arc;

use domain::entities::{MarketId, OutcomeId};
use domain::repositories::MarketRepository;
use domain::services::authorization;

use super::{MAX_THUMBNAIL_BYTES, MarketView};
use crate::ports::FileStorage;
use crate::use_cases::image;
use crate::{Actor, ApplicationError};

/// Storage folder outcome thumbnails live in.
const FOLDER: &str = "outcome-thumbnails";

/// Stores an outcome's image and points its `thumbnail_url` at it. Only actors
/// allowed by [`authorization::can_manage_markets`] may call this.
pub struct UploadOutcomeThumbnail {
    markets: Arc<dyn MarketRepository>,
    storage: Arc<dyn FileStorage>,
}

impl UploadOutcomeThumbnail {
    pub fn new(markets: Arc<dyn MarketRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { markets, storage }
    }

    /// Returns the parent market with its outcomes, the uploaded one included.
    pub async fn execute(
        &self,
        actor: &Actor,
        market_id: MarketId,
        outcome_id: OutcomeId,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<MarketView, ApplicationError> {
        if !authorization::can_manage_markets(actor.role) {
            return Err(ApplicationError::Forbidden("admin role required".into()));
        }

        let ext = image::validate("thumbnail", content_type, bytes, MAX_THUMBNAIL_BYTES)?;

        let market = self
            .markets
            .find_by_id(market_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;

        let mut outcomes = self.markets.outcomes_for(market_id).await?;
        // An outcome of another market is as good as absent for this route.
        let outcome = outcomes
            .iter_mut()
            .find(|o| o.id() == outcome_id)
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("outcome {outcome_id} of market {market_id}"))
            })?;

        let url = image::store(&*self.storage, FOLDER, &outcome_id.to_string(), ext, bytes).await?;
        outcome.set_thumbnail_url(Some(url));
        self.markets.update_outcome_thumbnail(outcome).await?;

        Ok(MarketView { market, outcomes })
    }
}
