use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("entity already exists: {0}")]
    Conflict(String),

    #[error("storage error: {0}")]
    Storage(String),
}
