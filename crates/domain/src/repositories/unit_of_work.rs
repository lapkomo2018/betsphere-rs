use async_trait::async_trait;

use super::error::RepositoryError;
use super::{RefreshTokenRepository, UserRepository};

/// Port for running several repository operations atomically. Implementations
/// live in the infrastructure layer (a database transaction in production).
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn TransactionScope>, RepositoryError>;
}

/// Repositories bound to one open transaction. All calls made through this
/// scope commit or roll back together: call [`commit`](Self::commit) to
/// persist, or drop the scope to roll everything back.
#[async_trait]
pub trait TransactionScope: Send + Sync {
    fn users(&self) -> &dyn UserRepository;

    fn refresh_tokens(&self) -> &dyn RefreshTokenRepository;

    async fn commit(self: Box<Self>) -> Result<(), RepositoryError>;
}
