use serde::{Deserialize, Serialize};

use crate::entities::UserId;

use super::Event;

/// A user's balance changed outside the ordinary user-save path (bet stakes
/// and payouts, which run as raw SQL transactions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserBalanceChanged {
    pub user_id: UserId,
}

impl Event for UserBalanceChanged {
    const TOPIC: &'static str = "user.balance_changed";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::tests::round_trips;

    #[test]
    fn round_trips_through_serde() {
        round_trips(UserBalanceChanged {
            user_id: UserId::new(),
        });
    }
}
