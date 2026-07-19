use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::{ChatChannel, ChatMessage, MessageId};
use domain::events::ChatMessagePosted;
use domain::repositories::{ChatMessageRepository, MessageCursor, RepositoryError};
use domain::value_objects::chat::MessageBody;

use super::map_sqlx_err;
use crate::events::publish;

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
    cursor: Option<MessageCursor>,
) -> Result<Vec<ChatMessage>, RepositoryError> {
    // Filter to one room, take `limit` rows from the requested end, then flip
    // to oldest-first for display. Split into two statements because `= NULL`
    // never matches and `IS NOT DISTINCT FROM` can't use the room index.
    // $1 is the limit; the room takes $2 only when it is a market, so the
    // cursor's placeholders shift accordingly.
    let (room_filter, next) = match channel.market_id() {
        Some(_) => ("market_id = $2".to_owned(), 3),
        None => ("market_id IS NULL".to_owned(), 2),
    };
    // Row-value comparison so `(created_at, id)` is one ordering key: strictly
    // older than the anchor for `before`, strictly newer for `after`.
    let (cursor_filter, inner_order) = match cursor {
        None => (String::new(), "DESC"),
        Some(MessageCursor::Before(_)) => (
            format!("AND (created_at, id) < (${next}, ${})", next + 1),
            "DESC",
        ),
        // Paging forward walks up from the anchor, so scan ascending and take
        // the messages *nearest* it rather than the newest in the room.
        Some(MessageCursor::After(_)) => (
            format!("AND (created_at, id) > (${next}, ${})", next + 1),
            "ASC",
        ),
    };
    let query = format!(
        "SELECT {MESSAGE_COLUMNS} FROM (
             SELECT {MESSAGE_COLUMNS} FROM chat_messages
             WHERE {room_filter} {cursor_filter}
             ORDER BY created_at {inner_order}, id {inner_order}
             LIMIT $1
         ) page
         ORDER BY created_at ASC, id ASC"
    );
    let mut rows = sqlx::query_as::<_, ChatMessageRow>(&query).bind(limit);
    if let Some(market_id) = channel.market_id() {
        rows = rows.bind(market_id.as_uuid());
    }
    if let Some(MessageCursor::Before(anchor) | MessageCursor::After(anchor)) = cursor {
        rows = rows.bind(anchor.created_at).bind(anchor.id.as_uuid());
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
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;
        insert_message(&mut *tx, message).await?;
        // Committed together with the insert, so the broadcast to live
        // subscribers fires exactly when the message becomes real.
        publish(
            &mut *tx,
            &ChatMessagePosted {
                message_id: message.id(),
            },
        )
        .await?;
        tx.commit().await.map_err(map_sqlx_err)
    }

    async fn find_by_id(&self, id: MessageId) -> Result<Option<ChatMessage>, RepositoryError> {
        let query = format!("SELECT {MESSAGE_COLUMNS} FROM chat_messages WHERE id = $1");
        let row = sqlx::query_as::<_, ChatMessageRow>(&query)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        row.map(ChatMessage::try_from).transpose()
    }

    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
        cursor: Option<MessageCursor>,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        list_recent_messages(&self.pool, channel, limit, cursor).await
    }
}
