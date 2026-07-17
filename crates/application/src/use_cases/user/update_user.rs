use std::sync::Arc;

use domain::entities::{Role, User};
use domain::repositories::UserRepository;
use uuid::Uuid;

use crate::ApplicationError;

/// Privileged edit of a user's mutable fields. Every field is optional; a
/// `None` leaves that field untouched, so callers patch only what they send.
#[derive(Debug, Default)]
pub struct UpdateUserInput {
    pub role: Option<Role>,
}

impl UpdateUserInput {
    /// Whether the patch would change anything at all.
    fn is_empty(&self) -> bool {
        self.role.is_none()
    }
}

/// Applies an administrative patch to a user. Intended for the internal/system
/// endpoint — it performs no role check of its own, so the caller is
/// responsible for gating access.
pub struct UpdateUser {
    users: Arc<dyn UserRepository>,
}

impl UpdateUser {
    pub fn new(users: Arc<dyn UserRepository>) -> Self {
        Self { users }
    }

    /// Loads the user, applies the patch, and persists it. Returns the updated
    /// user. Errors with `NotFound` if no user has that id, or `Domain` if the
    /// patch carries no fields.
    pub async fn execute(
        &self,
        user_id: Uuid,
        input: UpdateUserInput,
    ) -> Result<User, ApplicationError> {
        if input.is_empty() {
            return Err(ApplicationError::Domain(domain::DomainError::Validation(
                "no fields to update".into(),
            )));
        }

        let mut user = self
            .users
            .find_by_id(user_id.into())
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {user_id}")))?;

        if let Some(role) = input.role {
            user.set_role(role);
        }

        self.users.save(&user).await?;
        Ok(user)
    }
}
