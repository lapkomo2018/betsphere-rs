use std::sync::Arc;

use domain::entities::User;
use domain::repositories::UserRepository;
use domain::value_objects::user::{Email, Username};

use crate::ApplicationError;

pub struct CreateUserInput {
    pub username: String,
    pub email: String,
}

pub struct CreateUser {
    users: Arc<dyn UserRepository>,
}

impl CreateUser {
    pub fn new(users: Arc<dyn UserRepository>) -> Self {
        Self { users }
    }

    pub async fn execute(&self, input: CreateUserInput) -> Result<User, ApplicationError> {
        let username = Username::new(input.username)?;
        let email = Email::new(input.email)?;

        if self.users.find_by_email(&email).await?.is_some() {
            return Err(ApplicationError::Conflict(format!(
                "user with email {email} already exists"
            )));
        }

        let user = User::new(username, email);
        self.users.save(&user).await?;
        Ok(user)
    }
}
