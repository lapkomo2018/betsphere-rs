use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::value_objects::user::{Email, PasswordHash, Username};
use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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

impl From<Uuid> for UserId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<UserId> for Uuid {
    fn from(id: UserId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
        }
    }
}

impl std::str::FromStr for Role {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "admin" => Ok(Self::Admin),
            other => Err(DomainError::Validation(format!("unknown role: {other}"))),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    username: Username,
    email: Email,
    password_hash: PasswordHash,
    avatar_url: Option<String>,
    /// Virtual currency in minimal units.
    balance: i64,
    role: Role,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl User {
    /// Virtual balance every new user starts with.
    pub const STARTING_BALANCE: i64 = 10_000;

    /// Creates a brand-new user with the starting balance and `user` role.
    pub fn new(username: Username, email: Email, password_hash: PasswordHash) -> Self {
        let now = Utc::now();
        Self {
            id: UserId::new(),
            username,
            email,
            password_hash,
            avatar_url: None,
            balance: Self::STARTING_BALANCE,
            role: Role::User,
            created_at: now,
            updated_at: now,
        }
    }

    /// Reconstructs a user from persisted state. Only repositories should call this.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        id: UserId,
        username: Username,
        email: Email,
        password_hash: PasswordHash,
        avatar_url: Option<String>,
        balance: i64,
        role: Role,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            username,
            email,
            password_hash,
            avatar_url,
            balance,
            role,
            created_at,
            updated_at,
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

    pub fn password_hash(&self) -> &PasswordHash {
        &self.password_hash
    }

    pub fn avatar_url(&self) -> Option<&str> {
        self.avatar_url.as_deref()
    }

    pub fn balance(&self) -> i64 {
        self.balance
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_gets_starting_balance_and_user_role() {
        let user = User::new(
            Username::new("alice").unwrap(),
            Email::new("alice@example.com").unwrap(),
            PasswordHash::new("$argon2id$fake"),
        );
        assert_eq!(user.balance(), User::STARTING_BALANCE);
        assert_eq!(user.role(), Role::User);
        assert!(user.avatar_url().is_none());
    }

    #[test]
    fn role_round_trips_through_str() {
        assert_eq!("admin".parse::<Role>().unwrap(), Role::Admin);
        assert_eq!(Role::User.as_str().parse::<Role>().unwrap(), Role::User);
        assert!("superuser".parse::<Role>().is_err());
    }
}
