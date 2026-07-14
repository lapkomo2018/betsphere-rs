use axum::http::{HeaderValue, Method, header};
use tower_http::cors::{Any, CorsLayer};

use super::env::optional;
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct CorsConfig {
    /// Origins allowed to call the API from a browser.
    allowed_origins: Vec<String>,
}

impl CorsConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        let raw =
            optional("CORS_ALLOWED_ORIGINS").unwrap_or_else(|| "http://localhost:3000".into());
        let allowed_origins = raw
            .split(',')
            .map(|origin| origin.trim().to_owned())
            .filter(|origin| !origin.is_empty())
            .collect();
        Ok(Self { allowed_origins })
    }

    pub fn layer(&self) -> Result<CorsLayer, ConfigError> {
        let layer = CorsLayer::new()
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::PUT,
                Method::DELETE,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

        // Browsers reject wildcard origins combined with credentials, so `*`
        // runs without them: fine for token-in-header calls, but the refresh
        // cookie won't flow cross-origin. Dev convenience only.
        if self.allowed_origins.iter().any(|origin| origin == "*") {
            return Ok(layer.allow_origin(Any));
        }

        let origins = self
            .allowed_origins
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin).map_err(|e| ConfigError::Invalid {
                    key: "CORS_ALLOWED_ORIGINS",
                    reason: format!("invalid origin {origin}: {e}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(layer.allow_origin(origins).allow_credentials(true))
    }
}
