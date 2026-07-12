mod cache;
mod user_repository;

use std::time::Duration;

pub use cache::RedisCache;
pub use user_repository::CachedUserRepository;

pub use redis::aio::ConnectionManager;
use redis::aio::ConnectionManagerConfig;

/// Connects to Redis and returns a connection manager that multiplexes one
/// connection and transparently reconnects after failures. Cloning it is
/// cheap; clones share the underlying connection.
///
/// Both timeouts are deliberately tight: the cache must fail fast so callers
/// fall through to the primary store instead of stalling requests. Without
/// them a command can hang indefinitely when Redis goes away but the TCP
/// endpoint still accepts connections. The max reconnect delay is capped
/// because the default backoff grows to a minute, keeping the cache degraded
/// long after Redis is back.
pub async fn connect(redis_url: &str) -> Result<ConnectionManager, redis::RedisError> {
    let client = redis::Client::open(redis_url)?;
    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Duration::from_secs(1))
        .set_response_timeout(Duration::from_secs(1))
        .set_max_delay(1_000); // milliseconds between reconnect attempts
    ConnectionManager::new_with_config(client, config).await
}
