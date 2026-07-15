use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::{ChatChannel, ChatMessage};
use domain::repositories::{ChatMessageRepository, RepositoryError};
use domain::value_objects::chat::MessageBody;

use super::map_sqlx_err;

const MESSAGE_COLUMNS: &str = "id, author_id, market_id, body, created_at";

#[derive(sqlx::FromRow)]
struct ChatMessageRow {
    id: Uuid,
    author_id: Uuid,
    market_id: Option<Uuid>,
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
            ChatChannel::from(row.market_id.map(Into::into)),
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
        "INSERT INTO chat_messages (id, author_id, market_id, body, created_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(message.id().as_uuid())
    .bind(message.author_id().as_uuid())
    .bind(message.channel().market_id().map(|id| id.as_uuid()))
    .bind(message.body().as_str())
    .bind(message.created_at())
    .execute(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

pub(super) async fn list_recent_messages(
    exec: impl PgExecutor<'_>,
    channel: ChatChannel,
    limit: i64,
) -> Result<Vec<ChatMessage>, RepositoryError> {
    // Filter to one room, take the newest `limit` rows, then flip to
    // oldest-first for display. Split into two statements because `= NULL`
    // never matches and `IS NOT DISTINCT FROM` can't use the room index.
    let room_filter = match channel.market_id() {
        Some(_) => "market_id = $2",
        None => "market_id IS NULL",
    };
    let query = format!(
        "SELECT {MESSAGE_COLUMNS} FROM (
             SELECT {MESSAGE_COLUMNS} FROM chat_messages
             WHERE {room_filter}
             ORDER BY created_at DESC
             LIMIT $1
         ) recent
         ORDER BY created_at ASC"
    );
    let mut rows = sqlx::query_as::<_, ChatMessageRow>(&query).bind(limit);
    if let Some(market_id) = channel.market_id() {
        rows = rows.bind(market_id.as_uuid());
    }
    let rows = rows.fetch_all(exec).await.map_err(map_sqlx_err)?;
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

    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        list_recent_messages(&self.pool, channel, limit).await
    }
}
