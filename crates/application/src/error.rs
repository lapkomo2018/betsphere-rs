use domain::DomainError;
use domain::repositories::RepositoryError;
use thiserror::Error;

use crate::ports::{AuthPortError, StoragePortError};

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("{0} not found")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<RepositoryError> for ApplicationError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::Conflict(msg) => Self::Conflict(msg),
            RepositoryError::Storage(msg) => Self::Internal(msg),
        }
    }
}

impl From<AuthPortError> for ApplicationError {
    fn from(err: AuthPortError) -> Self {
        match err {
            AuthPortError::InvalidToken => Self::Unauthorized("invalid token".into()),
            AuthPortError::Internal(msg) => Self::Internal(msg),
        }
    }
}

impl From<StoragePortError> for ApplicationError {
    fn from(err: StoragePortError) -> Self {
        // Use cases only pass internally-built keys to writes, so an invalid
        // key is a bug, not user error.
        Self::Internal(err.to_string())
    }
}
