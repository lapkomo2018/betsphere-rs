use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoragePortError {
    #[error("invalid storage key")]
    InvalidKey,

    #[error("{0}")]
    Internal(String),
}

/// A file read back from storage.
#[derive(Debug, Clone)]
pub struct StoredFile {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Port for binary file storage (avatars, …). Implementations live in the
/// infrastructure layer (local disk for now, object storage later).
///
/// Keys are relative `/`-separated paths like `avatars/<id>.png`.
#[async_trait]
pub trait FileStorage: Send + Sync {
    /// Stores `bytes` under `key`, overwriting any existing file.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoragePortError>;

    /// Reads the file stored under `key`, or `None` if it does not exist.
    async fn get(&self, key: &str) -> Result<Option<StoredFile>, StoragePortError>;

    /// Removes the file under `key`. Deleting a missing file is not an error.
    async fn delete(&self, key: &str) -> Result<(), StoragePortError>;

    /// Public URL clients can fetch `key` from.
    fn public_url(&self, key: &str) -> String;
}
