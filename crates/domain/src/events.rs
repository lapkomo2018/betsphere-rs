//! Domain events: facts about committed state changes that other parts of
//! the system react to.
//!
//! Events are recorded by repositories inside the same transaction as the
//! change they describe (a transactional outbox), so an event exists if and
//! only if the change committed. Infrastructure delivers them to subscribers
//! asynchronously, at least once — handlers must be idempotent.
//!
//! Each event is its own struct implementing [`Event`], which ties the
//! stable topic string to the type: a handler subscribes to one event type
//! and can never be handed another, and serde derives the storage encoding,
//! so there is no per-event field mapping anywhere to forget or get wrong.

mod user;
mod market;
mod chat;

pub use chat::ChatMessagePosted;
pub use market::MarketPricesUpdated;
pub use user::UserBalanceChanged;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// A domain event. `TOPIC` is the stable string identifying the event type
/// in storage and to subscribers; the serde bounds are how storage encodes
/// and decodes the event's fields.
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    const TOPIC: &'static str;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Asserts an event survives the encode/decode cycle storage puts it
    /// through. Each event's own tests call this.
    pub(crate) fn round_trips<E: Event + PartialEq + std::fmt::Debug>(event: E) {
        let value = serde_json::to_value(&event).unwrap();
        let back: E = serde_json::from_value(value).unwrap();
        assert_eq!(back, event);
    }

    /// Topics live spread across the event modules, so collisions are only
    /// visible here; list every event's topic.
    #[test]
    fn topics_are_distinct() {
        let topics = [
            UserBalanceChanged::TOPIC,
            MarketPricesUpdated::TOPIC,
            ChatMessagePosted::TOPIC,
        ];
        let unique: std::collections::HashSet<_> = topics.iter().collect();
        assert_eq!(unique.len(), topics.len());
    }
}
