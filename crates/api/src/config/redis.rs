use std::time::Duration;

use super::env::{optional, parse_or};
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    /// Full-URL override (REDIS_URL); wins over the parts when set.
    url_override: Option<String>,
    /// How long cached entries live. Staleness is bounded by this value,
    /// so keep it short.
    pub cache_ttl: Duration,
}

impl RedisConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            host: optional("REDIS_HOST").unwrap_or_else(|| "localhost".into()),
            port: parse_or("REDIS_PORT", 6379)?,
            url_override: optional("REDIS_URL"),
            cache_ttl: Duration::from_secs(parse_or("CACHE_TTL_SECS", 60)?),
        })
    }

    /// Connection string assembled from the parts, unless REDIS_URL was set
    /// explicitly (useful for managed hosting that hands out one URL).
    pub fn url(&self) -> String {
        match &self.url_override {
            Some(url) => url.clone(),
            None => format!("redis://{}:{}", self.host, self.port),
        }
    }
}
