use std::sync::Arc;

use async_trait::async_trait;

use crate::ports::{EventHandler, MessageBroker, MessageBrokerExt};
use crate::realtime::{BetFeed, BetPlacedBroadcast};
use domain::events::BetPlaced;
use domain::repositories::BetRepository;

/// Fans a newly placed bet out to live WebSocket subscribers, on both its
/// market's feed and the cross-market one.
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
            market_id: bet.market_id(),
            outcome_id: bet.outcome_id(),
            amount: bet.amount(),
            price: bet.price(),
            created_at: bet.created_at(),
        };
        // The market feed first: a subscriber watching one market should not
        // learn of its bets later than the global ticker does.
        for feed in [BetFeed::Market(bet.market_id()), BetFeed::Global] {
            self.broker
                .broadcast(&feed, &broadcast)
                .await
                .map_err(|e| format!("could not broadcast bet {bet_id}: {e}"))?;
        }
        Ok(())
    }
}
