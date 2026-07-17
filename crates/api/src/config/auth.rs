use chrono::Duration;

use super::env::{optional, parse_or, required};
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Secret for signing HS256 access tokens. Required — there is no safe default.
    pub jwt_secret: String,
    pub access_ttl: Duration,
    pub refresh_ttl: Duration,
    /// Set the `Secure` flag on the refresh cookie (enable in production, behind HTTPS).
    pub cookie_secure: bool,
    /// Shared secret guarding the internal/system endpoints. When unset the
    /// internal API is disabled and every request to it is rejected.
    pub internal_api_key: Option<String>,
}

impl AuthConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            jwt_secret: required("JWT_SECRET")?,
            access_ttl: Duration::seconds(parse_or("ACCESS_TOKEN_TTL_SECS", 900)?),
            refresh_ttl: Duration::seconds(parse_or("REFRESH_TOKEN_TTL_SECS", 30 * 24 * 60 * 60)?),
            cookie_secure: parse_or("COOKIE_SECURE", false)?,
            internal_api_key: optional("INTERNAL_API_KEY"),
        })
    }
}
