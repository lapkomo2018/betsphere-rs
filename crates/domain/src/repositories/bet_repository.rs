use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::{
    Bet, BetId, BetStatus, Market, MarketId, Outcome, OutcomeId, PricePoint, UserId,
};
use crate::value_objects::market::Price;

/// How a bet listing is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BetSort {
    /// Most recently placed first.
    #[default]
    Newest,
    /// Largest stakes first.
    Popular,
}

/// Filters, ordering, and pagination for a bet listing. Built by the API
/// layer from the query string; `limit` is expected to be clamped by the caller.
#[derive(Debug, Clone)]
pub struct BetFilter {
    pub status: Option<BetStatus>,
    pub sort: BetSort,
    pub limit: i64,
    pub offset: i64,
}

impl Default for BetFilter {
    fn default() -> Self {
        Self {
            status: None,
            sort: BetSort::default(),
            limit: 20,
            offset: 0,
        }
    }
}

/// Aggregated betting record of one user, computed over every bet they have
/// placed. Refunded bets count toward totals but neither wins nor losses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserStats {
    pub total_bets: i64,
    pub wins: i64,
    pub losses: i64,
    /// Sum of every stake, in minimal currency units.
    pub total_volume: i64,
}

impl UserStats {
    /// Share of settled bets that won: `wins / (wins + losses)`.
    /// Zero while nothing has settled.
    pub fn win_rate(&self) -> f64 {
        let settled = self.wins + self.losses;
        if settled == 0 {
            0.0
        } else {
            self.wins as f64 / settled as f64
        }
    }
}

/// One user's still-open stake on a single outcome, aggregated over every
/// active bet they hold on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivePosition {
    pub user_id: UserId,
    pub outcome_id: OutcomeId,
    /// Stake-weighted average of the prices those bets were fixed at:
    /// `SUM(amount * price) / SUM(amount)`, floored to a whole tick. A bigger
    /// stake pulls the average further toward its own price.
    pub avg_price: Price,
}

/// Port for bet persistence. Placing and settling bets move balances and
/// market aggregates together, so implementations must make [`place`](Self::place)
/// and [`settle`](Self::settle) atomic — partial application would corrupt
/// balances.
#[async_trait]
pub trait BetRepository: Send + Sync {
    /// Atomically records a placed bet: debits the bettor's balance — failing
    /// with [`RepositoryError::Conflict`] if it no longer covers the stake —
    /// stores the bet, applies the recalculated outcome volumes and prices,
    /// bumps the market's total volume (and participant count for a bettor's
    /// first bet on the market), and appends the price points.
    async fn place(
        &self,
        bet: &Bet,
        priced_outcomes: &[Outcome],
        points: &[PricePoint],
    ) -> Result<(), RepositoryError>;

    /// Every still-active bet on a market, for settlement.
    async fn active_for_market(&self, market_id: MarketId) -> Result<Vec<Bet>, RepositoryError>;

    /// Atomically records a resolution: the market's new status and winner,
    /// each settled bet's status and payout, and the balance credits for
    /// winning bets.
    async fn settle(&self, market: &Market, settled: &[Bet]) -> Result<(), RepositoryError>;

    /// Finds a bet by its ID.
    async fn find_by_id(&self, id: BetId) -> Result<Option<Bet>, RepositoryError>;

    async fn find_by_user(
        &self,
        user_id: UserId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError>;

    async fn find_by_market(
        &self,
        market_id: MarketId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError>;

    /// The global bet feed across all markets.
    async fn feed(&self, filter: &BetFilter) -> Result<Vec<Bet>, RepositoryError>;

    /// Aggregated stats over every bet the user has placed.
    async fn stats_for_user(&self, user_id: UserId) -> Result<UserStats, RepositoryError>;

    /// The active positions behind the given `(user, outcome)` pairs, batched
    /// so a listing resolves every row in one round trip. Pairs the user holds
    /// no active bet on are simply absent from the result, and the order is
    /// unspecified — callers index by the pair.
    async fn active_positions(
        &self,
        pairs: &[(UserId, OutcomeId)],
    ) -> Result<Vec<ActivePosition>, RepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_rate_is_wins_over_settled() {
        let stats = UserStats {
            total_bets: 5,
            wins: 3,
            losses: 1,
            total_volume: 500,
        };
        assert_eq!(stats.win_rate(), 0.75);
    }

    #[test]
    fn win_rate_is_zero_with_no_settled_bets() {
        assert_eq!(UserStats::default().win_rate(), 0.0);
    }
}
