//! Infrastructure layer: concrete implementations of the domain's and
//! application's ports.
//!
//! Ships an in-memory store (dev/tests), a Postgres adapter (sqlx), a Redis
//! cache decorator, local-disk file storage, and the crypto/token services
//! (Argon2, JWT).

pub mod auth;
pub mod persistence;
pub mod storage;
