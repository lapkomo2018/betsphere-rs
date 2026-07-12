use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use domain::entities::{RefreshToken, User};
use domain::repositories::RefreshTokenRepository;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ports::AccessTokenService;
use crate::ApplicationError;

/// The result of a successful register/login/refresh: the user plus a fresh
/// token pair. `refresh_token` is the raw value handed to the client (the
/// repository only ever sees its hash).
pub struct AuthSession {
    pub user: User,
    pub access_token: String,
    pub refresh_token: String,
    pub refresh_expires_at: DateTime<Utc>,
}

/// Issues access + refresh token pairs. Shared by the auth use cases.
///
/// The refresh-token repository is passed per call so the write can join
/// whatever transaction the calling use case has open.
pub struct SessionIssuer {
    access_tokens: Arc<dyn AccessTokenService>,
    refresh_ttl: Duration,
}

impl SessionIssuer {
    pub fn new(access_tokens: Arc<dyn AccessTokenService>, refresh_ttl: Duration) -> Self {
        Self {
            access_tokens,
            refresh_ttl,
        }
    }

    pub(crate) async fn issue(
        &self,
        user: User,
        refresh_tokens: &dyn RefreshTokenRepository,
    ) -> Result<AuthSession, ApplicationError> {
        let access_token = self.access_tokens.issue(user.id(), user.role())?;

        let refresh_token = generate_refresh_token();
        let record = RefreshToken::new(
            user.id(),
            hash_refresh_token(&refresh_token),
            Utc::now() + self.refresh_ttl,
        );
        refresh_tokens.save(&record).await?;

        Ok(AuthSession {
            user,
            access_token,
            refresh_token,
            refresh_expires_at: record.expires_at(),
        })
    }
}

/// 244 bits of randomness, hex-encoded.
fn generate_refresh_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Deterministic hash used as the storage key for refresh tokens.
pub(crate) fn hash_refresh_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}
