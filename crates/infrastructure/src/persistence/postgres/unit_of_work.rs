use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::Mutex;
use uuid::Uuid;

use domain::entities::{RefreshToken, User, UserId};
use domain::repositories::{
    RefreshTokenRepository, RepositoryError, TransactionScope, UnitOfWork, UserRepository,
};
use domain::value_objects::user::{Email, Username};

use super::{map_sqlx_err, refresh_token_repository as token_sql, user_repository as user_sql};

/// One open transaction, shared by the scope's repositories. sqlx requires
/// `&mut` access per statement while the repository traits take `&self`, so
/// the handle sits behind an async mutex (queries within one scope are
/// sequential anyway — the lock is never contended).
type SharedTx = Arc<Mutex<Transaction<'static, Postgres>>>;

/// Starts Postgres transactions. Cloning is cheap (shares the pool).
pub struct PgUnitOfWork {
    pool: PgPool,
}

impl PgUnitOfWork {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UnitOfWork for PgUnitOfWork {
    async fn begin(&self) -> Result<Box<dyn TransactionScope>, RepositoryError> {
        let tx: SharedTx = Arc::new(Mutex::new(self.pool.begin().await.map_err(map_sqlx_err)?));
        Ok(Box::new(PgTransactionScope {
            users: TxUserRepository { tx: tx.clone() },
            refresh_tokens: TxRefreshTokenRepository { tx: tx.clone() },
            tx,
        }))
    }
}

struct PgTransactionScope {
    tx: SharedTx,
    users: TxUserRepository,
    refresh_tokens: TxRefreshTokenRepository,
}

#[async_trait]
impl TransactionScope for PgTransactionScope {
    fn users(&self) -> &dyn UserRepository {
        &self.users
    }

    fn refresh_tokens(&self) -> &dyn RefreshTokenRepository {
        &self.refresh_tokens
    }

    async fn commit(self: Box<Self>) -> Result<(), RepositoryError> {
        let PgTransactionScope {
            tx,
            users,
            refresh_tokens,
        } = *self;
        // The repositories hold the only other Arc clones; drop them so the
        // transaction can be taken out and committed.
        drop(users);
        drop(refresh_tokens);
        let tx = Arc::try_unwrap(tx)
            .map_err(|_| RepositoryError::Storage("transaction still borrowed at commit".into()))?
            .into_inner();
        tx.commit().await.map_err(map_sqlx_err)
    }
}

// If the scope is dropped without commit, `Transaction`'s own Drop impl
// rolls everything back — no explicit rollback path needed.

struct TxUserRepository {
    tx: SharedTx,
}

#[async_trait]
impl UserRepository for TxUserRepository {
    async fn save(&self, user: &User) -> Result<(), RepositoryError> {
        user_sql::save_user(&mut **self.tx.lock().await, user).await
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        user_sql::find_user_by_id(&mut **self.tx.lock().await, id).await
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError> {
        user_sql::find_user_by(&mut **self.tx.lock().await, "email", email.as_str()).await
    }

    async fn find_by_username(&self, username: &Username) -> Result<Option<User>, RepositoryError> {
        user_sql::find_user_by(&mut **self.tx.lock().await, "username", username.as_str()).await
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        user_sql::list_users(&mut **self.tx.lock().await).await
    }
}

struct TxRefreshTokenRepository {
    tx: SharedTx,
}

#[async_trait]
impl RefreshTokenRepository for TxRefreshTokenRepository {
    async fn save(&self, token: &RefreshToken) -> Result<(), RepositoryError> {
        token_sql::insert_refresh_token(&mut **self.tx.lock().await, token).await
    }

    async fn find_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError> {
        token_sql::find_refresh_token_by_hash(&mut **self.tx.lock().await, token_hash).await
    }

    async fn delete(&self, id: Uuid) -> Result<bool, RepositoryError> {
        token_sql::delete_refresh_token(&mut **self.tx.lock().await, id).await
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> Result<(), RepositoryError> {
        token_sql::delete_refresh_tokens_for_user(&mut **self.tx.lock().await, user_id).await
    }
}
