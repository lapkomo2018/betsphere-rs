//! Application layer: use cases that orchestrate domain objects.
//!
//! Depends only on the domain crate. Knows nothing about HTTP or storage
//! details — it talks to persistence exclusively through repository ports
//! and to crypto/token services through the ports in [`ports`].

pub mod actor;
pub mod error;
pub mod ports;
pub mod realtime;
pub mod use_cases;

pub use actor::Actor;
pub use error::ApplicationError;
