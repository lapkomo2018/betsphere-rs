use domain::entities::{Role, UserId};
use domain::value_objects::user::{Password, PasswordHash};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthPortError {
    #[error("invalid token")]
    InvalidToken,

    #[error("{0}")]
    Internal(String),
}

/// Hashes and verifies passwords (Argon2 in production).
pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &Password) -> Result<PasswordHash, AuthPortError>;

    fn verify(&self, password: &Password, hash: &PasswordHash) -> Result<bool, AuthPortError>;
}

/// Claims carried by a verified access token.
#[derive(Debug, Clone, Copy)]
pub struct AccessClaims {
    pub user_id: UserId,
    pub role: Role,
}

/// Signs and verifies short-lived access tokens (JWT in production).
pub trait AccessTokenService: Send + Sync {
    fn issue(&self, user_id: UserId, role: Role) -> Result<String, AuthPortError>;

    fn verify(&self, token: &str) -> Result<AccessClaims, AuthPortError>;
}
