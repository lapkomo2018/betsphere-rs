//! Cross-instance messaging adapters.
//!
//! Implements the application's [`MessageBroker`](application::ports::MessageBroker)
//! port: a Redis Pub/Sub adapter for production (keeps the API stateless across
//! instances) and an in-process adapter for dev/tests. Both are generic over
//! channel names, so every real-time feature shares one broker.

mod in_memory_message_broker;
mod redis_message_broker;

pub use in_memory_message_broker::InMemoryMessageBroker;
pub use redis_message_broker::RedisMessageBroker;
