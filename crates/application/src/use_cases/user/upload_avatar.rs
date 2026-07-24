use std::sync::Arc;

use domain::entities::{User, UserId};
use domain::repositories::UserRepository;

use crate::ApplicationError;
use crate::ports::FileStorage;
use crate::use_cases::image;

/// Maximum avatar size in bytes.
pub const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

/// Storage folder avatars live in.
const FOLDER: &str = "avatars";

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
        let ext = image::validate("avatar", content_type, bytes, MAX_AVATAR_BYTES)?;

        let mut user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {user_id}")))?;

        let url = image::store(&*self.storage, FOLDER, &user_id.to_string(), ext, bytes).await?;
        user.set_avatar_url(Some(url));
        self.users.save(&user).await?;

        Ok(user)
    }
}
