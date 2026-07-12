use std::sync::Arc;

use async_trait::async_trait;

use domain::repositories::{
    RefreshTokenRepository, RepositoryError, TransactionScope, UnitOfWork, UserRepository,
};

/// Unit of work over the in-memory stores. NOTE: writes apply immediately and
/// are NOT rolled back if the scope is dropped without commit — good enough
/// for tests, not a real transaction.
pub struct InMemoryUnitOfWork {
    users: Arc<dyn UserRepository>,
    refresh_tokens: Arc<dyn RefreshTokenRepository>,
}

impl InMemoryUnitOfWork {
    pub fn new(
        users: Arc<dyn UserRepository>,
        refresh_tokens: Arc<dyn RefreshTokenRepository>,
    ) -> Self {
        Self {
            users,
            refresh_tokens,
        }
    }
}

#[async_trait]
impl UnitOfWork for InMemoryUnitOfWork {
    async fn begin(&self) -> Result<Box<dyn TransactionScope>, RepositoryError> {
        Ok(Box::new(InMemoryScope {
            users: self.users.clone(),
            refresh_tokens: self.refresh_tokens.clone(),
        }))
    }
}

struct InMemoryScope {
    users: Arc<dyn UserRepository>,
    refresh_tokens: Arc<dyn RefreshTokenRepository>,
}

#[async_trait]
impl TransactionScope for InMemoryScope {
    fn users(&self) -> &dyn UserRepository {
        self.users.as_ref()
    }

    fn refresh_tokens(&self) -> &dyn RefreshTokenRepository {
        self.refresh_tokens.as_ref()
    }

    async fn commit(self: Box<Self>) -> Result<(), RepositoryError> {
        Ok(())
    }
}
