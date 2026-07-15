use std::sync::Arc;

use domain::entities::UserId;
use domain::repositories::{BetRepository, UserStats};

use crate::ApplicationError;

/// Aggregated betting stats for a profile. Callers resolve the user first,
/// so an unknown id simply yields empty stats here.
pub struct GetUserStats {
    bets: Arc<dyn BetRepository>,
}

impl GetUserStats {
    pub fn new(bets: Arc<dyn BetRepository>) -> Self {
        Self { bets }
    }

    pub async fn execute(&self, id: UserId) -> Result<UserStats, ApplicationError> {
        Ok(self.bets.stats_for_user(id).await?)
    }
}
