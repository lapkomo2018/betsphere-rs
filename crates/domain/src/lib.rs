//! Domain layer: entities, value objects, and repository ports.
//!
//! This crate is the innermost circle of the architecture. It has no
//! knowledge of databases, web frameworks, or any other infrastructure.

pub mod entities;
pub mod error;
pub mod repositories;
pub mod services;
pub mod value_objects;

pub use error::DomainError;
