//! Small helpers for reading typed values from the process environment.

use std::str::FromStr;

use super::error::ConfigError;

pub fn optional(key: &'static str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// For future required vars: `required("SOME_KEY")?`.
#[allow(dead_code)]
pub fn required(key: &'static str) -> Result<String, ConfigError> {
    optional(key).ok_or(ConfigError::Missing(key))
}

pub fn parse_or<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match optional(key) {
        None => Ok(default),
        Some(raw) => raw.parse().map_err(|e| ConfigError::Invalid {
            key,
            reason: format!("{e}"),
        }),
    }
}
