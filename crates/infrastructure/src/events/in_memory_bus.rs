use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use application::ports::EventHandler;
use domain::events::Event;

use super::{DeliveryError, ErasedEventHandler, TypedHandler};

/// Synchronous in-process stand-in for the outbox pipeline, for environments
/// without Postgres (tests, the in-memory dev setup): in-memory repositories
/// dispatch each event straight to the registered handlers right after the
/// write that raised it. Delivery is best-effort — there is no persisted
/// backlog, so a failing handler is logged and the event dropped.
#[derive(Default)]
pub struct InMemoryEventBus {
    handlers: RwLock<HashMap<String, Vec<Arc<dyn ErasedEventHandler>>>>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `handler` on its event type's topic. Interior-mutable
    /// (unlike [`OutboxProcessor::with_handler`](super::OutboxProcessor::with_handler))
    /// because handlers usually hold the very repositories that hold this bus,
    /// so they can only be built after it.
    pub fn register<E, H>(&self, handler: H)
    where
        E: Event,
        H: EventHandler<E> + 'static,
    {
        self.handlers
            .write()
            .expect("event bus lock poisoned")
            .entry(E::TOPIC.to_string())
            .or_default()
            .push(Arc::new(TypedHandler::new(handler)));
    }

    /// Delivers one event to every handler of its topic, in registration
    /// order. Encodes through the same payload representation the outbox
    /// stores, so tests exercise the production codec.
    pub async fn dispatch<E: Event>(&self, event: &E) {
        let handlers = self
            .handlers
            .read()
            .expect("event bus lock poisoned")
            .get(E::TOPIC)
            .cloned();
        let payload = match serde_json::to_value(event) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::warn!("unencodable in-memory event {}: {e}", E::TOPIC);
                return;
            }
        };
        for handler in handlers.into_iter().flatten() {
            if let Err(DeliveryError::Undecodable(e) | DeliveryError::Failed(e)) =
                handler.handle(&payload).await
            {
                tracing::warn!("in-memory event {} failed: {e}", E::TOPIC);
            }
        }
    }
}
