use std::sync::Arc;

use domain::entities::{MarketId, OutcomeId};
use domain::repositories::{BetRepository, MarketRepository};
use domain::services::authorization;

use super::MarketView;
use crate::{Actor, ApplicationError};

/// Settles a market on a winning outcome and pays out the bets: every active
/// bet on the winner is credited `amount / price`, every other active bet is
/// marked lost. The market update, bet statuses, and balance credits are
/// persisted atomically. Only actors allowed by
/// [`authorization::can_manage_markets`] may call this.
pub struct ResolveMarket {
    markets: Arc<dyn MarketRepository>,
    bets: Arc<dyn BetRepository>,
}

impl ResolveMarket {
    pub fn new(markets: Arc<dyn MarketRepository>, bets: Arc<dyn BetRepository>) -> Self {
        Self { markets, bets }
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

        // Settle every open bet: winners are paid `amount / price`, the rest
        // lose their stake. Persisted with the market update in one shot.
        let mut settled = self.bets.active_for_market(market_id).await?;
        for bet in &mut settled {
            if bet.outcome_id() == winning_outcome {
                bet.settle_as_winner()?;
            } else {
                bet.settle_as_loser()?;
            }
        }
        self.bets.settle(&market, &settled).await?;

        Ok(MarketView { market, outcomes })
    }
}
