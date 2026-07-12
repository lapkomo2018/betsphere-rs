//! Application layer: use cases that orchestrate domain objects.
//!
//! Depends only on the domain crate. Knows nothing about HTTP or storage
//! details — it talks to persistence exclusively through repository ports.

pub mod error;
pub mod use_cases;

pub use error::ApplicationError;
