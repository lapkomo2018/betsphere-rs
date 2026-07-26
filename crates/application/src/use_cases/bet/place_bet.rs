use std::sync::Arc;

use chrono::Utc;

use domain::DomainError;
use domain::entities::{Bet, OutcomeId, PricePoint};
use domain::repositories::{BetRepository, MarketRepository, UserRepository};
use domain::services::pricing;
use domain::value_objects::market::Price;

use super::{BetView, view_for};
use crate::{Actor, ApplicationError};

/// Validated inputs for placing a bet.
pub struct NewBet {
    pub outcome_id: OutcomeId,
    /// Stake in minimal currency units.
    pub amount: i64,
}

/// Stakes part of the acting user's balance on one outcome of an open market.
/// The bet locks in the outcome's current price; volumes and prices are then
/// recalculated and persisted atomically with the balance debit.
pub struct PlaceBet {
    markets: Arc<dyn MarketRepository>,
    bets: Arc<dyn BetRepository>,
    users: Arc<dyn UserRepository>,
}

impl PlaceBet {
    pub fn new(
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            markets,
            bets,
            users,
        }
    }

    pub async fn execute(&self, actor: &Actor, input: NewBet) -> Result<BetView, ApplicationError> {
        let outcome = self
            .markets
            .outcome_by_id(input.outcome_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("outcome {}", input.outcome_id)))?;

        let market = self
            .markets
            .find_by_id(outcome.market_id())
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("market {}", outcome.market_id())))?;
        if !market.accepts_bets(Utc::now()) {
            return Err(DomainError::RuleViolation("market is not accepting bets".into()).into());
        }

        let mut outcomes = self.markets.outcomes_for(market.id()).await?;
        let chosen = outcomes
            .iter()
            .position(|o| o.id() == input.outcome_id)
            .ok_or_else(|| {
                DomainError::RuleViolation(format!(
                    "outcome {} does not belong to market {}",
                    input.outcome_id,
                    market.id()
                ))
            })?;

        let user = self
            .users
            .find_by_id(actor.user_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {}", actor.user_id)))?;
        // Friendly pre-check; the repository re-checks atomically at debit
        // time, so a concurrent spend can't push the balance negative.
        if user.balance() < input.amount {
            return Err(DomainError::RuleViolation("insufficient balance".into()).into());
        }

        // The bet buys at the price on the board. An outcome nobody has backed
        // yet sits at 0.0000; clamp to one tick so the payout stays defined.
        let price = outcomes[chosen].current_price().max(Price::MIN_TICK);
        let bet = Bet::place(
            actor.user_id,
            market.id(),
            input.outcome_id,
            input.amount,
            price,
        )?;

        outcomes[chosen].add_volume(bet.amount());
        pricing::recalculate_prices(&mut outcomes);
        let points: Vec<PricePoint> = outcomes
            .iter()
            .map(|o| PricePoint::new(o.id(), o.current_price()))
            .collect();

        self.bets.place(&bet, &outcomes, &points).await?;

        // Read back after the commit so the average covers the bet just
        // placed alongside whatever the bettor already held on this outcome.
        let avg_price = self
            .bets
            .active_positions(&[(actor.user_id, input.outcome_id)])
            .await?
            .first()
            .map_or(bet.price(), |position| position.avg_price);

        Ok(view_for(
            bet,
            &market,
            &outcomes[chosen],
            user.username().to_string(),
            avg_price,
        ))
    }
}
