use application::ports::{MessageBroker, MessageBrokerError, MessageStream};
use async_trait::async_trait;
use futures::StreamExt;
use redis::Client;
use redis::aio::ConnectionManager;

/// Redis Pub/Sub-backed message broker.
///
/// `PUBLISH` goes through the shared multiplexed [`ConnectionManager`]; each
/// subscription gets its own dedicated connection, because a Redis connection
/// in subscribe mode can't be used for anything else. Since all fan-out flows
/// through Redis, the API instances hold no per-channel state and can be scaled
/// horizontally or restarted without dropping subscribers.
#[derive(Clone)]
pub struct RedisMessageBroker {
    client: Client,
    publisher: ConnectionManager,
}

impl RedisMessageBroker {
    pub fn new(client: Client, publisher: ConnectionManager) -> Self {
        Self { client, publisher }
    }
}

#[async_trait]
impl MessageBroker for RedisMessageBroker {
    async fn publish(&self, channel: &str, payload: Vec<u8>) -> Result<(), MessageBrokerError> {
        let mut conn = self.publisher.clone();
        redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| MessageBrokerError::Backend(e.to_string()))
    }

    async fn subscribe(&self, channel: &str) -> Result<MessageStream, MessageBrokerError> {
        let mut pubsub = self
            .client
            .get_async_pubsub()
            .await
            .map_err(|e| MessageBrokerError::Backend(e.to_string()))?;
        pubsub
            .subscribe(channel)
            .await
            .map_err(|e| MessageBrokerError::Backend(e.to_string()))?;

        // `into_on_message` owns the connection, so the stream stays alive for
        // as long as the subscriber holds it.
        let stream = pubsub
            .into_on_message()
            .filter_map(|msg| async move { msg.get_payload::<Vec<u8>>().ok() });
        Ok(Box::pin(stream))
    }
}
