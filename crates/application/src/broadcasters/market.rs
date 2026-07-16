use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use crate::ports::{EventHandler, MessageBroker, MessageBrokerExt};
use crate::realtime::{PriceTick, PriceUpdateBroadcast};
use domain::events::MarketPricesUpdated;
use domain::repositories::MarketRepository;

/// Fans a market's price change out to live WebSocket subscribers: on each
/// `MarketPricesUpdated` event, publishes the market's outcome prices to its
/// broker feed channel. The event carries only the market id; prices are read
/// at delivery time, so a redelivered or reordered event broadcasts the
/// current truth rather than resurrecting a stale snapshot — which also makes
/// the handler idempotent, as at-least-once delivery requires.
pub struct MarketPriceUpdateBroadcaster {
    markets: Arc<dyn MarketRepository>,
    broker: Arc<dyn MessageBroker>,
}

impl MarketPriceUpdateBroadcaster {
    pub fn new(markets: Arc<dyn MarketRepository>, broker: Arc<dyn MessageBroker>) -> Self {
        Self { markets, broker }
    }
}

#[async_trait]
impl EventHandler<MarketPricesUpdated> for MarketPriceUpdateBroadcaster {
    async fn handle(&self, event: &MarketPricesUpdated) -> Result<(), String> {
        let market_id = event.market_id;

        let outcomes = self
            .markets
            .outcomes_for(market_id)
            .await
            .map_err(|e| format!("could not load outcomes of market {market_id}: {e}"))?;
        if outcomes.is_empty() {
            // Market no longer exists; nothing to broadcast.
            return Ok(());
        }

        let recorded_at = Utc::now();
        let ticks: Vec<PriceTick> = outcomes
            .iter()
            .map(|o| PriceTick {
                outcome_id: o.id(),
                price: o.current_price(),
                recorded_at,
            })
            .collect();

        self.broker
            .broadcast(&market_id, &PriceUpdateBroadcast { ticks })
            .await
            .map_err(|e| format!("could not broadcast prices of market {market_id}: {e}"))
    }
}
