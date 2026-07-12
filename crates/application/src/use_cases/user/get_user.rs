use std::sync::Arc;

use domain::entities::User;
use domain::repositories::UserRepository;
use uuid::Uuid;

use crate::ApplicationError;

pub struct GetUser {
    users: Arc<dyn UserRepository>,
}

impl GetUser {
    pub fn new(users: Arc<dyn UserRepository>) -> Self {
        Self { users }
    }

    pub async fn execute(&self, id: Uuid) -> Result<User, ApplicationError> {
        self.users
            .find_by_id(id.into())
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {id}")))
    }
}
