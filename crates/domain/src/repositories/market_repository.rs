use chrono::{DateTime, Utc};

use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::{Market, MarketId, MarketStatus, Outcome, OutcomeId, PricePoint};

/// How a market listing is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketSort {
    /// Highest combined activity (participants + volume) first.
    #[default]
    Popular,
    /// Most recently created first.
    Newest,
    /// Largest total volume first.
    Volume,
    /// Nearest `closes_at` deadline first (markets without one come last).
    ClosingSoon,
}

/// Filters, ordering, and pagination for a market listing. Built by the API
/// layer from the query string; `limit` is expected to be clamped by the caller.
#[derive(Debug, Clone)]
pub struct MarketFilter {
    pub sort: MarketSort,
    pub category: Option<String>,
    pub status: Option<MarketStatus>,
    pub search: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for MarketFilter {
    fn default() -> Self {
        Self {
            sort: MarketSort::default(),
            category: None,
            status: None,
            search: None,
            limit: 20,
            offset: 0,
        }
    }
}

/// Bucket width for aggregating price history into chart points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceInterval {
    #[default]
    Minute,
    Hour,
    Day,
}

/// Window and resolution for a price-history query.
#[derive(Debug, Clone, Default)]
pub struct PriceHistoryQuery {
    pub interval: PriceInterval,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

/// Port for market/outcome/price-history persistence. Implementations live in
/// the infrastructure layer.
#[async_trait]
pub trait MarketRepository: Send + Sync {
    /// Persists a new market together with its outcomes and a starting price
    /// point per outcome, atomically.
    async fn create(&self, market: &Market, outcomes: &[Outcome]) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: MarketId) -> Result<Option<Market>, RepositoryError>;

    /// Markets for several ids in one query, to avoid N+1 lookups when
    /// enriching a bet listing. Order is unspecified; missing ids are skipped.
    async fn find_by_ids(&self, ids: &[MarketId]) -> Result<Vec<Market>, RepositoryError>;

    /// Outcomes of one market, in a stable creation order.
    async fn outcomes_for(&self, market_id: MarketId) -> Result<Vec<Outcome>, RepositoryError>;

    /// Retrieves an `Outcome` entity by its unique identifier.
    async fn outcome_by_id(&self, outcome_id: OutcomeId) -> Result<Option<Outcome>, RepositoryError>;

    /// Outcomes for several markets in one query, to avoid N+1 lookups when
    /// building a listing.
    async fn outcomes_for_markets(
        &self,
        market_ids: &[MarketId],
    ) -> Result<Vec<Outcome>, RepositoryError>;

    async fn list(&self, filter: &MarketFilter) -> Result<Vec<Market>, RepositoryError>;

    /// The single most popular market right now, if any exist. Powers the
    /// homepage's featured slot.
    async fn featured(&self) -> Result<Option<Market>, RepositoryError>;

    /// Persists a resolution: the market's new status and winning outcome.
    async fn resolve(&self, market: &Market) -> Result<(), RepositoryError>;

    async fn price_history(
        &self,
        market_id: MarketId,
        query: &PriceHistoryQuery,
    ) -> Result<Vec<PricePoint>, RepositoryError>;
}
