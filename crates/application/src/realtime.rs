//! Contracts for real-time broadcasts that cross process boundaries.
//!
//! The producer (a handler in [`broadcasters`](crate::broadcasters)) and the
//! consumer (the WebSocket endpoint in the API) live in different crates and,
//! at scale, in different processes; the payload they exchange over the
//! message broker is defined here so both sides share one definition. Each
//! payload's [`Broadcast`] impl also derives its broker channel, so a channel
//! can never be paired with the wrong message type.

mod bet;
mod chat;
mod market;

pub use bet::BetPlacedBroadcast;
pub use chat::{ChatAuthor, ChatMessageBroadcast};
pub use market::{PriceTick, PriceUpdateBroadcast};
