use std::net::SocketAddr;

use super::env::parse_or;
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
}

impl ServerConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: parse_or("BIND_ADDR", "127.0.0.1:8080".parse().unwrap())?,
        })
    }
}
