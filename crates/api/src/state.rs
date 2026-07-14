//! Shared handler state, one substate per feature area.
//!
//! `FromRef` lets a handler take `State<AuthState>` (etc.), so each route
//! module only sees the use cases it needs; new features add a substate here
//! instead of growing one flat struct.

mod auth;
mod bet;
mod chat;
mod file;
mod market;
mod user;

use axum::extract::FromRef;

pub use auth::AuthState;
pub use bet::BetState;
pub use chat::{ChatState, GLOBAL_CHANNEL, HISTORY_LIMIT};
pub use file::FileState;
pub use market::MarketState;
pub use user::UserState;

/// Top-level state the router is built with; substates are extracted from it.
#[derive(Clone, FromRef)]
pub struct AppState {
    pub auth: AuthState,
    pub users: UserState,
    pub files: FileState,
    pub chat: ChatState,
    pub markets: MarketState,
    pub bets: BetState,
}
