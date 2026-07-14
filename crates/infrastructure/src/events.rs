//! Transactional-outbox event pipeline.
//!
//! Repositories record [`domain::events::DomainEvent`]s in the
//! `outbox_events` table **inside the same transaction** as the change they
//! describe, so an event exists if and only if the change committed. The
//! [`OutboxProcessor`] then delivers the backlog to registered
//! [`EventHandler`]s: at-least-once, in insertion order, retrying failures.
//! A crash between commit and delivery loses nothing — the event is still in
//! the table when the processor comes back.

mod outbox;
mod user_cache;

pub use outbox::{OutboxProcessor, publish};
pub use user_cache::UserCacheInvalidator;

use async_trait::async_trait;

/// A subscriber to outbox events. Delivery is at-least-once, so handlers
/// must be idempotent.
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// The topic this handler consumes (see [`domain::events::DomainEvent::topic`]).
    fn topic(&self) -> &'static str;

    /// Processes one event. Returning an error leaves the event pending; the
    /// processor delivers it again on a later pass.
    async fn handle(&self, payload: &serde_json::Value) -> Result<(), String>;
}
