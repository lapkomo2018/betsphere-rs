use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::DomainError;
use crate::value_objects::market::{MarketTitle, OutcomeLabel, Price};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarketId(Uuid);

impl MarketId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MarketId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for MarketId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<MarketId> for Uuid {
    fn from(id: MarketId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutcomeId(Uuid);

impl OutcomeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for OutcomeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OutcomeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for OutcomeId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<OutcomeId> for Uuid {
    fn from(id: OutcomeId) -> Self {
        id.0
    }
}

/// Lifecycle of a market: accepting bets, closed to new bets, or settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStatus {
    Open,
    Closed,
    Resolved,
}

impl MarketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Resolved => "resolved",
        }
    }
}

impl std::str::FromStr for MarketStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "resolved" => Ok(Self::Resolved),
            other => Err(DomainError::Validation(format!(
                "unknown market status: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A prediction market — the platform's central entity. A market carries two or
/// more [`Outcome`]s and settles to exactly one of them when resolved.
#[derive(Debug, Clone)]
pub struct Market {
    id: MarketId,
    title: MarketTitle,
    description: Option<String>,
    category: Option<String>,
    status: MarketStatus,
    resolved_outcome_id: Option<OutcomeId>,
    total_volume: i64,
    participants_count: i32,
    created_at: DateTime<Utc>,
    closes_at: Option<DateTime<Utc>>,
}

impl Market {
    /// Creates a brand-new open market. `description` and `category` are trimmed
    /// and normalised to `None` when blank.
    pub fn new(
        title: MarketTitle,
        description: Option<String>,
        category: Option<String>,
        closes_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: MarketId::new(),
            title,
            description: normalise_optional(description),
            category: normalise_optional(category),
            status: MarketStatus::Open,
            resolved_outcome_id: None,
            total_volume: 0,
            participants_count: 0,
            created_at: Utc::now(),
            closes_at,
        }
    }

    /// Reconstructs a market from persisted state. Only repositories should call this.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        id: MarketId,
        title: MarketTitle,
        description: Option<String>,
        category: Option<String>,
        status: MarketStatus,
        resolved_outcome_id: Option<OutcomeId>,
        total_volume: i64,
        participants_count: i32,
        created_at: DateTime<Utc>,
        closes_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            title,
            description,
            category,
            status,
            resolved_outcome_id,
            total_volume,
            participants_count,
            created_at,
            closes_at,
        }
    }

    /// Settles the market on `winning_outcome`. The caller is responsible for
    /// checking the outcome belongs to this market (it needs the outcome list
    /// to do so). Fails if the market is already resolved.
    pub fn resolve(&mut self, winning_outcome: OutcomeId) -> Result<(), DomainError> {
        if self.status == MarketStatus::Resolved {
            return Err(DomainError::RuleViolation(
                "market is already resolved".into(),
            ));
        }
        self.status = MarketStatus::Resolved;
        self.resolved_outcome_id = Some(winning_outcome);
        Ok(())
    }

    /// Records a newly staked bet in the market's aggregates. `new_participant`
    /// is true when this is the bettor's first bet on the market.
    pub fn record_stake(&mut self, amount: i64, new_participant: bool) {
        self.total_volume = self.total_volume.saturating_add(amount);
        if new_participant {
            self.participants_count += 1;
        }
    }

    /// Whether the market currently accepts bets: open and not past its deadline.
    pub fn accepts_bets(&self, now: DateTime<Utc>) -> bool {
        self.status == MarketStatus::Open && self.closes_at.is_none_or(|deadline| now < deadline)
    }

    pub fn id(&self) -> MarketId {
        self.id
    }

    pub fn title(&self) -> &MarketTitle {
        &self.title
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    pub fn status(&self) -> MarketStatus {
        self.status
    }

    pub fn resolved_outcome_id(&self) -> Option<OutcomeId> {
        self.resolved_outcome_id
    }

    pub fn total_volume(&self) -> i64 {
        self.total_volume
    }

    pub fn participants_count(&self) -> i32 {
        self.participants_count
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn closes_at(&self) -> Option<DateTime<Utc>> {
        self.closes_at
    }
}

/// One possible result of a [`Market`], carrying its current price (implied
/// probability) and the total volume staked on it.
#[derive(Debug, Clone)]
pub struct Outcome {
    id: OutcomeId,
    market_id: MarketId,
    label: OutcomeLabel,
    current_price: Price,
    volume: i64,
}

impl Outcome {
    /// Creates a brand-new outcome with zero volume at the given starting price.
    pub fn new(market_id: MarketId, label: OutcomeLabel, current_price: Price) -> Self {
        Self {
            id: OutcomeId::new(),
            market_id,
            label,
            current_price,
            volume: 0,
        }
    }

    /// Reconstructs an outcome from persisted state. Only repositories should call this.
    pub fn from_parts(
        id: OutcomeId,
        market_id: MarketId,
        label: OutcomeLabel,
        current_price: Price,
        volume: i64,
    ) -> Self {
        Self {
            id,
            market_id,
            label,
            current_price,
            volume,
        }
    }

    pub fn id(&self) -> OutcomeId {
        self.id
    }

    pub fn market_id(&self) -> MarketId {
        self.market_id
    }

    pub fn label(&self) -> &OutcomeLabel {
        &self.label
    }

    pub fn current_price(&self) -> Price {
        self.current_price
    }

    pub fn set_current_price(&mut self, price: Price) {
        self.current_price = price;
    }

    /// Adds a freshly staked bet's amount to this outcome's volume.
    pub fn add_volume(&mut self, amount: i64) {
        self.volume = self.volume.saturating_add(amount);
    }

    pub fn volume(&self) -> i64 {
        self.volume
    }
}

/// Trims a free-text field and collapses a blank value to `None`.
fn normalise_optional(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_market() -> Market {
        Market::new(
            MarketTitle::new("Will it rain tomorrow?").unwrap(),
            Some("  ".into()),
            Some(" weather ".into()),
            None,
        )
    }

    #[test]
    fn new_market_is_open_with_normalised_fields() {
        let market = open_market();
        assert_eq!(market.status(), MarketStatus::Open);
        assert_eq!(market.description(), None); // blank collapses to None
        assert_eq!(market.category(), Some("weather"));
        assert_eq!(market.total_volume(), 0);
    }

    #[test]
    fn resolve_sets_winner_and_rejects_double_resolve() {
        let mut market = open_market();
        let winner = OutcomeId::new();
        market.resolve(winner).unwrap();
        assert_eq!(market.status(), MarketStatus::Resolved);
        assert_eq!(market.resolved_outcome_id(), Some(winner));
        assert!(market.resolve(OutcomeId::new()).is_err());
    }

    #[test]
    fn status_round_trips_through_str() {
        for status in ["open", "closed", "resolved"] {
            assert_eq!(status.parse::<MarketStatus>().unwrap().as_str(), status);
        }
        assert!("archived".parse::<MarketStatus>().is_err());
    }
}
