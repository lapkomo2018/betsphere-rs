//! Composition root: wires infrastructure into use cases and starts the server.

mod config;
mod error;
mod extract;
mod routes;
mod state;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use application::broadcasters::{
    BetPlacedBroadcaster, ChatMessageBroadcaster, MarketPriceUpdateBroadcaster,
};
use infrastructure::auth::{Argon2PasswordHasher, JwtAccessTokens};
use infrastructure::events::{OutboxProcessor, UserCacheInvalidator};
use infrastructure::messaging::RedisMessageBroker;
use infrastructure::persistence::postgres::{
    self, run_migrations, PgBetRepository, PgChatMessageRepository, PgMarketRepository,
    PgRefreshTokenRepository, PgUnitOfWork, PgUserRepository,
};
use infrastructure::persistence::redis::{self, CachedUserRepository};
use infrastructure::storage::LocalFileStorage;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::{
    AppState, AuthState, BetState, ChatState, FileState, MarketState, UserState, WsState,
};

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
    let markets = Arc::new(PgMarketRepository::new(pool.clone()));
    let bets = Arc::new(PgBetRepository::new(pool.clone()));

    let broker = Arc::new(RedisMessageBroker::new(redis_client, cache));

    // Repositories record state changes (balance moves, price moves, posted
    // chat messages) in the outbox; the processor delivers the events to keep
    // the user cache in sync and to broadcast to live WebSocket subscribers.
    let outbox = OutboxProcessor::new(pool.clone())
        .with_handler(UserCacheInvalidator::new(users.clone()))
        .with_handler(BetPlacedBroadcaster::new(bets.clone(), broker.clone()))
        .with_handler(MarketPriceUpdateBroadcaster::new(markets.clone(), broker.clone()))
        .with_handler(ChatMessageBroadcaster::new(
            chat_messages.clone(),
            users.clone(),
            broker.clone(),
        ));
    tokio::spawn(outbox.run());

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
        users: UserState::new(users.clone(), bets.clone(), storage.clone()),
        files: FileState::new(storage),
        chat: ChatState::new(chat_messages.clone(), users.clone(), markets.clone()),
        ws: WsState::new(
            chat_messages,
            users.clone(),
            markets.clone(),
            bets.clone(),
            access_tokens,
            broker,
        ),
        markets: MarketState::new(markets.clone(), bets.clone()),
        bets: BetState::new(bets, markets, users),
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
