use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::DomainError;
use crate::entities::{MarketId, OutcomeId, UserId};
use crate::value_objects::market::Price;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BetId(Uuid);

impl BetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for BetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for BetId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<BetId> for Uuid {
    fn from(id: BetId) -> Self {
        id.0
    }
}

/// Lifecycle of a bet: live until its market resolves, then settled one way
/// or the other. `Refunded` is reserved for cancelled markets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetStatus {
    Active,
    Won,
    Lost,
    Refunded,
}

impl BetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Won => "won",
            Self::Lost => "lost",
            Self::Refunded => "refunded",
        }
    }
}

impl std::str::FromStr for BetStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "won" => Ok(Self::Won),
            "lost" => Ok(Self::Lost),
            "refunded" => Ok(Self::Refunded),
            other => Err(DomainError::Validation(format!(
                "unknown bet status: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for BetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A user's stake on one outcome of a market. The price is fixed at placement,
/// so later price moves change neither the stake nor the eventual payout.
#[derive(Debug, Clone)]
pub struct Bet {
    id: BetId,
    user_id: UserId,
    market_id: MarketId,
    outcome_id: OutcomeId,
    /// Stake in minimal currency units.
    amount: i64,
    /// The outcome's price when the bet was placed.
    price: Price,
    status: BetStatus,
    /// Credited winnings; set when the bet settles as won.
    payout: Option<i64>,
    created_at: DateTime<Utc>,
}

impl Bet {
    /// Places a new active bet. The amount must be positive and the price
    /// above zero — a zero price would make the payout division meaningless;
    /// callers clamp to [`Price::MIN_TICK`] when backing an outcome that has
    /// no volume yet.
    pub fn place(
        user_id: UserId,
        market_id: MarketId,
        outcome_id: OutcomeId,
        amount: i64,
        price: Price,
    ) -> Result<Self, DomainError> {
        if amount <= 0 {
            return Err(DomainError::Validation(
                "bet amount must be positive".into(),
            ));
        }
        if price == Price::ZERO {
            return Err(DomainError::Validation(
                "bet price must be above zero".into(),
            ));
        }
        Ok(Self {
            id: BetId::new(),
            user_id,
            market_id,
            outcome_id,
            amount,
            price,
            status: BetStatus::Active,
            payout: None,
            created_at: Utc::now(),
        })
    }

    /// Reconstructs a bet from persisted state. Only repositories should call this.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        id: BetId,
        user_id: UserId,
        market_id: MarketId,
        outcome_id: OutcomeId,
        amount: i64,
        price: Price,
        status: BetStatus,
        payout: Option<i64>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            user_id,
            market_id,
            outcome_id,
            amount,
            price,
            status,
            payout,
            created_at,
        }
    }

    /// Settles this bet as the winner and returns the payout to credit:
    /// `amount / price`, floored to whole units. Backing an outcome at 0.25
    /// returns four times the stake.
    pub fn settle_as_winner(&mut self) -> Result<i64, DomainError> {
        self.ensure_active()?;
        // `place` guarantees price > 0. Saturating keeps absurd stakes from
        // wrapping instead of panicking.
        let payout = self.amount.saturating_mul(i64::from(Price::SCALE))
            / i64::from(self.price.as_ten_thousandths());
        self.status = BetStatus::Won;
        self.payout = Some(payout);
        Ok(payout)
    }

    /// Settles this bet as a loser; the stake is simply gone.
    pub fn settle_as_loser(&mut self) -> Result<(), DomainError> {
        self.ensure_active()?;
        self.status = BetStatus::Lost;
        self.payout = None;
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), DomainError> {
        if self.status != BetStatus::Active {
            return Err(DomainError::RuleViolation(format!(
                "bet {} is already settled",
                self.id
            )));
        }
        Ok(())
    }

    pub fn id(&self) -> BetId {
        self.id
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn market_id(&self) -> MarketId {
        self.market_id
    }

    pub fn outcome_id(&self) -> OutcomeId {
        self.outcome_id
    }

    pub fn amount(&self) -> i64 {
        self.amount
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn status(&self) -> BetStatus {
        self.status
    }

    pub fn payout(&self) -> Option<i64> {
        self.payout
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bet_at(amount: i64, price_bp: i32) -> Bet {
        Bet::place(
            UserId::new(),
            MarketId::new(),
            OutcomeId::new(),
            amount,
            Price::from_ten_thousandths(price_bp).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn place_rejects_bad_amount_and_zero_price() {
        let (user, market, outcome) = (UserId::new(), MarketId::new(), OutcomeId::new());
        let half = Price::from_ten_thousandths(5_000).unwrap();
        assert!(Bet::place(user, market, outcome, 0, half).is_err());
        assert!(Bet::place(user, market, outcome, -5, half).is_err());
        assert!(Bet::place(user, market, outcome, 100, Price::ZERO).is_err());
    }

    #[test]
    fn winner_payout_is_stake_over_price() {
        // 100 staked at 0.2500 pays 400.
        let mut bet = bet_at(100, 2_500);
        assert_eq!(bet.settle_as_winner().unwrap(), 400);
        assert_eq!(bet.status(), BetStatus::Won);
        assert_eq!(bet.payout(), Some(400));
    }

    #[test]
    fn loser_keeps_no_payout_and_cannot_settle_twice() {
        let mut bet = bet_at(100, 5_000);
        bet.settle_as_loser().unwrap();
        assert_eq!(bet.status(), BetStatus::Lost);
        assert_eq!(bet.payout(), None);
        assert!(bet.settle_as_winner().is_err());
        assert!(bet.settle_as_loser().is_err());
    }

    #[test]
    fn status_round_trips_through_str() {
        for status in ["active", "won", "lost", "refunded"] {
            assert_eq!(status.parse::<BetStatus>().unwrap().as_str(), status);
        }
        assert!("void".parse::<BetStatus>().is_err());
    }
}
