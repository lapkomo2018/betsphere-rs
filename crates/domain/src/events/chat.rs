use serde::{Deserialize, Serialize};

use crate::entities::{MessageId, UserId};

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

/// A user added or took back a reaction on a chat message. The emoji rides
/// along because it names *which* tally moved — everything else about the
/// reaction (the resulting count, the message's room) is re-read at delivery
/// time, as with [`ChatMessagePosted`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReactionChanged {
    pub message_id: MessageId,
    pub user_id: UserId,
    pub emoji: String,
    /// `true` when the reaction was added, `false` when it was taken back.
    pub added: bool,
}

impl Event for ChatReactionChanged {
    const TOPIC: &'static str = "chat.reaction_changed";
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
        round_trips(ChatReactionChanged {
            message_id: MessageId::new(),
            user_id: UserId::new(),
            emoji: "🔥".to_owned(),
            added: true,
        });
    }
}
