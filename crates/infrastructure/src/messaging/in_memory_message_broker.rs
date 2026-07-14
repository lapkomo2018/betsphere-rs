use std::collections::HashMap;
use std::sync::Mutex;

use application::ports::{MessageBroker, MessageBrokerError, MessageStream};
use async_trait::async_trait;
use tokio::sync::broadcast;

/// Per-channel broadcast buffer size. A subscriber that falls this far behind
/// skips the backlog rather than stalling the channel (matching the Redis
/// adapter, which also drops overflowing messages for slow consumers).
const CAPACITY: usize = 256;

/// In-process message broker over per-channel [`tokio::sync::broadcast`]
/// channels.
///
/// Used in dev/tests where a single process serves every connection. It is
/// *not* stateless across instances — production uses
/// [`RedisMessageBroker`](super::RedisMessageBroker) for that.
#[derive(Default)]
pub struct InMemoryMessageBroker {
    channels: Mutex<HashMap<String, broadcast::Sender<Vec<u8>>>>,
}

impl InMemoryMessageBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the sender for `channel`, creating it on first use. Senders are
    /// retained so subscribers and publishers rendezvous on the same channel.
    fn sender(&self, channel: &str) -> broadcast::Sender<Vec<u8>> {
        let mut channels = self.channels.lock().expect("broker mutex poisoned");
        channels
            .entry(channel.to_owned())
            .or_insert_with(|| broadcast::channel(CAPACITY).0)
            .clone()
    }
}

#[async_trait]
impl MessageBroker for InMemoryMessageBroker {
    async fn publish(&self, channel: &str, payload: Vec<u8>) -> Result<(), MessageBrokerError> {
        // Errs only when there are no subscribers, which isn't a failure here.
        let _ = self.sender(channel).send(payload);
        Ok(())
    }

    async fn subscribe(&self, channel: &str) -> Result<MessageStream, MessageBrokerError> {
        let rx = self.sender(channel).subscribe();
        let stream = futures::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(payload) => return Some((payload, rx)),
                    // Skip past a lagged gap and keep delivering.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::MessageBrokerExt;
    use futures::StreamExt;

    /// A message type that owns its byte codec instead of going through serde.
    #[derive(Debug, PartialEq)]
    struct Ping(u8);

    impl From<Ping> for Vec<u8> {
        fn from(ping: Ping) -> Self {
            vec![ping.0]
        }
    }

    impl TryFrom<Vec<u8>> for Ping {
        type Error = String;

        fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
            bytes
                .first()
                .copied()
                .map(Ping)
                .ok_or("empty payload".into())
        }
    }

    #[tokio::test]
    async fn round_trips_types_via_their_own_codec() {
        let broker = InMemoryMessageBroker::new();
        // Subscribe before publishing so the message isn't dropped.
        let mut pings = broker.subscribe_decoded::<Ping>("ping").await.unwrap();

        broker.publish_encoded("ping", Ping(7)).await.unwrap();

        assert_eq!(pings.next().await, Some(Ping(7)));
    }
}
