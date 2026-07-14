use std::sync::Arc;

use domain::entities::{MarketId, OutcomeId};
use domain::repositories::MarketRepository;
use domain::services::authorization;

use super::MarketView;
use crate::{Actor, ApplicationError};

/// Settles a market on a winning outcome. Only actors allowed by
/// [`authorization::can_manage_markets`] may call this.
///
/// Payouts to winning bets are intentionally out of scope here: bets are a
/// separate feature (spec §4) not yet implemented. This resolves the market's
/// state — status and winning outcome — which is the precondition for that
/// later payout step.
pub struct ResolveMarket {
    markets: Arc<dyn MarketRepository>,
}

impl ResolveMarket {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        market_id: MarketId,
        winning_outcome: OutcomeId,
    ) -> Result<MarketView, ApplicationError> {
        if !authorization::can_manage_markets(actor.role) {
            return Err(ApplicationError::Forbidden("admin role required".into()));
        }

        let mut market = self
            .markets
            .find_by_id(market_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("market {market_id}")))?;

        let outcomes = self.markets.outcomes_for(market_id).await?;
        if !outcomes.iter().any(|o| o.id() == winning_outcome) {
            return Err(ApplicationError::Domain(
                domain::DomainError::RuleViolation(format!(
                    "outcome {winning_outcome} does not belong to market {market_id}"
                )),
            ));
        }

        // Domain guards against resolving twice.
        market.resolve(winning_outcome)?;
        self.markets.resolve(&market).await?;

        Ok(MarketView { market, outcomes })
    }
}
