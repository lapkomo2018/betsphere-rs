//! Application layer: use cases that orchestrate domain objects.
//!
//! Depends only on the domain crate. Knows nothing about HTTP or storage
//! details — it talks to persistence exclusively through repository ports
//! and to crypto/token services through the ports in [`ports`].
//!
//! Orchestration is triggered two ways: by a request, in [`use_cases`], and by
//! a committed domain event, in [`broadcasters`]. Both are plain application
//! logic; how a request arrives, and how an event is stored and delivered, are
//! infrastructure's business.

pub mod actor;
pub mod broadcasters;
pub mod error;
pub mod ports;
pub mod realtime;
pub mod use_cases;

pub use actor::Actor;
pub use error::ApplicationError;
