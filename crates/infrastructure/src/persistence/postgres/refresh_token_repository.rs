use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::{RefreshToken, UserId};
use domain::repositories::{RefreshTokenRepository, RepositoryError};

use super::map_sqlx_err;

#[derive(sqlx::FromRow)]
struct RefreshTokenRow {
    id: Uuid,
    user_id: Uuid,
    token_hash: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl From<RefreshTokenRow> for RefreshToken {
    fn from(row: RefreshTokenRow) -> Self {
        RefreshToken::from_parts(
            row.id,
            row.user_id.into(),
            row.token_hash,
            row.expires_at,
            row.created_at,
        )
    }
}

// --- Query functions ---
// Generic over the executor so the same SQL runs against the pool
// (auto-commit) or an open transaction (unit of work).

pub(super) async fn insert_refresh_token(
    exec: impl PgExecutor<'_>,
    token: &RefreshToken,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(token.id())
    .bind(token.user_id().as_uuid())
    .bind(token.token_hash())
    .bind(token.expires_at())
    .bind(token.created_at())
    .execute(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

pub(super) async fn find_refresh_token_by_hash(
    exec: impl PgExecutor<'_>,
    token_hash: &str,
) -> Result<Option<RefreshToken>, RepositoryError> {
    let row = sqlx::query_as::<_, RefreshTokenRow>(
        "SELECT id, user_id, token_hash, expires_at, created_at
         FROM refresh_tokens WHERE token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(row.map(RefreshToken::from))
}

pub(super) async fn delete_refresh_token(
    exec: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<bool, RepositoryError> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE id = $1")
        .bind(id)
        .execute(exec)
        .await
        .map_err(map_sqlx_err)?;
    Ok(result.rows_affected() > 0)
}

pub(super) async fn delete_refresh_tokens_for_user(
    exec: impl PgExecutor<'_>,
    user_id: UserId,
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id.as_uuid())
        .execute(exec)
        .await
        .map_err(map_sqlx_err)?;
    Ok(())
}

// --- Pool-backed repository (each call auto-commits) ---

pub struct PgRefreshTokenRepository {
    pool: PgPool,
}

impl PgRefreshTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn save(&self, token: &RefreshToken) -> Result<(), RepositoryError> {
        insert_refresh_token(&self.pool, token).await
    }

    async fn find_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError> {
        find_refresh_token_by_hash(&self.pool, token_hash).await
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepositoryError> {
        delete_refresh_token(&self.pool, id).await
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> Result<(), RepositoryError> {
        delete_refresh_tokens_for_user(&self.pool, user_id).await
    }
}
