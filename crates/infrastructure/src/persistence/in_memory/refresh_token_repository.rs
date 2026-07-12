use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use domain::entities::{RefreshToken, UserId};
use domain::repositories::{RefreshTokenRepository, RepositoryError};

/// Thread-safe in-memory refresh-token store. Useful for development and tests.
#[derive(Default)]
pub struct InMemoryRefreshTokenRepository {
    tokens: RwLock<HashMap<Uuid, RefreshToken>>,
}

impl InMemoryRefreshTokenRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RefreshTokenRepository for InMemoryRefreshTokenRepository {
    async fn save(&self, token: &RefreshToken) -> Result<(), RepositoryError> {
        self.tokens.write().await.insert(token.id(), token.clone());
        Ok(())
    }

    async fn find_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError> {
        Ok(self
            .tokens
            .read()
            .await
            .values()
            .find(|t| t.token_hash() == token_hash)
            .cloned())
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepositoryError> {
        Ok(self.tokens.write().await.remove(&id).is_some())
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> Result<(), RepositoryError> {
        self.tokens
            .write()
            .await
            .retain(|_, t| t.user_id() != user_id);
        Ok(())
    }
}
