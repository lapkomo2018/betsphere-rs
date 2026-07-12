//! Composition root: wires infrastructure into use cases and starts the server.

mod config;
mod error;
mod extract;
mod routes;
mod state;

use std::sync::Arc;

use infrastructure::auth::{Argon2PasswordHasher, JwtAccessTokens};
use infrastructure::persistence::postgres::{
    connect, run_migrations, PgRefreshTokenRepository, PgUnitOfWork, PgUserRepository,
};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;
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

    let pool = connect(&config.database.url())
        .await
        .expect("failed to connect to Postgres");
    run_migrations(&pool).await.expect("migrations failed");
    tracing::info!("database migrations are up to date");

    let users = Arc::new(PgUserRepository::new(pool.clone()));
    let refresh_tokens = Arc::new(PgRefreshTokenRepository::new(pool.clone()));
    let uow = Arc::new(PgUnitOfWork::new(pool));
    let hasher = Arc::new(Argon2PasswordHasher::new());
    let access_tokens = Arc::new(JwtAccessTokens::new(
        &config.auth.jwt_secret,
        config.auth.access_ttl,
    ));

    let state = AppState::new(
        users,
        refresh_tokens,
        uow,
        hasher,
        access_tokens,
        config.auth.refresh_ttl,
        config.auth.cookie_secure,
    );

    let cors = config.cors.layer().expect("invalid CORS configuration");
    let trace = TraceLayer::new_for_http()
        .make_span_with(|request: &axum::http::Request<_>| {
            tracing::info_span!(
                "request",
                method = %request.method(),
                uri = %request.uri(),
            )
        })
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        );
    let app = routes::router(state).layer(cors).layer(trace);

    let listener = tokio::net::TcpListener::bind(config.server.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.server.bind_addr));

    tracing::info!("betsphere API listening on {}", config.server.bind_addr);
    axum::serve(listener, app).await.expect("server error");
}
