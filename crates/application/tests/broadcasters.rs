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

use application::broadcasters::{
    ChatMessageBroadcaster, ChatReactionBroadcaster, MarketPriceUpdateBroadcaster,
};
use application::ports::{EventHandler, MessageBrokerExt};
use application::realtime::{ChatMessageBroadcast, ChatReactionBroadcast, PriceUpdateBroadcast};
use domain::entities::{
    ChatChannel, ChatMessage, Market, MarketId, MessageId, Outcome, OutcomeId, User, UserId,
};
use domain::events::{ChatMessagePosted, ChatReactionChanged, MarketPricesUpdated};
use domain::repositories::{
    ChatMessageRepository as _, MarketRepository as _, UserRepository as _,
};
use domain::value_objects::chat::{MessageBody, ReactionEmoji};
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
        None,
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
    assert!(broadcast.reply_to.is_none());
}

#[tokio::test]
async fn broadcasts_a_reply_with_the_message_it_quotes() {
    let messages = Arc::new(InMemoryChatMessageRepository::new());
    let users = Arc::new(InMemoryUserRepository::new());
    let broker = Arc::new(InMemoryMessageBroker::new());

    let author = User::new(
        Username::new("alice").unwrap(),
        Email::new("alice@example.com").unwrap(),
        PasswordHash::new("$argon2id$fake"),
    );
    users.save(&author).await.unwrap();
    let quoted = ChatMessage::new(
        author.id(),
        ChatChannel::Global,
        MessageBody::new("the original").unwrap(),
        None,
    );
    let reply = ChatMessage::new(
        author.id(),
        ChatChannel::Global,
        MessageBody::new("agreed").unwrap(),
        Some(quoted.id()),
    );
    messages.save(&quoted).await.unwrap();
    messages.save(&reply).await.unwrap();

    let mut feed = broker
        .subscribe_broadcast::<ChatMessageBroadcast>(&ChatChannel::Global)
        .await
        .unwrap();

    let handler = ChatMessageBroadcaster::new(messages, users, broker.clone());
    handler
        .handle(&ChatMessagePosted {
            message_id: reply.id(),
        })
        .await
        .unwrap();

    let broadcast = feed.next().await.unwrap();
    let quote = broadcast.reply_to.expect("reply should carry its quote");
    assert_eq!(quote.id, quoted.id());
    assert_eq!(quote.body, "the original");
    assert_eq!(quote.author.username, Username::new("alice").unwrap());
}

#[tokio::test]
async fn broadcasts_the_reaction_tally_after_each_change() {
    let messages = Arc::new(InMemoryChatMessageRepository::new());
    let broker = Arc::new(InMemoryMessageBroker::new());

    let author = UserId::new();
    let message = ChatMessage::new(
        author,
        ChatChannel::Global,
        MessageBody::new("nice call").unwrap(),
        None,
    );
    messages.save(&message).await.unwrap();
    let emoji = ReactionEmoji::new("🔥").unwrap();
    let reactor = UserId::new();
    messages
        .add_reaction(message.id(), reactor, &emoji)
        .await
        .unwrap();

    let mut feed = broker
        .subscribe_broadcast::<ChatReactionBroadcast>(&ChatChannel::Global)
        .await
        .unwrap();

    let handler = ChatReactionBroadcaster::new(messages.clone(), broker.clone());
    let changed = |added| ChatReactionChanged {
        message_id: message.id(),
        user_id: reactor,
        emoji: "🔥".to_owned(),
        added,
    };
    handler.handle(&changed(true)).await.unwrap();

    let broadcast = feed.next().await.unwrap();
    assert_eq!(broadcast.message_id, message.id());
    assert_eq!(broadcast.emoji, "🔥");
    assert_eq!(broadcast.count, 1);
    assert!(broadcast.added);

    // The count is re-read, not stepped: once the reaction is gone the same
    // handler publishes zero rather than decrementing what it last sent.
    messages
        .remove_reaction(message.id(), reactor, &emoji)
        .await
        .unwrap();
    handler.handle(&changed(false)).await.unwrap();

    let broadcast = feed.next().await.unwrap();
    assert_eq!(broadcast.count, 0);
    assert!(!broadcast.added);
}

#[tokio::test]
async fn reaction_to_a_vanished_message_is_a_no_op() {
    let handler = ChatReactionBroadcaster::new(
        Arc::new(InMemoryChatMessageRepository::new()),
        Arc::new(InMemoryMessageBroker::new()),
    );
    handler
        .handle(&ChatReactionChanged {
            message_id: MessageId::new(),
            user_id: UserId::new(),
            emoji: "🔥".to_owned(),
            added: true,
        })
        .await
        .unwrap();
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
