use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::{Bet, BetStatus, Market, MarketId, Outcome, PricePoint, UserId};

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
}
