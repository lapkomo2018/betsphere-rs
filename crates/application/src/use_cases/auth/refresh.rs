use std::sync::Arc;

use chrono::Utc;
use domain::repositories::UnitOfWork;

use crate::use_cases::auth::session::{hash_refresh_token, AuthSession, SessionIssuer};
use crate::ApplicationError;

/// Exchanges a valid refresh token for a new token pair (rotation: the old
/// refresh token is invalidated in the same transaction).
pub struct RefreshSession {
    uow: Arc<dyn UnitOfWork>,
    sessions: Arc<SessionIssuer>,
}

impl RefreshSession {
    pub fn new(uow: Arc<dyn UnitOfWork>, sessions: Arc<SessionIssuer>) -> Self {
        Self { uow, sessions }
    }

    pub async fn execute(&self, raw_token: &str) -> Result<AuthSession, ApplicationError> {
        let invalid = || ApplicationError::Unauthorized("invalid refresh token".into());

        let tx = self.uow.begin().await?;

        let record = tx
            .refresh_tokens()
            .find_by_token_hash(&hash_refresh_token(raw_token))
            .await?
            .ok_or_else(invalid)?;

        // Single use: consuming the token must succeed. Under concurrent
        // refreshes only one transaction's DELETE reports an affected row;
        // the loser gets `false` here and a 401.
        if !tx.refresh_tokens().delete(record.id()).await? {
            return Err(invalid());
        }

        if record.is_expired(Utc::now()) {
            // Keep the deletion of the stale token.
            tx.commit().await?;
            return Err(invalid());
        }

        let user = tx
            .users()
            .find_by_id(record.user_id())
            .await?
            .ok_or_else(invalid)?;

        let session = self.sessions.issue(user, tx.refresh_tokens()).await?;
        tx.commit().await?;

        Ok(session)
    }
}
