use std::sync::Arc;

use domain::repositories::{RefreshTokenRepository, UserRepository};
use domain::value_objects::user::{Email, Password};

use crate::ApplicationError;
use crate::ports::PasswordHasher;
use crate::use_cases::auth::session::{AuthSession, SessionIssuer};

pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub struct Login {
    users: Arc<dyn UserRepository>,
    refresh_tokens: Arc<dyn RefreshTokenRepository>,
    hasher: Arc<dyn PasswordHasher>,
    sessions: Arc<SessionIssuer>,
}

impl Login {
    pub fn new(
        users: Arc<dyn UserRepository>,
        refresh_tokens: Arc<dyn RefreshTokenRepository>,
        hasher: Arc<dyn PasswordHasher>,
        sessions: Arc<SessionIssuer>,
    ) -> Self {
        Self {
            users,
            refresh_tokens,
            hasher,
            sessions,
        }
    }

    pub async fn execute(&self, input: LoginInput) -> Result<AuthSession, ApplicationError> {
        // Malformed credentials get the same answer as wrong ones so the
        // endpoint doesn't leak which accounts exist.
        let invalid = || ApplicationError::Unauthorized("invalid credentials".into());

        let email = Email::new(input.email).map_err(|_| invalid())?;
        let password = Password::new(input.password).map_err(|_| invalid())?;

        let user = self
            .users
            .find_by_email(&email)
            .await?
            .ok_or_else(invalid)?;

        if !self.hasher.verify(&password, user.password_hash())? {
            return Err(invalid());
        }

        // Single INSERT — no transaction needed.
        self.sessions
            .issue(user, self.refresh_tokens.as_ref())
            .await
    }
}
