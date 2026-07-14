//! Domain events: facts about committed state changes that other parts of
//! the system react to.
//!
//! Events are recorded by repositories inside the same transaction as the
//! change they describe (a transactional outbox), so an event exists if and
//! only if the change committed. Infrastructure delivers them to subscribers
//! asynchronously, at least once — handlers must be idempotent.

use crate::entities::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainEvent {
    /// A user's balance changed outside the ordinary user-save path (bet
    /// stakes and payouts, which run as raw SQL transactions).
    UserBalanceChanged { user_id: UserId },
}

impl DomainEvent {
    /// Topic of [`DomainEvent::UserBalanceChanged`].
    pub const USER_BALANCE_CHANGED: &'static str = "user.balance_changed";

    /// Stable string identifying the event type in storage and to subscribers.
    pub fn topic(&self) -> &'static str {
        match self {
            Self::UserBalanceChanged { .. } => Self::USER_BALANCE_CHANGED,
        }
    }
}
