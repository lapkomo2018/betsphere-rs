use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use application::ports::{MessageBroker, MessageBrokerExt};
use application::realtime::{PriceTick, PriceUpdateBroadcast};
use domain::events::MarketPricesUpdated;
use domain::repositories::MarketRepository;

use super::EventHandler;

/// Fans a market's price change out to live WebSocket subscribers: on each
/// `MarketPricesUpdated` event, publishes the market's outcome prices to its
/// broker feed channel. The event carries only the market id; prices are read
/// at delivery time, so a redelivered or reordered event broadcasts the
/// current truth rather than resurrecting a stale snapshot — which also makes
/// the handler idempotent, as at-least-once delivery requires.
pub struct PriceUpdateBroadcaster {
    markets: Arc<dyn MarketRepository>,
    broker: Arc<dyn MessageBroker>,
}

impl PriceUpdateBroadcaster {
    pub fn new(markets: Arc<dyn MarketRepository>, broker: Arc<dyn MessageBroker>) -> Self {
        Self { markets, broker }
    }
}

#[async_trait]
impl EventHandler<MarketPricesUpdated> for PriceUpdateBroadcaster {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use futures::StreamExt;

    use domain::entities::{Market, Outcome, OutcomeId};
    use domain::repositories::MarketRepository as _;
    use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};

    use super::*;
    use crate::messaging::InMemoryMessageBroker;
    use crate::persistence::in_memory::InMemoryMarketRepository;

    fn seeded_market() -> (Market, Vec<Outcome>) {
        let market = Market::new(MarketTitle::new("Will it rain?").unwrap(), None, None, None);
        let outcomes = vec![
            Outcome::new(
                market.id(),
                OutcomeLabel::new("Yes").unwrap(),
                Price::from_ten_thousandths(2_500).unwrap(),
            ),
            Outcome::new(
                market.id(),
                OutcomeLabel::new("No").unwrap(),
                Price::from_ten_thousandths(7_500).unwrap(),
            ),
        ];
        (market, outcomes)
    }

    #[tokio::test]
    async fn broadcasts_the_market_current_prices() {
        let markets = Arc::new(InMemoryMarketRepository::new());
        let broker = Arc::new(InMemoryMessageBroker::new());
        let (market, outcomes) = seeded_market();
        markets.create(&market, &outcomes).await.unwrap();

        let mut feed = broker
            .subscribe_broadcast::<PriceUpdateBroadcast>(&market.id())
            .await
            .unwrap();

        let handler = PriceUpdateBroadcaster::new(markets, broker.clone());
        handler
            .handle(&MarketPricesUpdated {
                market_id: market.id(),
            })
            .await
            .unwrap();

        let broadcast = feed.next().await.unwrap();
        let prices: HashMap<OutcomeId, f64> = broadcast
            .ticks
            .iter()
            .map(|t| (t.outcome_id, t.price.as_fraction()))
            .collect();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices[&outcomes[0].id()], 0.25);
        assert_eq!(prices[&outcomes[1].id()], 0.75);
    }

    #[tokio::test]
    async fn unknown_market_is_a_no_op() {
        let handler = PriceUpdateBroadcaster::new(
            Arc::new(InMemoryMarketRepository::new()),
            Arc::new(InMemoryMessageBroker::new()),
        );
        handler
            .handle(&MarketPricesUpdated {
                market_id: domain::entities::MarketId::new(),
            })
            .await
            .unwrap();
    }
}
