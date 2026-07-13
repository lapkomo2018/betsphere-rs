mod auth;
mod cors;
mod database;
mod env;
mod error;
mod redis;
mod server;
mod storage;

pub use auth::AuthConfig;
pub use cors::CorsConfig;
pub use database::DatabaseConfig;
pub use error::ConfigError;
pub use redis::RedisConfig;
pub use server::ServerConfig;
pub use storage::StorageConfig;

/// Typed application configuration, loaded once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub cors: CorsConfig,
    pub storage: StorageConfig,
}

impl Config {
    /// Reads configuration from the process environment.
    ///
    /// Call `dotenvy::dotenv()` before this if you want `.env` support.
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            server: ServerConfig::from_env()?,
            database: DatabaseConfig::from_env()?,
            redis: RedisConfig::from_env()?,
            auth: AuthConfig::from_env()?,
            cors: CorsConfig::from_env()?,
            storage: StorageConfig::from_env()?,
        })
    }
}
