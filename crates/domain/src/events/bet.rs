use serde::{Deserialize, Serialize};

use crate::entities::BetId;

use super::Event;

/// A bet was placed and committed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetPlaced {
    pub bet_id: BetId,
}

impl Event for BetPlaced {
    const TOPIC: &'static str = "bet.placed";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::tests::round_trips;

    #[test]
    fn round_trips_through_serde() {
        round_trips(BetPlaced {
            bet_id: BetId::new(),
        });
    }
}