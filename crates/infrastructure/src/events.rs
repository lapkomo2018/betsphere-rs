//! Transactional-outbox event pipeline.
//!
//! Repositories record [`domain::events`] in the `outbox_events` table
//! **inside the same transaction** as the change they describe, so an event
//! exists if and only if the change committed. The [`OutboxProcessor`] then
//! delivers the backlog to registered [`EventHandler`]s: at-least-once, in
//! insertion order, retrying failures. A crash between commit and delivery
//! loses nothing — the event is still in the table when the processor comes
//! back.

mod chat_broadcaster;
mod in_memory_bus;
mod outbox;
mod price_broadcaster;
mod user_cache;

pub use chat_broadcaster::ChatMessageBroadcaster;
pub use in_memory_bus::InMemoryEventBus;
pub use outbox::{publish, OutboxProcessor};
pub use price_broadcaster::PriceUpdateBroadcaster;
pub use user_cache::UserCacheInvalidator;

use std::marker::PhantomData;

use async_trait::async_trait;
use domain::events::Event;

/// A subscriber to one event type. The topic comes from `E::TOPIC`, so a
/// handler can only ever be registered on — and handed — the event it is
/// written for. Delivery is at-least-once, so handlers must be idempotent.
#[async_trait]
pub trait EventHandler<E: Event>: Send + Sync {
    /// Processes one event. Returning an error leaves the event pending; the
    /// processor delivers it again on a later pass.
    async fn handle(&self, event: &E) -> Result<(), String>;
}

/// Why one erased delivery attempt did not succeed.
enum DeliveryError {
    /// The stored payload does not decode as this topic's event type. It can
    /// never succeed, so the processor drops the event instead of retrying.
    Undecodable(String),
    /// The handler failed; the event stays pending for a later pass.
    Failed(String),
}

/// Object-safe view of an [`EventHandler`], so differently-typed handlers
/// can share one registry (keyed by `E::TOPIC` at registration). Decodes the
/// stored payload into the handler's event type — the only place raw
/// payloads meet handlers.
#[async_trait]
trait ErasedEventHandler: Send + Sync {
    async fn handle(&self, payload: &serde_json::Value) -> Result<(), DeliveryError>;
}

/// Adapter binding a typed [`EventHandler`] to its event type's topic.
struct TypedHandler<E, H> {
    handler: H,
    _event: PhantomData<fn(E)>,
}

impl<E: Event, H: EventHandler<E>> TypedHandler<E, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _event: PhantomData,
        }
    }
}

#[async_trait]
impl<E: Event, H: EventHandler<E>> ErasedEventHandler for TypedHandler<E, H> {
    async fn handle(&self, payload: &serde_json::Value) -> Result<(), DeliveryError> {
        let event: E = serde_json::from_value(payload.clone())
            .map_err(|e| DeliveryError::Undecodable(format!("not a {} event: {e}", E::TOPIC)))?;
        self.handler
            .handle(&event)
            .await
            .map_err(DeliveryError::Failed)
    }
}
