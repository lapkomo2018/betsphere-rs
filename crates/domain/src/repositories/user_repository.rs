use async_trait::async_trait;
use thiserror::Error;

use crate::entities::{User, UserId};
use crate::value_objects::user::Email;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("entity already exists: {0}")]
    Conflict(String),

    #[error("storage error: {0}")]
    Storage(String),
}

/// Port for user persistence. Implementations live in the infrastructure layer.
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn save(&self, user: &User) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError>;

    async fn list(&self) -> Result<Vec<User>, RepositoryError>;
}
