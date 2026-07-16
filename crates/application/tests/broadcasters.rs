//! Broadcaster handlers driven over infrastructure's in-memory repositories
//! and broker.
//!
//! These are integration tests rather than `#[cfg(test)]` unit tests for a
//! reason worth knowing before moving them back: `infrastructure` depends on
//! `application`, so the dev-dependency that brings the in-memory fakes here
//! forms a cycle. Cargo permits it — dev-dependencies apply only to test
//! targets — but a unit test inside `src` links a *second*, `cfg(test)` copy
//! of `application`, distinct from the one the fakes implement their traits
//! against, and every `MessageBroker` bound fails to resolve. An integration
//! test links the same `application` rlib `infrastructure` does, so the traits
//! line up.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;

use application::broadcasters::{ChatMessageBroadcaster, MarketPriceUpdateBroadcaster};
use application::ports::{EventHandler, MessageBrokerExt};
use application::realtime::{ChatMessageBroadcast, PriceUpdateBroadcast};
use domain::entities::{
    ChatChannel, ChatMessage, Market, MarketId, MessageId, Outcome, OutcomeId, User,
};
use domain::events::{ChatMessagePosted, MarketPricesUpdated};
use domain::repositories::{ChatMessageRepository as _, MarketRepository as _, UserRepository as _};
use domain::value_objects::chat::MessageBody;
use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};
use domain::value_objects::user::{Email, PasswordHash, Username};
use infrastructure::messaging::InMemoryMessageBroker;
use infrastructure::persistence::in_memory::{
    InMemoryChatMessageRepository, InMemoryMarketRepository, InMemoryUserRepository,
};

#[tokio::test]
async fn broadcasts_the_message_with_its_author() {
    let messages = Arc::new(InMemoryChatMessageRepository::new());
    let users = Arc::new(InMemoryUserRepository::new());
    let broker = Arc::new(InMemoryMessageBroker::new());

    let author = User::new(
        Username::new("alice").unwrap(),
        Email::new("alice@example.com").unwrap(),
        PasswordHash::new("$argon2id$fake"),
    );
    users.save(&author).await.unwrap();
    let message = ChatMessage::new(
        author.id(),
        ChatChannel::Global,
        MessageBody::new("hello").unwrap(),
    );
    messages.save(&message).await.unwrap();

    let mut feed = broker
        .subscribe_broadcast::<ChatMessageBroadcast>(&ChatChannel::Global)
        .await
        .unwrap();

    let handler = ChatMessageBroadcaster::new(messages, users, broker.clone());
    handler
        .handle(&ChatMessagePosted {
            message_id: message.id(),
        })
        .await
        .unwrap();

    let broadcast = feed.next().await.unwrap();
    assert_eq!(broadcast.id, message.id());
    assert_eq!(broadcast.body, "hello");
    assert_eq!(broadcast.author.username, Username::new("alice").unwrap());
}

#[tokio::test]
async fn vanished_message_is_a_no_op() {
    let handler = ChatMessageBroadcaster::new(
        Arc::new(InMemoryChatMessageRepository::new()),
        Arc::new(InMemoryUserRepository::new()),
        Arc::new(InMemoryMessageBroker::new()),
    );
    handler
        .handle(&ChatMessagePosted {
            message_id: MessageId::new(),
        })
        .await
        .unwrap();
}

fn seeded_market() -> (Market, Vec<Outcome>) {
    let market = Market::new(MarketTitle::new("Will it rain?").unwrap(), None, None, None);
    let outcomes = vec![
        Outcome::new(
            market.id(),
            OutcomeLabel::new("Yes").unwrap(),
            Price::from_ten_thousandths(2_500).unwrap(),
        ),
        Outcome::new(
            market.id(),
            OutcomeLabel::new("No").unwrap(),
            Price::from_ten_thousandths(7_500).unwrap(),
        ),
    ];
    (market, outcomes)
}

#[tokio::test]
async fn broadcasts_the_market_current_prices() {
    let markets = Arc::new(InMemoryMarketRepository::new());
    let broker = Arc::new(InMemoryMessageBroker::new());
    let (market, outcomes) = seeded_market();
    markets.create(&market, &outcomes).await.unwrap();

    let mut feed = broker
        .subscribe_broadcast::<PriceUpdateBroadcast>(&market.id())
        .await
        .unwrap();

    let handler = MarketPriceUpdateBroadcaster::new(markets, broker.clone());
    handler
        .handle(&MarketPricesUpdated {
            market_id: market.id(),
        })
        .await
        .unwrap();

    let broadcast = feed.next().await.unwrap();
    let prices: HashMap<OutcomeId, f64> = broadcast
        .ticks
        .iter()
        .map(|t| (t.outcome_id, t.price.as_fraction()))
        .collect();
    assert_eq!(prices.len(), 2);
    assert_eq!(prices[&outcomes[0].id()], 0.25);
    assert_eq!(prices[&outcomes[1].id()], 0.75);
}

#[tokio::test]
async fn unknown_market_is_a_no_op() {
    let handler = MarketPriceUpdateBroadcaster::new(
        Arc::new(InMemoryMarketRepository::new()),
        Arc::new(InMemoryMessageBroker::new()),
    );
    handler
        .handle(&MarketPricesUpdated {
            market_id: MarketId::new(),
        })
        .await
        .unwrap();
}
