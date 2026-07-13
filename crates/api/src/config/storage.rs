use std::path::PathBuf;

use super::env::parse_or;
use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Directory uploaded files are stored under.
    pub root: PathBuf,
}

impl StorageConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            root: parse_or("STORAGE_ROOT", PathBuf::from("storage"))?,
        })
    }
}
