use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use application::ports::{FileStorage, StoragePortError, StoredFile};
use async_trait::async_trait;

/// Stores files on the local filesystem under a root directory.
///
/// Keys map to paths below `root`; `public_url` prefixes them with
/// `public_base` — the absolute URL of the API route that serves them back
/// (app URL + the files route from the api crate).
pub struct LocalFileStorage {
    root: PathBuf,
    public_base: String,
}

impl LocalFileStorage {
    pub fn new(root: impl Into<PathBuf>, public_base: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            public_base: public_base.into(),
        }
    }

    /// Resolves a key to a path under `root`, rejecting anything that could
    /// escape it (`..`, absolute paths, separators or odd characters).
    fn resolve(&self, key: &str) -> Result<PathBuf, StoragePortError> {
        let valid_segment = |s: &str| {
            !s.is_empty()
                && !s.starts_with('.')
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        };
        if key.is_empty() || !key.split('/').all(valid_segment) {
            return Err(StoragePortError::InvalidKey);
        }
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl FileStorage for LocalFileStorage {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoragePortError> {
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io_err)?;
        }
        tokio::fs::write(&path, bytes).await.map_err(io_err)
    }

    async fn get(&self, key: &str) -> Result<Option<StoredFile>, StoragePortError> {
        // An invalid key (e.g. traversal attempt from a request path) simply
        // doesn't exist.
        let Ok(path) = self.resolve(key) else {
            return Ok(None);
        };
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(StoredFile {
                content_type: content_type_for(&path),
                bytes,
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(e)),
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StoragePortError> {
        let path = self.resolve(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err(e)),
        }
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{key}", self.public_base.trim_end_matches('/'))
    }
}

fn io_err(err: std::io::Error) -> StoragePortError {
    StoragePortError::Internal(format!("file storage io error: {err}"))
}

/// Guesses the content type from the file extension.
fn content_type_for(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> LocalFileStorage {
        let dir = std::env::temp_dir().join(format!("betsphere-test-{}", uuid::Uuid::new_v4()));
        LocalFileStorage::new(dir, "/api/files")
    }

    #[tokio::test]
    async fn put_get_delete_round_trip() {
        let storage = storage();
        storage.put("avatars/a.png", b"png-bytes").await.unwrap();

        let file = storage.get("avatars/a.png").await.unwrap().unwrap();
        assert_eq!(file.bytes, b"png-bytes");
        assert_eq!(file.content_type, "image/png");

        storage.delete("avatars/a.png").await.unwrap();
        assert!(storage.get("avatars/a.png").await.unwrap().is_none());
        // Deleting again is not an error.
        storage.delete("avatars/a.png").await.unwrap();
    }

    #[tokio::test]
    async fn rejects_keys_that_escape_the_root() {
        let storage = storage();
        for key in [
            "../evil",
            "avatars/../../evil",
            "/etc/passwd",
            "a\\b",
            ".hidden",
            "",
        ] {
            assert!(
                matches!(
                    storage.put(key, b"x").await,
                    Err(StoragePortError::InvalidKey)
                ),
                "key {key:?} should be rejected"
            );
            assert!(storage.get(key).await.unwrap().is_none());
        }
    }

    #[test]
    fn public_url_joins_base_and_key() {
        let storage = LocalFileStorage::new("root", "http://localhost:8080/api/files/");
        assert_eq!(
            storage.public_url("avatars/a.png"),
            "http://localhost:8080/api/files/avatars/a.png"
        );
    }
}
