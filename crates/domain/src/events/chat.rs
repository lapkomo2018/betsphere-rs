use serde::{Deserialize, Serialize};

use crate::entities::MessageId;

use super::Event;

/// A chat message was posted. Carries only the message id; subscribers load
/// the message (and its author's current profile) at delivery time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessagePosted {
    pub message_id: MessageId,
}

impl Event for ChatMessagePosted {
    const TOPIC: &'static str = "chat.message_posted";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::tests::round_trips;

    #[test]
    fn round_trips_through_serde() {
        round_trips(ChatMessagePosted {
            message_id: MessageId::new(),
        });
    }
}
