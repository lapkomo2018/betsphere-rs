use std::sync::Arc;

use domain::repositories::RefreshTokenRepository;

use crate::use_cases::auth::session::hash_refresh_token;
use crate::ApplicationError;

/// Invalidates the presented refresh token. Idempotent: logging out with an
/// unknown or already-revoked token is not an error.
pub struct Logout {
    refresh_tokens: Arc<dyn RefreshTokenRepository>,
}

impl Logout {
    pub fn new(refresh_tokens: Arc<dyn RefreshTokenRepository>) -> Self {
        Self { refresh_tokens }
    }

    pub async fn execute(&self, raw_token: &str) -> Result<(), ApplicationError> {
        let hash = hash_refresh_token(raw_token);
        if let Some(record) = self.refresh_tokens.find_by_token_hash(&hash).await? {
            // Idempotent: a concurrent logout winning the delete is fine.
            let _ = self.refresh_tokens.delete(record.id()).await?;
        }
        Ok(())
    }
}
