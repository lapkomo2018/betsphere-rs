use async_trait::async_trait;
use uuid::Uuid;

use super::error::RepositoryError;
use crate::entities::{RefreshToken, UserId};

/// Port for refresh-token persistence. Implementations live in the
/// infrastructure layer.
#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    async fn save(&self, token: &RefreshToken) -> Result<(), RepositoryError>;

    async fn find_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError>;

    /// Returns `true` if a token was actually deleted. Callers enforcing
    /// single-use semantics must treat `false` as "someone else consumed it".
    async fn delete(&self, id: Uuid) -> Result<bool, RepositoryError>;

    async fn delete_all_for_user(&self, user_id: UserId) -> Result<(), RepositoryError>;
}
