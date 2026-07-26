//! Shared handling for user-supplied images (avatars, market and outcome
//! thumbnails): validation of the upload and storage under a deterministic key.

use chrono::Utc;
use domain::DomainError;

use crate::ApplicationError;
use crate::ports::FileStorage;

/// Allowed image content types and the file extension each is stored under.
const ALLOWED_TYPES: [(&str, &str); 3] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
];

/// Checks the content type and size of an upload, returning the extension it
/// should be stored under. `kind` names the image in error messages.
pub(super) fn validate(
    kind: &str,
    content_type: &str,
    bytes: &[u8],
    max_bytes: usize,
) -> Result<&'static str, DomainError> {
    let ext = ALLOWED_TYPES
        .iter()
        .find(|(ct, _)| *ct == content_type)
        .map(|(_, ext)| *ext)
        .ok_or_else(|| {
            DomainError::Validation(format!(
                "unsupported {kind} content type: {content_type:?} (allowed: png, jpeg, webp)"
            ))
        })?;

    if bytes.is_empty() {
        return Err(DomainError::Validation(format!("{kind} file is empty")));
    }
    if bytes.len() > max_bytes {
        return Err(DomainError::Validation(format!(
            "{kind} exceeds {max_bytes} bytes"
        )));
    }
    Ok(ext)
}

/// Stores `bytes` under `<folder>/<owner>.<ext>` and returns the URL clients
/// should use.
pub(super) async fn store(
    storage: &dyn FileStorage,
    folder: &str,
    owner: &str,
    ext: &str,
    bytes: &[u8],
) -> Result<String, ApplicationError> {
    let key_for = |ext: &str| format!("{folder}/{owner}.{ext}");

    // The key is deterministic per owner, so re-uploading overwrites the old
    // file; variants stored under a different extension are removed.
    for (_, other) in ALLOWED_TYPES.iter().filter(|(_, e)| *e != ext) {
        storage.delete(&key_for(other)).await?;
    }
    let key = key_for(ext);
    storage.put(&key, bytes).await?;

    // `?v=` busts client caches: the key stays the same but the URL changes.
    Ok(format!(
        "{}?v={}",
        storage.public_url(&key),
        Utc::now().timestamp()
    ))
}
