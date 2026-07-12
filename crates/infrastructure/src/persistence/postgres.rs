mod refresh_token_repository;
mod unit_of_work;
mod user_repository;

pub use refresh_token_repository::PgRefreshTokenRepository;
pub use unit_of_work::PgUnitOfWork;
pub use user_repository::PgUserRepository;

use sqlx::postgres::PgPoolOptions;
pub use sqlx::PgPool;

/// SQL migrations embedded into the binary at compile time from the
/// workspace-root `migrations/` directory.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

/// Applies pending migrations. Safe to call on every startup: already-applied
/// migrations are skipped (tracked in the `_sqlx_migrations` table).
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// Maps sqlx errors onto the repository port's error type.
pub(crate) fn map_sqlx_err(err: sqlx::Error) -> domain::repositories::RepositoryError {
    use domain::repositories::RepositoryError;

    if let sqlx::Error::Database(db) = &err
        && db.is_unique_violation()
    {
        return RepositoryError::Conflict(db.message().to_owned());
    }
    RepositoryError::Storage(err.to_string())
}
