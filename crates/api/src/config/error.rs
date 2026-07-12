use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var {0}")]
    Missing(&'static str),

    #[error("invalid value for {key}: {reason}")]
    Invalid { key: &'static str, reason: String },
}
