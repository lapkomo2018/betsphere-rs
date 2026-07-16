use std::sync::Arc;

use application::ports::EventHandler;
use async_trait::async_trait;
use domain::events::UserBalanceChanged;

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
impl EventHandler<UserBalanceChanged> for UserCacheInvalidator {
    async fn handle(&self, event: &UserBalanceChanged) -> Result<(), String> {
        if self.users.evict(event.user_id).await {
            Ok(())
        } else {
            // Unconfirmed delete (Redis unreachable): stay pending and retry.
            Err(format!("could not evict user {} from cache", event.user_id))
        }
    }
}
