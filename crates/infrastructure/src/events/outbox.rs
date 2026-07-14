use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::events::DomainEvent;
use domain::repositories::RepositoryError;
use sqlx::postgres::PgListener;
use sqlx::{PgExecutor, PgPool};

use super::EventHandler;
use crate::persistence::postgres::map_sqlx_err;

/// Postgres NOTIFY channel that wakes the processor as soon as an event commits.
const CHANNEL: &str = "outbox_events";

/// Fallback sweep for events whose NOTIFY was missed (processor down or
/// reconnecting when they committed) and for retrying failed deliveries.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Events claimed per delivery pass.
const BATCH: i64 = 100;

/// Records `event` in the outbox and notifies the processor. Call with the
/// transaction that makes the change the event describes — the row and the
/// NOTIFY only take effect if that transaction commits.
pub async fn publish(
    exec: impl PgExecutor<'_>,
    event: &DomainEvent,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "WITH queued AS (INSERT INTO outbox_events (topic, payload) VALUES ($1, $2))
         SELECT pg_notify($3, '')",
    )
        .bind(event.topic())
        .bind(payload_for(event))
        .bind(CHANNEL)
        .execute(exec)
        .await
        .map_err(map_sqlx_err)?;
    Ok(())
}

/// Wire format of each event type. The inverse lives in the event's handler.
fn payload_for(event: &DomainEvent) -> serde_json::Value {
    match event {
        DomainEvent::UserBalanceChanged { user_id } => {
            serde_json::json!({ "user_id": user_id.as_uuid() })
        }
    }
}

/// Delivers outbox events to registered handlers. Run [`run`](Self::run) in
/// its own task; several instances coordinate safely through
/// `FOR UPDATE SKIP LOCKED`, each claiming disjoint batches.
pub struct OutboxProcessor {
    pool: PgPool,
    handlers: HashMap<String, Vec<Arc<dyn EventHandler>>>,
}

impl OutboxProcessor {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            handlers: HashMap::new(),
        }
    }

    pub fn with_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.handlers
            .entry(handler.topic().to_string())
            .or_default()
            .push(handler);
        self
    }

    /// Runs forever: drains the pending backlog, then wakes on each NOTIFY
    /// or, failing that, on the sweep interval.
    pub async fn run(self) {
        let mut listener = None;
        loop {
            if listener.is_none() {
                match PgListener::connect_with(&self.pool).await {
                    Ok(mut l) => match l.listen(CHANNEL).await {
                        Ok(()) => listener = Some(l),
                        Err(e) => tracing::warn!("outbox LISTEN failed: {e}"),
                    },
                    Err(e) => tracing::warn!("outbox listener connect failed: {e}"),
                }
            }

            self.drain().await;

            match &mut listener {
                Some(l) => {
                    let woke = tokio::select! {
                        received = l.recv() => received,
                        _ = tokio::time::sleep(SWEEP_INTERVAL) => {
                            self.prune().await;
                            continue;
                        }
                    };
                    if let Err(e) = woke {
                        tracing::warn!("outbox listener dropped, reconnecting: {e}");
                        listener = None;
                    }
                }
                // No listener; fall back to pure polling until reconnect works.
                None => tokio::time::sleep(SWEEP_INTERVAL).await,
            }
        }
    }

    /// Delivers pending events until the backlog is empty or a delivery
    /// fails (failed events wait for the sweep, giving transient errors —
    /// e.g. Redis down — breathing room instead of a hot retry loop).
    async fn drain(&self) {
        loop {
            match self.deliver_batch().await {
                Ok(delivered) if delivered == BATCH as usize => continue,
                Ok(_) => break,
                Err(e) => {
                    tracing::warn!("outbox delivery pass failed: {e}");
                    break;
                }
            }
        }
    }

    /// Claims one batch of pending events and delivers each to its handlers.
    /// Returns how many were delivered; failed ones stay pending with their
    /// attempt count bumped.
    async fn deliver_batch(&self) -> Result<usize, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let events: Vec<(i64, String, serde_json::Value)> = sqlx::query_as(
            "SELECT id, topic, payload FROM outbox_events
             WHERE processed_at IS NULL
             ORDER BY id LIMIT $1
             FOR UPDATE SKIP LOCKED",
        )
            .bind(BATCH)
            .fetch_all(&mut *tx)
            .await?;

        let mut delivered: Vec<i64> = Vec::new();
        let mut failed: Vec<i64> = Vec::new();
        for (id, topic, payload) in &events {
            let mut ok = true;
            for handler in self.handlers.get(topic).into_iter().flatten() {
                if let Err(e) = handler.handle(payload).await {
                    tracing::warn!("outbox event {id} ({topic}) failed: {e}");
                    ok = false;
                }
            }
            if ok {
                delivered.push(*id);
            } else {
                failed.push(*id);
            }
        }

        if !delivered.is_empty() {
            sqlx::query("UPDATE outbox_events SET processed_at = now() WHERE id = ANY($1)")
                .bind(&delivered)
                .execute(&mut *tx)
                .await?;
        }
        if !failed.is_empty() {
            sqlx::query("UPDATE outbox_events SET attempts = attempts + 1 WHERE id = ANY($1)")
                .bind(&failed)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(delivered.len())
    }

    /// Deletes processed events past their debugging shelf life.
    async fn prune(&self) {
        let result =
            sqlx::query("DELETE FROM outbox_events WHERE processed_at < now() - INTERVAL '1 day'")
                .execute(&self.pool)
                .await;
        if let Err(e) = result {
            tracing::warn!("outbox prune failed: {e}");
        }
    }
}
