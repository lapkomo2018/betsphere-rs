use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::ChatMessage;
use domain::repositories::{ChatMessageRepository, RepositoryError};
use domain::value_objects::chat::MessageBody;

use super::map_sqlx_err;

const MESSAGE_COLUMNS: &str = "id, author_id, body, created_at";

#[derive(sqlx::FromRow)]
struct ChatMessageRow {
    id: Uuid,
    author_id: Uuid,
    body: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<ChatMessageRow> for ChatMessage {
    type Error = RepositoryError;

    fn try_from(row: ChatMessageRow) -> Result<Self, Self::Error> {
        let corrupt = |e: domain::DomainError| {
            RepositoryError::Storage(format!("corrupt chat message row {}: {e}", row.id))
        };
        Ok(ChatMessage::from_parts(
            row.id.into(),
            row.author_id.into(),
            MessageBody::new(&row.body).map_err(corrupt)?,
            row.created_at,
        ))
    }
}

// --- Query functions ---

pub(super) async fn insert_message(
    exec: impl PgExecutor<'_>,
    message: &ChatMessage,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO chat_messages (id, author_id, body, created_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(message.id().as_uuid())
    .bind(message.author_id().as_uuid())
    .bind(message.body().as_str())
    .bind(message.created_at())
    .execute(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

pub(super) async fn list_recent_messages(
    exec: impl PgExecutor<'_>,
    limit: i64,
) -> Result<Vec<ChatMessage>, RepositoryError> {
    // Take the newest `limit` rows, then flip to oldest-first for display.
    let query = format!(
        "SELECT {MESSAGE_COLUMNS} FROM (
             SELECT {MESSAGE_COLUMNS} FROM chat_messages
             ORDER BY created_at DESC
             LIMIT $1
         ) recent
         ORDER BY created_at ASC"
    );
    let rows = sqlx::query_as::<_, ChatMessageRow>(&query)
        .bind(limit)
        .fetch_all(exec)
        .await
        .map_err(map_sqlx_err)?;
    rows.into_iter().map(ChatMessage::try_from).collect()
}

// --- Pool-backed repository (each call auto-commits) ---

pub struct PgChatMessageRepository {
    pool: PgPool,
}

impl PgChatMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChatMessageRepository for PgChatMessageRepository {
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        insert_message(&self.pool, message).await
    }

    async fn list_recent(&self, limit: i64) -> Result<Vec<ChatMessage>, RepositoryError> {
        list_recent_messages(&self.pool, limit).await
    }
}
