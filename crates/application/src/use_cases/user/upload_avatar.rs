use std::sync::Arc;

use chrono::Utc;
use domain::DomainError;
use domain::entities::{User, UserId};
use domain::repositories::UserRepository;

use crate::ApplicationError;
use crate::ports::FileStorage;

/// Allowed avatar content types and the file extension each is stored under.
const ALLOWED_TYPES: [(&str, &str); 3] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
];

/// Maximum avatar size in bytes.
pub const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

pub struct UploadAvatar {
    users: Arc<dyn UserRepository>,
    storage: Arc<dyn FileStorage>,
}

impl UploadAvatar {
    pub fn new(users: Arc<dyn UserRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self { users, storage }
    }

    /// Stores the image and points the user's `avatar_url` at it.
    /// Returns the updated user.
    pub async fn execute(
        &self,
        user_id: UserId,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<User, ApplicationError> {
        let ext = ALLOWED_TYPES
            .iter()
            .find(|(ct, _)| *ct == content_type)
            .map(|(_, ext)| *ext)
            .ok_or_else(|| {
                DomainError::Validation(format!(
                    "unsupported avatar content type: {content_type:?} (allowed: png, jpeg, webp)"
                ))
            })?;

        if bytes.is_empty() {
            return Err(DomainError::Validation("avatar file is empty".into()).into());
        }
        if bytes.len() > MAX_AVATAR_BYTES {
            return Err(DomainError::Validation(format!(
                "avatar exceeds {MAX_AVATAR_BYTES} bytes"
            ))
            .into());
        }

        let mut user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {user_id}")))?;

        // The key is deterministic per user, so re-uploading overwrites the
        // old file; variants stored under a different extension are removed.
        let key = avatar_key(user_id, ext);
        for (_, other) in ALLOWED_TYPES.iter().filter(|(_, e)| *e != ext) {
            self.storage.delete(&avatar_key(user_id, other)).await?;
        }
        self.storage.put(&key, bytes).await?;

        // `?v=` busts client caches: the key stays the same but the URL changes.
        let url = format!(
            "{}?v={}",
            self.storage.public_url(&key),
            Utc::now().timestamp()
        );
        user.set_avatar_url(Some(url));
        self.users.save(&user).await?;

        Ok(user)
    }
}

fn avatar_key(user_id: UserId, ext: &str) -> String {
    format!("avatars/{user_id}.{ext}")
}
