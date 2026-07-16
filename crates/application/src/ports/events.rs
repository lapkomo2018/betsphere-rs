use async_trait::async_trait;

use domain::events::Event;

/// A subscriber to one event type. The topic comes from `E::TOPIC`, so a
/// handler can only ever be registered on — and handed — the event it is
/// written for. Delivery is at-least-once, so handlers must be idempotent.
///
/// The delivery mechanism is the infrastructure layer's concern: it decides
/// how events are stored, decoded, and retried, and hands each handler an
/// already-decoded event.
#[async_trait]
pub trait EventHandler<E: Event>: Send + Sync {
    /// Processes one event. Returning an error asks for redelivery — how, and
    /// how often, is up to the delivering pipeline.
    async fn handle(&self, event: &E) -> Result<(), String>;
}
