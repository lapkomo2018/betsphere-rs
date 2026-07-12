use super::env::{optional, parse_or};
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
    /// Full-URL override (DATABASE_URL); wins over the parts when set.
    url_override: Option<String>,
}

impl DatabaseConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            host: optional("POSTGRES_HOST").unwrap_or_else(|| "localhost".into()),
            port: parse_or("POSTGRES_PORT", 5432)?,
            user: optional("POSTGRES_USER").unwrap_or_else(|| "betsphere".into()),
            password: optional("POSTGRES_PASSWORD").unwrap_or_else(|| "betsphere".into()),
            name: optional("POSTGRES_DB").unwrap_or_else(|| "betsphere".into()),
            url_override: optional("DATABASE_URL"),
        })
    }

    /// Connection string assembled from the parts, unless DATABASE_URL was
    /// set explicitly (useful for managed hosting that hands out one URL).
    pub fn url(&self) -> String {
        match &self.url_override {
            Some(url) => url.clone(),
            None => format!(
                "postgres://{}:{}@{}:{}/{}",
                self.user, self.password, self.host, self.port, self.name
            ),
        }
    }
}
