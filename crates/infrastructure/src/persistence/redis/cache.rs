use std::marker::PhantomData;
use std::time::Duration;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Upper bound on a single cache round-trip before giving up.
const CACHE_OP_TIMEOUT: Duration = Duration::from_millis(500);

/// TTL'd JSON cache over Redis for one record type `C`.
///
/// `C` is the wire format (a plain serde struct, mirroring how Postgres row
/// types work); domain values convert in with `C: From<&T>` and out with
/// `T: TryFrom<C>`, keeping serialization concerns out of the domain layer.
///
/// Fails open by design: every Redis error is logged as a warning and turned
/// into a miss (reads) or ignored (writes), and each operation is capped at
/// [`CACHE_OP_TIMEOUT`] so a broken Redis (e.g. one stuck in the connection
/// manager's reconnect backoff) delays a request by at most that much
/// instead of stalling it.
pub struct RedisCache<C> {
    redis: ConnectionManager,
    ttl: Duration,
    _record: PhantomData<fn() -> C>,
}

impl<C: Serialize + DeserializeOwned> RedisCache<C> {
    pub fn new(redis: ConnectionManager, ttl: Duration) -> Self {
        Self {
            redis,
            ttl,
            _record: PhantomData,
        }
    }

    /// Cached value under `key`; `None` on miss or any cache failure.
    pub async fn get<T: TryFrom<C>>(&self, key: &str) -> Option<T> {
        let mut conn = self.redis.clone();
        let raw: Option<String> = match bounded(conn.get(key)).await {
            Ok(raw) => raw,
            Err(e) => {
                tracing::warn!("redis GET {key} failed: {e}");
                return None;
            }
        };
        let raw = raw?;

        match serde_json::from_str::<C>(&raw).map(T::try_from) {
            Ok(Ok(value)) => Some(value),
            // Corrupt entry (schema drift, bad data): drop it and miss.
            _ => {
                tracing::warn!("dropping corrupt cache entry {key}");
                self.delete(key).await;
                None
            }
        }
    }

    /// Stores one record under every key in `keys`, each expiring after the
    /// cache's TTL.
    pub async fn put<T>(&self, keys: &[String], value: &T)
    where
        C: for<'a> From<&'a T>,
    {
        let record = match serde_json::to_string(&C::from(value)) {
            Ok(record) => record,
            Err(e) => {
                tracing::warn!("failed to serialize cache record for {keys:?}: {e}");
                return;
            }
        };
        let ttl = self.ttl.as_secs();
        let mut pipe = redis::pipe();
        for key in keys {
            pipe.set_ex(key, &record, ttl);
        }
        let mut conn = self.redis.clone();
        if let Err(e) = bounded(pipe.query_async::<()>(&mut conn)).await {
            tracing::warn!("failed to cache {keys:?}: {e}");
        }
    }

    /// Deletes `key`, returning whether the delete was confirmed. `false`
    /// (error or timeout) lets callers that need certainty retry.
    pub async fn delete(&self, key: &str) -> bool {
        let mut conn = self.redis.clone();
        match bounded(conn.del::<_, ()>(key)).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("redis DEL {key} failed: {e}");
                false
            }
        }
    }

    /// Read-through lookup: returns the value cached under `key`, otherwise
    /// loads from the source of truth and, when found, caches it under all
    /// of `keys_for(value)` (e.g. every alias the value is looked up by).
    pub async fn get_or_load<T, E>(
        &self,
        key: &str,
        load: impl AsyncFnOnce() -> Result<Option<T>, E>,
        keys_for: impl FnOnce(&T) -> Vec<String>,
    ) -> Result<Option<T>, E>
    where
        T: TryFrom<C>,
        C: for<'a> From<&'a T>,
    {
        if let Some(value) = self.get(key).await {
            return Ok(Some(value));
        }
        let found = load().await?;
        if let Some(value) = &found {
            self.put(&keys_for(value), value).await;
        }
        Ok(found)
    }
}

/// Runs a redis future with [`CACHE_OP_TIMEOUT`], flattening the timeout
/// into a redis error.
async fn bounded<T>(
    op: impl Future<Output = Result<T, redis::RedisError>>,
) -> Result<T, redis::RedisError> {
    tokio::time::timeout(CACHE_OP_TIMEOUT, op)
        .await
        .unwrap_or_else(|_: tokio::time::error::Elapsed| {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "cache operation timed out",
            )))
        })
}
