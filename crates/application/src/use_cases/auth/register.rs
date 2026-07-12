use std::sync::Arc;

use domain::entities::User;
use domain::repositories::UnitOfWork;
use domain::value_objects::user::{Email, Password, Username};

use crate::ports::PasswordHasher;
use crate::use_cases::auth::session::{AuthSession, SessionIssuer};
use crate::ApplicationError;

pub struct RegisterInput {
    pub username: String,
    pub email: String,
    pub password: String,
}

pub struct Register {
    uow: Arc<dyn UnitOfWork>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<SessionIssuer>,
}

impl Register {
    pub fn new(
        uow: Arc<dyn UnitOfWork>,
        hasher: Arc<dyn PasswordHasher>,
        sessions: Arc<SessionIssuer>,
    ) -> Self {
        Self {
            uow,
            hasher,
            sessions,
        }
    }

    pub async fn execute(&self, input: RegisterInput) -> Result<AuthSession, ApplicationError> {
        let username = Username::new(input.username)?;
        let email = Email::new(input.email)?;
        let password = Password::new(input.password)?;

        // Hash before opening the transaction: Argon2 is deliberately slow
        // and shouldn't hold a connection.
        let password_hash = self.hasher.hash(&password)?;

        // User + first refresh token are created atomically. Early returns
        // drop the scope, which rolls back. Concurrent registrations with the
        // same email/username slip past the checks but hit the DB unique
        // constraints, surfacing as Conflict.
        let tx = self.uow.begin().await?;

        if tx.users().find_by_email(&email).await?.is_some() {
            return Err(ApplicationError::Conflict(format!(
                "user with email {email} already exists"
            )));
        }
        if tx.users().find_by_username(&username).await?.is_some() {
            return Err(ApplicationError::Conflict(format!(
                "username {username} is already taken"
            )));
        }

        let user = User::new(username, email, password_hash);
        tx.users().save(&user).await?;

        let session = self.sessions.issue(user, tx.refresh_tokens()).await?;
        tx.commit().await?;

        Ok(session)
    }
}
