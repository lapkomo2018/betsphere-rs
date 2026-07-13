use std::net::SocketAddr;

use super::env::{optional, parse_or};
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    /// Public base URL clients reach the API on (scheme + host, no trailing
    /// slash). Used to build absolute URLs, e.g. for uploaded files.
    pub app_url: String,
}

impl ServerConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        let app_url = optional("APP_URL")
            .unwrap_or_else(|| "http://localhost:8080".into())
            .trim_end_matches('/')
            .to_owned();
        Ok(Self {
            bind_addr: parse_or("BIND_ADDR", "127.0.0.1:8080".parse().unwrap())?,
            app_url,
        })
    }
}
