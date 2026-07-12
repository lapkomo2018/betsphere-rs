use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use domain::entities::{User, UserId};
use domain::repositories::{RepositoryError, UserRepository};
use domain::value_objects::user::{Email, PasswordHash, Username};

use super::{ConnectionManager, RedisCache};

/// Read-through / write-through cache in front of another [`UserRepository`],
/// backed by a [`RedisCache`].
///
/// Single-user lookups are cached under `user:id:{id}`, `user:email:{email}`,
/// and `user:username:{username}`, each holding the full serialized user.
/// Entries expire after `ttl`; `save` refreshes them. Writes that bypass this
/// decorator (e.g. inside a unit-of-work transaction) or change a user's
/// email/username leave stale entries for at most `ttl`, so keep it short.
/// `list` is never cached.
pub struct CachedUserRepository {
    inner: Arc<dyn UserRepository>,
    cache: RedisCache<CachedUser>,
}

impl CachedUserRepository {
    pub fn new(inner: Arc<dyn UserRepository>, redis: ConnectionManager, ttl: Duration) -> Self {
        Self {
            inner,
            cache: RedisCache::new(redis, ttl),
        }
    }
}

#[async_trait]
impl UserRepository for CachedUserRepository {
    async fn save(&self, user: &User) -> Result<(), RepositoryError> {
        self.inner.save(user).await?;
        self.cache.put(&user_keys(user), user).await;
        Ok(())
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        self.cache
            .get_or_load(
                &id_key(id),
                async || self.inner.find_by_id(id).await,
                user_keys,
            )
            .await
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError> {
        self.cache
            .get_or_load(
                &email_key(email),
                async || self.inner.find_by_email(email).await,
                user_keys,
            )
            .await
    }

    async fn find_by_username(&self, username: &Username) -> Result<Option<User>, RepositoryError> {
        self.cache
            .get_or_load(
                &username_key(username),
                async || self.inner.find_by_username(username).await,
                user_keys,
            )
            .await
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        self.inner.list().await
    }
}

fn id_key(id: UserId) -> String {
    format!("user:id:{id}")
}

fn email_key(email: &Email) -> String {
    format!("user:email:{}", email.as_str())
}

fn username_key(username: &Username) -> String {
    format!("user:username:{}", username.as_str())
}

/// Every key a user is looked up by; a cached user is stored under all of them.
fn user_keys(user: &User) -> Vec<String> {
    vec![
        id_key(user.id()),
        email_key(user.email()),
        username_key(user.username()),
    ]
}

/// Wire format of a cached user. A separate serde struct (mirroring the
/// Postgres row type) keeps the domain entity free of serialization concerns.
#[derive(Serialize, Deserialize)]
struct CachedUser {
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

impl From<&User> for CachedUser {
    fn from(user: &User) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().as_str().to_owned(),
            email: user.email().as_str().to_owned(),
            password_hash: user.password_hash().as_str().to_owned(),
            avatar_url: user.avatar_url().map(str::to_owned),
            balance: user.balance(),
            role: user.role().as_str().to_owned(),
            created_at: user.created_at(),
            updated_at: user.updated_at(),
        }
    }
}

impl TryFrom<CachedUser> for User {
    type Error = domain::DomainError;

    fn try_from(cached: CachedUser) -> Result<Self, Self::Error> {
        Ok(User::from_parts(
            cached.id.into(),
            Username::new(&cached.username)?,
            Email::new(&cached.email)?,
            PasswordHash::new(&cached.password_hash),
            cached.avatar_url,
            cached.balance,
            cached.role.parse()?,
            cached.created_at,
            cached.updated_at,
        ))
    }
}
