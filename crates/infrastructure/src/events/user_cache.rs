use std::sync::Arc;

use async_trait::async_trait;
use domain::entities::UserId;
use domain::events::DomainEvent;

use super::EventHandler;
use crate::persistence::redis::CachedUserRepository;

/// Keeps the read-through user cache coherent with balance changes made in
/// raw SQL transactions (bet stakes and payouts): evicts the changed user so
/// the next lookup reloads the committed balance. Idempotent — evicting an
/// absent entry is a no-op — as at-least-once delivery requires.
pub struct UserCacheInvalidator {
    users: Arc<CachedUserRepository>,
}

impl UserCacheInvalidator {
    pub fn new(users: Arc<CachedUserRepository>) -> Self {
        Self { users }
    }
}

#[async_trait]
impl EventHandler for UserCacheInvalidator {
    fn topic(&self) -> &'static str {
        DomainEvent::USER_BALANCE_CHANGED
    }

    async fn handle(&self, payload: &serde_json::Value) -> Result<(), String> {
        let user_id = payload
            .get("user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .ok_or_else(|| format!("malformed {} payload: {payload}", self.topic()))?;

        if self.users.evict(UserId::from(user_id)).await {
            Ok(())
        } else {
            // Unconfirmed delete (Redis unreachable): stay pending and retry.
            Err(format!("could not evict user {user_id} from cache"))
        }
    }
}
