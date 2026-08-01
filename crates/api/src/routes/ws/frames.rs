//! The JSON frames exchanged over the socket, and the two helpers that put a
//! server frame on the wire.

use application::realtime::{BetPlacedBroadcast, PriceTick};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Utc};
use domain::entities::{Bet, OutcomeId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routes::chat::ChatMessageResponse;

/// Frames a client may send. `channel` is a wire channel name (see the module
/// docs).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ClientFrame {
    /// Join a channel: replays its current state (chat history / market
    /// prices), then streams live frames.
    Subscribe { channel: String },
    /// Leave a channel.
    Unsubscribe { channel: String },
    /// Post a message to a chat room (subscribing first is not required).
    ChatMessage { channel: String, body: String },
}

/// Frames the server sends.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ServerFrame {
    /// One chat room's recent messages (oldest first), sent once per subscribe.
    History {
        channel: String,
        data: Vec<ChatMessageResponse>,
    },
    /// A bet feed's recent bets (oldest first), sent once per bet-feed
    /// subscribe. Shares the `history` wire tag with the chat variant; the
    /// channel name tells the client which payload shape to expect.
    #[serde(rename = "history")]
    BetHistory {
        channel: String,
        data: Vec<BetPlacedResponse>,
    },
    /// A live message posted to a chat room this client is subscribed to.
    ChatMessage {
        channel: String,
        data: ChatMessageResponse,
    },
    /// One outcome's new price on a market feed. A snapshot of every outcome
    /// is sent on subscribe; live frames follow as bets move the prices.
    PriceUpdate {
        channel: String,
        data: PriceUpdateResponse,
    },
    /// One newly committed bet on a bet feed.
    BetPlaced {
        channel: String,
        data: BetPlacedResponse,
    },
    /// A problem with the client's last frame; the connection stays open.
    Error { message: String },
}

#[derive(Debug, Serialize)]
pub(super) struct PriceUpdateResponse {
    pub(super) outcome_id: Uuid,
    pub(super) price: f64,
    pub(super) recorded_at: DateTime<Utc>,
}

impl From<PriceTick> for PriceUpdateResponse {
    fn from(value: PriceTick) -> Self {
        Self {
            outcome_id: value.outcome_id.as_uuid(),
            price: value.price.as_fraction(),
            recorded_at: value.recorded_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct BetPlacedResponse {
    id: Uuid,
    user_id: Uuid,
    /// Always present, including on a single market's feed, so one client-side
    /// handler serves both bet feeds.
    market_id: Uuid,
    outcome_id: OutcomeId,
    amount: i64,
    price: f64,
    created_at: DateTime<Utc>,
}

impl From<BetPlacedBroadcast> for BetPlacedResponse {
    fn from(value: BetPlacedBroadcast) -> Self {
        Self {
            id: value.id.as_uuid(),
            user_id: value.user_id.as_uuid(),
            market_id: value.market_id.as_uuid(),
            outcome_id: value.outcome_id,
            amount: value.amount,
            price: value.price.as_fraction(),
            created_at: value.created_at,
        }
    }
}

impl From<&Bet> for BetPlacedResponse {
    fn from(bet: &Bet) -> Self {
        Self {
            id: bet.id().as_uuid(),
            user_id: bet.user_id().as_uuid(),
            market_id: bet.market_id().as_uuid(),
            outcome_id: bet.outcome_id(),
            amount: bet.amount(),
            price: bet.price().as_fraction(),
            created_at: bet.created_at(),
        }
    }
}

/// Sends one frame; `Err` means the socket is dead and the connection loop
/// should stop.
pub(super) async fn send(socket: &mut WebSocket, frame: &ServerFrame) -> Result<(), ()> {
    let text = match serde_json::to_string(frame) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize ws frame");
            return Ok(());
        }
    };
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

pub(super) async fn send_error(socket: &mut WebSocket, message: &str) -> Result<(), ()> {
    send(
        socket,
        &ServerFrame::Error {
            message: message.to_owned(),
        },
    )
    .await
}
