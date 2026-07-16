use std::sync::Arc;

use async_trait::async_trait;

use crate::ports::{EventHandler, MessageBroker, MessageBrokerExt};
use crate::realtime::BetPlacedBroadcast;
use domain::events::BetPlaced;
use domain::repositories::BetRepository;

/// Fans a newly placed bet out to live WebSocket subscribers.
pub struct BetPlacedBroadcaster {
    bets: Arc<dyn BetRepository>,
    broker: Arc<dyn MessageBroker>,
}

impl BetPlacedBroadcaster {
    pub fn new(bets: Arc<dyn BetRepository>, broker: Arc<dyn MessageBroker>) -> Self {
        Self { bets, broker }
    }
}

#[async_trait]
impl EventHandler<BetPlaced> for BetPlacedBroadcaster {
    async fn handle(&self, event: &BetPlaced) -> Result<(), String> {
        let bet_id = event.bet_id;

        let Some(bet) = self
            .bets
            .find_by_id(bet_id)
            .await
            .map_err(|e| format!("could not find bet {}: {e}", bet_id))?
        else {
            tracing::warn!("bet {bet_id} vanished before broadcast");
            return Ok(());
        };

        let broadcast = BetPlacedBroadcast {
            id: bet_id,
            user_id: bet.user_id(),
            outcome_id: bet.outcome_id(),
            amount: bet.amount(),
            price: bet.price(),
            created_at: bet.created_at(),
        };
        self.broker
            .broadcast(&bet.market_id(), &broadcast)
            .await
            .map_err(|e| format!("could not broadcast bet {}: {e}", event.bet_id))
    }
}
