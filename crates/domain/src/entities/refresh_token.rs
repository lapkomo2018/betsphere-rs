use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::UserId;

/// A server-side refresh-token record. Only a SHA-256 hash of the raw token
/// is stored, so a database leak does not expose usable tokens.
#[derive(Debug, Clone)]
pub struct RefreshToken {
    id: Uuid,
    user_id: UserId,
    token_hash: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl RefreshToken {
    pub fn new(user_id: UserId, token_hash: String, expires_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            token_hash,
            expires_at,
            created_at: Utc::now(),
        }
    }

    /// Reconstructs a token from persisted state. Only repositories should call this.
    pub fn from_parts(
        id: Uuid,
        user_id: UserId,
        token_hash: String,
        expires_at: DateTime<Utc>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            user_id,
            token_hash,
            expires_at,
            created_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn expiry_check() {
        let now = Utc::now();
        let token = RefreshToken::new(UserId::new(), "hash".into(), now + Duration::hours(1));
        assert!(!token.is_expired(now));
        assert!(token.is_expired(now + Duration::hours(2)));
    }
}
