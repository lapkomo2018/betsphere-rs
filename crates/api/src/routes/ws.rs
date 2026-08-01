//! The platform's single WebSocket endpoint (`GET /ws`).
//!
//! Every real-time stream is multiplexed over this one socket via
//! `subscribe` / `unsubscribe` frames carrying a channel name:
//!
//! - `global_chat` — the global chat room;
//! - `market_chat:<market uuid>` — one market's chat room;
//! - `market:<market uuid>` — one market's live feed (price updates).
//! - `market_bets:<market uuid>` — one market's live feed of placed bets.
//! - `global_bets` — every market's placed bets, in one cross-market feed.
//!
//! A chat channel is two-way: `chat_message`, `add_reaction` and
//! `remove_reaction` frames go up, and `chat_message` and `reaction_update`
//! frames come back down. The reaction frames name a message rather than a
//! channel — the message id already says which room it is in.
//!
//! The endpoint is split along the path a frame takes: [`channel`] parses a
//! wire name into the stream it identifies, [`frames`] is the JSON both
//! directions speak, [`connection`] is the per-socket loop, and [`subscribe`]
//! joins one channel — broker subscription, state replay, and the task that
//! forwards its live frames.

mod channel;
mod connection;
mod frames;
mod subscribe;

use crate::error::ApiError;
use crate::state::{AppState, WsState};
use application::ApplicationError;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;

pub fn router() -> OpenApiRouter<AppState> {
    // The WebSocket upgrade isn't expressible in OpenAPI, so it's a plain
    // route rather than a documented one.
    OpenApiRouter::new().route("/ws", get(ws_upgrade))
}

/// Query string on the WebSocket handshake. Browsers can't set the
/// `Authorization` header on a WS request, so the access token rides here.
#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

/// Upgrades to the multiplexed WebSocket. Connect to `/ws?token=<access
/// token>`, then subscribe to channels with `subscribe` frames.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
    Query(query): Query<WsQuery>,
) -> Result<Response, ApiError> {
    let claims = state
        .access_tokens
        .verify(&query.token)
        .map_err(ApplicationError::from)?;
    let user_id = claims.user_id;

    Ok(ws.on_upgrade(move |socket| connection::handle_socket(socket, state, user_id)))
}
