use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::value_objects::user::{Email, Username};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    username: Username,
    email: Email,
    created_at: DateTime<Utc>,
}

impl User {
    /// Creates a brand-new user.
    pub fn new(username: Username, email: Email) -> Self {
        Self {
            id: UserId::new(),
            username,
            email,
            created_at: Utc::now(),
        }
    }

    /// Reconstructs a user from persisted state. Only repositories should call this.
    pub fn from_parts(
        id: UserId,
        username: Username,
        email: Email,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            username,
            email,
            created_at,
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn username(&self) -> &Username {
        &self.username
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}
