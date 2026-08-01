use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use domain::entities::{ChatChannel, ChatMessage, MessageId, UserId};
use domain::events::{ChatMessagePosted, ChatReactionChanged};
use domain::repositories::{ChatMessageRepository, MessageCursor, ReactionTally, RepositoryError};
use domain::value_objects::chat::{MessageBody, ReactionEmoji};

use super::map_sqlx_err;
use crate::events::publish;

const MESSAGE_COLUMNS: &str = "id, author_id, market_id, body, reply_to_id, created_at";

#[derive(sqlx::FromRow)]
struct ChatMessageRow {
    id: Uuid,
    author_id: Uuid,
    market_id: Option<Uuid>,
    body: String,
    reply_to_id: Option<Uuid>,
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
            row.reply_to_id.map(Into::into),
            row.created_at,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct ReactionTallyRow {
    message_id: Uuid,
    emoji: String,
    count: i64,
    reacted: bool,
}

// --- Query functions ---

pub(super) async fn insert_message(
    exec: impl PgExecutor<'_>,
    message: &ChatMessage,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO chat_messages (id, author_id, market_id, body, reply_to_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(message.id().as_uuid())
    .bind(message.author_id().as_uuid())
    .bind(message.channel().market_id().map(|id| id.as_uuid()))
    .bind(message.body().as_str())
    .bind(message.reply_to().map(|id| id.as_uuid()))
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

    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<ChatMessage>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = ids.iter().map(|id| id.as_uuid()).collect();
        let query = format!("SELECT {MESSAGE_COLUMNS} FROM chat_messages WHERE id = ANY($1)");
        let rows = sqlx::query_as::<_, ChatMessageRow>(&query)
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(ChatMessage::try_from).collect()
    }

    async fn list_recent(
        &self,
        channel: ChatChannel,
        limit: i64,
        cursor: Option<MessageCursor>,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        list_recent_messages(&self.pool, channel, limit, cursor).await
    }

    async fn add_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &ReactionEmoji,
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;
        // The primary key already says a user holds an emoji once; letting it
        // absorb the repeat is what makes a retried frame idempotent.
        let inserted = sqlx::query(
            "INSERT INTO chat_message_reactions (message_id, user_id, emoji)
             VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(message_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(emoji.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?
        .rows_affected()
            > 0;

        // Only a real change is worth an event: a no-op insert would otherwise
        // broadcast an unchanged tally to every subscriber.
        if inserted {
            publish_reaction_change(&mut *tx, message_id, user_id, emoji, true).await?;
        }
        tx.commit().await.map_err(map_sqlx_err)?;
        Ok(inserted)
    }

    async fn remove_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &ReactionEmoji,
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;
        let removed = sqlx::query(
            "DELETE FROM chat_message_reactions
             WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
        )
        .bind(message_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(emoji.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?
        .rows_affected()
            > 0;

        if removed {
            publish_reaction_change(&mut *tx, message_id, user_id, emoji, false).await?;
        }
        tx.commit().await.map_err(map_sqlx_err)?;
        Ok(removed)
    }

    async fn reactions_for(
        &self,
        message_ids: &[MessageId],
        viewer: UserId,
    ) -> Result<HashMap<MessageId, Vec<ReactionTally>>, RepositoryError> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<Uuid> = message_ids.iter().map(|id| id.as_uuid()).collect();
        // One grouped pass over the whole page: the tally and the viewer's own
        // membership in it both fall out of the same scan.
        let rows = sqlx::query_as::<_, ReactionTallyRow>(
            "SELECT message_id, emoji, COUNT(*) AS count,
                    COALESCE(bool_or(user_id = $2), false) AS reacted
             FROM chat_message_reactions
             WHERE message_id = ANY($1)
             GROUP BY message_id, emoji
             ORDER BY message_id, MIN(created_at), emoji",
        )
        .bind(&ids)
        .bind(viewer.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        let mut tallies: HashMap<MessageId, Vec<ReactionTally>> = HashMap::new();
        for row in rows {
            let emoji = ReactionEmoji::new(&row.emoji).map_err(|e| {
                RepositoryError::Storage(format!(
                    "corrupt reaction on chat message {}: {e}",
                    row.message_id
                ))
            })?;
            tallies
                .entry(row.message_id.into())
                .or_default()
                .push(ReactionTally {
                    emoji,
                    count: row.count,
                    reacted: row.reacted,
                });
        }
        Ok(tallies)
    }
}

/// Records a reaction change in the outbox, inside the transaction that made
/// it — same contract as the insert in [`ChatMessageRepository::save`].
async fn publish_reaction_change(
    exec: impl PgExecutor<'_>,
    message_id: MessageId,
    user_id: UserId,
    emoji: &ReactionEmoji,
    added: bool,
) -> Result<(), RepositoryError> {
    publish(
        exec,
        &ChatReactionChanged {
            message_id,
            user_id,
            emoji: emoji.as_str().to_owned(),
            added,
        },
    )
    .await
}
