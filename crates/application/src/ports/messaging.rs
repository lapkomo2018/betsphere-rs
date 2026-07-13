use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageBrokerError {
    #[error("message broker backend error: {0}")]
    Backend(String),

    #[error("failed to encode message: {0}")]
    Encode(String),
}

/// A live stream of the raw byte payloads published to one channel.
pub type MessageStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

/// A live stream of typed messages decoded from one channel.
pub type TypedStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Generic publish/subscribe over named channels, carrying **opaque byte
/// payloads**.
///
/// [`publish`](MessageBroker::publish) delivers a payload to every subscriber
/// of `channel` across *all* running API instances;
/// [`subscribe`](MessageBroker::subscribe) yields that channel's live stream.
/// Backed by Redis Pub/Sub in production, which keeps the API instances
/// themselves stateless: no fan-out state lives in any single process, so
/// instances can be scaled out, restarted, or load-balanced freely.
///
/// The payload is bytes rather than a typed value on purpose: the broker is a
/// dumb transport shared by every real-time feature, and each feature owns its
/// own encoding. Callers that want typed Rust values use the JSON helpers on
/// [`MessageBrokerExt`]; callers relaying an already-encoded frame (e.g. the
/// chat WebSocket) use these raw methods and avoid a needless re-encode.
///
/// Kept object-safe (usable as `dyn MessageBroker`) so one broker instance
/// serves every channel and type; the generic typing lives in the extension
/// trait instead.
#[async_trait]
pub trait MessageBroker: Send + Sync {
    async fn publish(&self, channel: &str, payload: Vec<u8>) -> Result<(), MessageBrokerError>;

    async fn subscribe(&self, channel: &str) -> Result<MessageStream, MessageBrokerError>;
}

/// Typed JSON convenience layer over any [`MessageBroker`], mirroring how
/// `StreamExt` extends `Stream`. Blanket-implemented, so it works on a plain
/// `dyn MessageBroker` without touching the object-safe core.
#[async_trait]
pub trait MessageBrokerExt: MessageBroker {
    /// Serializes `value` as JSON and publishes it to `channel`.
    async fn publish_json<T>(&self, channel: &str, value: &T) -> Result<(), MessageBrokerError>
    where
        T: Serialize + Sync,
    {
        let payload =
            serde_json::to_vec(value).map_err(|e| MessageBrokerError::Encode(e.to_string()))?;
        self.publish(channel, payload).await
    }

    /// Subscribes to `channel` and decodes each payload from JSON into `T`.
    /// Payloads that fail to decode are logged and skipped, so one bad message
    /// can't tear down the stream.
    async fn subscribe_json<T>(&self, channel: &str) -> Result<TypedStream<T>, MessageBrokerError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let raw = self.subscribe(channel).await?;
        let stream = raw.filter_map(|payload| async move {
            match serde_json::from_slice::<T>(&payload) {
                Ok(value) => Some(value),
                Err(e) => {
                    tracing::warn!("dropping undecodable broker message: {e}");
                    None
                }
            }
        });
        Ok(Box::pin(stream))
    }

    /// Publishes a value that encodes itself into bytes. For message types that
    /// own their wire format (raw bytes, bincode, protobuf, ...) via
    /// `Into<Vec<u8>>`; use [`publish_json`](Self::publish_json) for plain
    /// serde structs.
    async fn publish_encoded<T>(&self, channel: &str, value: T) -> Result<(), MessageBrokerError>
    where
        T: Into<Vec<u8>> + Send,
    {
        self.publish(channel, value.into()).await
    }

    /// Subscribes to `channel` and decodes each payload with the message type's
    /// own `TryFrom<Vec<u8>>` codec (the fallible counterpart to
    /// [`publish_encoded`](Self::publish_encoded)). Undecodable payloads are
    /// logged and skipped.
    async fn subscribe_decoded<T>(
        &self,
        channel: &str,
    ) -> Result<TypedStream<T>, MessageBrokerError>
    where
        T: TryFrom<Vec<u8>> + Send + 'static,
        <T as TryFrom<Vec<u8>>>::Error: std::fmt::Display,
    {
        let raw = self.subscribe(channel).await?;
        let stream = raw.filter_map(|payload| async move {
            match T::try_from(payload) {
                Ok(value) => Some(value),
                Err(e) => {
                    tracing::warn!("dropping undecodable broker message: {e}");
                    None
                }
            }
        });
        Ok(Box::pin(stream))
    }
}

impl<B: MessageBroker + ?Sized> MessageBrokerExt for B {}
