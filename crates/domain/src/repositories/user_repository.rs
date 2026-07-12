use async_trait::async_trait;

use super::RepositoryError;
use crate::entities::{User, UserId};
use crate::value_objects::user::{Email, Username};

/// Port for user persistence. Implementations live in the infrastructure layer.
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn save(&self, user: &User) -> Result<(), RepositoryError>;

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError>;

    async fn find_by_username(&self, username: &Username) -> Result<Option<User>, RepositoryError>;

    async fn list(&self) -> Result<Vec<User>, RepositoryError>;
}
