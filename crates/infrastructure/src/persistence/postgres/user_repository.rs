use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::{User, UserId};
use domain::repositories::{RepositoryError, UserRepository};
use domain::value_objects::user::{Email, PasswordHash, Username};

use super::map_sqlx_err;

const USER_COLUMNS: &str =
    "id, username, email, password_hash, avatar_url, balance, role, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    avatar_url: Option<String>,
    balance: i64,
    role: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<UserRow> for User {
    type Error = RepositoryError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        let corrupt = |e: domain::DomainError| {
            RepositoryError::Storage(format!("corrupt user row {}: {e}", row.id))
        };
        Ok(User::from_parts(
            row.id.into(),
            Username::new(&row.username).map_err(corrupt)?,
            Email::new(&row.email).map_err(corrupt)?,
            PasswordHash::new(&row.password_hash),
            row.avatar_url.clone(),
            row.balance,
            row.role.parse().map_err(corrupt)?,
            row.created_at,
            row.updated_at,
        ))
    }
}

// --- Query functions ---
// Generic over the executor so the same SQL runs against the pool
// (auto-commit) or an open transaction (unit of work).

pub(super) async fn save_user(
    exec: impl PgExecutor<'_>,
    user: &User,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, avatar_url, balance, role, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (id) DO UPDATE SET
             username = EXCLUDED.username,
             email = EXCLUDED.email,
             password_hash = EXCLUDED.password_hash,
             avatar_url = EXCLUDED.avatar_url,
             balance = EXCLUDED.balance,
             role = EXCLUDED.role,
             updated_at = EXCLUDED.updated_at",
    )
        .bind(user.id().as_uuid())
        .bind(user.username().as_str())
        .bind(user.email().as_str())
        .bind(user.password_hash().as_str())
        .bind(user.avatar_url())
        .bind(user.balance())
        .bind(user.role().as_str())
        .bind(user.created_at())
        .bind(user.updated_at())
        .execute(exec)
        .await
        .map_err(map_sqlx_err)?;
    Ok(())
}

pub(super) async fn find_user_by_id(
    exec: impl PgExecutor<'_>,
    id: UserId,
) -> Result<Option<User>, RepositoryError> {
    let query = format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1");
    let row = sqlx::query_as::<_, UserRow>(&query)
        .bind(id.as_uuid())
        .fetch_optional(exec)
        .await
        .map_err(map_sqlx_err)?;
    row.map(User::try_from).transpose()
}

/// `column` must be a trusted identifier (never user input).
pub(super) async fn find_user_by(
    exec: impl PgExecutor<'_>,
    column: &str,
    value: &str,
) -> Result<Option<User>, RepositoryError> {
    let query = format!("SELECT {USER_COLUMNS} FROM users WHERE {column} = $1");
    let row = sqlx::query_as::<_, UserRow>(&query)
        .bind(value)
        .fetch_optional(exec)
        .await
        .map_err(map_sqlx_err)?;
    row.map(User::try_from).transpose()
}

pub(super) async fn list_users(exec: impl PgExecutor<'_>) -> Result<Vec<User>, RepositoryError> {
    let query = format!("SELECT {USER_COLUMNS} FROM users ORDER BY created_at");
    let rows = sqlx::query_as::<_, UserRow>(&query)
        .fetch_all(exec)
        .await
        .map_err(map_sqlx_err)?;
    rows.into_iter().map(User::try_from).collect()
}

// --- Pool-backed repository (each call auto-commits) ---

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn save(&self, user: &User) -> Result<(), RepositoryError> {
        save_user(&self.pool, user).await
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        find_user_by_id(&self.pool, id).await
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError> {
        find_user_by(&self.pool, "email", email.as_str()).await
    }

    async fn find_by_username(&self, username: &Username) -> Result<Option<User>, RepositoryError> {
        find_user_by(&self.pool, "username", username.as_str()).await
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        list_users(&self.pool).await
    }
}
