//! Composition root: wires infrastructure into use cases and starts the server.

mod config;
mod error;
mod routes;
mod state;

use std::sync::Arc;

use infrastructure::persistence::in_memory::InMemoryUserRepository;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env().expect("invalid configuration");

    let user_repository = Arc::new(InMemoryUserRepository::new());
    let state = AppState::new(user_repository);

    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(config.server.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.server.bind_addr));

    tracing::info!("betsphere API listening on {}", config.server.bind_addr);
    axum::serve(listener, app).await.expect("server error");
}
