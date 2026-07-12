mod database;
mod env;
mod error;
mod server;

pub use database::DatabaseConfig;
pub use error::ConfigError;
pub use server::ServerConfig;

/// Typed application configuration, loaded once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    #[allow(dead_code)] // read once the Postgres adapter lands
    pub database: DatabaseConfig,
}

impl Config {
    /// Reads configuration from the process environment.
    ///
    /// Call `dotenvy::dotenv()` before this if you want `.env` support.
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            server: ServerConfig::from_env()?,
            database: DatabaseConfig::from_env()?,
        })
    }
}
