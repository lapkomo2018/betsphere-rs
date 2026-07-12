//! Infrastructure layer: concrete implementations of the domain's ports.
//!
//! Currently ships an in-memory store. A real database adapter (e.g. sqlx +
//! Postgres) would live here too, behind the same repository traits.

pub mod persistence;
