use std::sync::Arc;

use crate::ApplicationError;
use crate::ports::{FileStorage, StoredFile};

pub struct GetFile {
    storage: Arc<dyn FileStorage>,
}

impl GetFile {
    pub fn new(storage: Arc<dyn FileStorage>) -> Self {
        Self { storage }
    }

    pub async fn execute(&self, key: &str) -> Result<StoredFile, ApplicationError> {
        self.storage
            .get(key)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("file {key}")))
    }
}
