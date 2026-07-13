//! Composition root: wires infrastructure into use cases and starts the server.

mod config;
mod error;
mod extract;
mod routes;
mod state;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use infrastructure::auth::{Argon2PasswordHasher, JwtAccessTokens};
use infrastructure::messaging::RedisMessageBroker;
use infrastructure::persistence::postgres::{
    self, PgChatMessageRepository, PgRefreshTokenRepository, PgUnitOfWork, PgUserRepository,
    run_migrations,
};
use infrastructure::persistence::redis::{self, CachedUserRepository};
use infrastructure::storage::LocalFileStorage;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::{AppState, AuthState, ChatState, FileState, UserState};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env().expect("invalid configuration");

    let pool = postgres::connect(&config.database.url())
        .await
        .expect("failed to connect to Postgres");
    run_migrations(&pool).await.expect("migrations failed");
    tracing::info!("database migrations are up to date");

    let cache = redis::connect(&config.redis.url())
        .await
        .expect("failed to connect to Redis");
    tracing::info!("connected to Redis");
    let redis_client = redis::client(&config.redis.url()).expect("invalid Redis URL");

    let users = Arc::new(CachedUserRepository::new(
        Arc::new(PgUserRepository::new(pool.clone())),
        cache.clone(),
        config.redis.cache_ttl,
    ));
    let refresh_tokens = Arc::new(PgRefreshTokenRepository::new(pool.clone()));
    let chat_messages = Arc::new(PgChatMessageRepository::new(pool.clone()));
    let uow = Arc::new(PgUnitOfWork::new(pool));
    let hasher = Arc::new(Argon2PasswordHasher::new());
    let access_tokens = Arc::new(JwtAccessTokens::new(
        &config.auth.jwt_secret,
        config.auth.access_ttl,
    ));
    let storage = Arc::new(LocalFileStorage::new(
        config.storage.root.clone(),
        format!("{}{}", config.server.app_url, routes::FILES_PUBLIC_BASE),
    ));
    let broker = Arc::new(RedisMessageBroker::new(redis_client, cache));

    let state = AppState {
        auth: AuthState::new(
            users.clone(),
            refresh_tokens,
            uow,
            hasher,
            access_tokens.clone(),
            config.auth.refresh_ttl,
            config.auth.cookie_secure,
        ),
        users: UserState::new(users.clone(), storage.clone()),
        files: FileState::new(storage),
        chat: ChatState::new(chat_messages, users, access_tokens, broker),
    };

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
