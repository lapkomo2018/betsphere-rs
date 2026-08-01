//! Demo activity simulation: a cast of bot accounts that keeps a demo or
//! development deployment looking inhabited.
//!
//! The bots drive the same use cases the HTTP and WebSocket layers do, as
//! ordinary users with ordinary balances — nothing here reaches past the
//! application layer or writes a table by hand. That is the whole point: their
//! bets move real prices, their messages travel the outbox to the broker and
//! out to live subscribers, and a client watching the demo sees exactly the
//! traffic it would see from a busy room of people.
//!
//! It is a driving adapter, in the same ring as the HTTP API — it depends on
//! [`application`], and nothing depends on it but the composition root, which
//! only starts it when `DEMO_MODE` is on.

mod bettor;
mod cast;
mod chatter;
mod config;
mod content;
mod llm;
mod market_maker;

use std::sync::Arc;
use std::time::Duration;

use application::ApplicationError;
use application::ports::PasswordHasher;
use application::use_cases::market::{ListMarkets, MarketView};
use domain::entities::MarketStatus;
use domain::repositories::{
    BetRepository, ChatMessageRepository, MarketFilter, MarketRepository, MarketSort,
    UserRepository,
};
use rand::RngExt;
use rand::rngs::StdRng;

pub use config::{DemoConfig, LlmConfig};

use crate::bettor::Bettor;
use crate::chatter::Chatter;
use crate::llm::Llm;
use crate::market_maker::MarketMaker;

/// How many open markets the bots consider at a time. Well past the number the
/// maker keeps on the board, so "everything open" is what they really see.
const MARKET_SAMPLE: i64 = 50;

/// The simulation. Build one at the composition root, then [`run`](Self::run)
/// it on its own task; it never returns.
pub struct Simulation {
    users: Arc<dyn UserRepository>,
    markets: Arc<dyn MarketRepository>,
    bets: Arc<dyn BetRepository>,
    messages: Arc<dyn ChatMessageRepository>,
    hasher: Arc<dyn PasswordHasher>,
    config: DemoConfig,
}

impl Simulation {
    pub fn new(
        users: Arc<dyn UserRepository>,
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        messages: Arc<dyn ChatMessageRepository>,
        hasher: Arc<dyn PasswordHasher>,
        config: DemoConfig,
    ) -> Self {
        Self {
            users,
            markets,
            bets,
            messages,
            hasher,
            config,
        }
    }

    /// Provisions the bot accounts, then runs the three activity loops until
    /// the process ends. Failing to provision disables the simulation and
    /// leaves the rest of the server running: a demo that cannot start its
    /// bots is worth less than one, not zero.
    pub async fn run(self) {
        let cast = match cast::provision(&*self.users, &*self.hasher, &self.config).await {
            Ok(cast) => Arc::new(cast),
            Err(e) => {
                tracing::error!("demo simulation disabled, could not provision bots: {e}");
                return;
            }
        };

        tracing::warn!(
            bots = cast.len(),
            "demo simulation started: every @{} account shares the DEMO_BOT_PASSWORD and one of them is an admin — never point this at a real database",
            cast::EMAIL_DOMAIN,
        );

        let maker = MarketMaker::new(
            self.markets.clone(),
            self.bets.clone(),
            self.messages.clone(),
            self.users.clone(),
            cast.clone(),
            self.config.market_every,
        );
        let bettor = Bettor::new(
            self.markets.clone(),
            self.bets.clone(),
            self.users.clone(),
            cast.clone(),
            self.config.bet_every,
        );
        let chatter = Chatter::new(
            self.markets,
            self.messages,
            self.users,
            cast,
            self.config.chat_every,
            self.config.llm.as_ref().and_then(connect_model),
        );

        // Three loops on one task: they are IO-bound and deliberately slow, so
        // interleaving them costs nothing and keeps the demo to a single
        // spawned future.
        tokio::join!(maker.run(), bettor.run(), chatter.run());
    }
}

/// Builds the client for the configured model server. Nothing is sent yet, so
/// this says only that the client could be built — an unreachable server shows
/// up later, one skipped line at a time.
fn connect_model(config: &LlmConfig) -> Option<Arc<Llm>> {
    match Llm::new(config) {
        Ok(llm) => {
            tracing::info!(
                model = config.model,
                url = config.url,
                "demo chatter is writing with a local model",
            );
            Some(Arc::new(llm))
        }
        Err(e) => {
            tracing::warn!("demo chatter falling back to canned lines: {e}");
            None
        }
    }
}

/// The markets the bots can act on, newest first. Includes ones past their
/// deadline: the market maker settles those, and the other loops skip them by
/// asking each market whether it still accepts bets.
async fn open_markets(list: &ListMarkets) -> Result<Vec<MarketView>, ApplicationError> {
    list.execute(&MarketFilter {
        status: Some(MarketStatus::Open),
        sort: MarketSort::Newest,
        limit: MARKET_SAMPLE,
        ..Default::default()
    })
    .await
}

/// Waits roughly `interval` before the next action, varied by ±40% so the
/// three loops drift apart instead of firing in lockstep forever.
async fn sleep_around(interval: Duration, rng: &mut StdRng) {
    let spread = rng.random_range(0.6..1.4);
    tokio::time::sleep(interval.mul_f64(spread)).await;
}

#[cfg(test)]
mod tests {
    use crate::cast::Cast;
    use domain::entities::{ChatChannel, ChatMessage, MessageId};
    use domain::repositories::BetFilter;
    use infrastructure::auth::Argon2PasswordHasher;
    use infrastructure::persistence::in_memory::{
        InMemoryBetRepository, InMemoryChatMessageRepository, InMemoryMarketRepository,
        InMemoryUserRepository,
    };
    use rand::SeedableRng;
    use rand::rngs::SysRng;

    use super::*;

    /// An empty platform with a provisioned cast over it, and the loops wired
    /// to the same stores so a test can step them one action at a time.
    struct World {
        users: Arc<InMemoryUserRepository>,
        markets: Arc<InMemoryMarketRepository>,
        bets: Arc<InMemoryBetRepository>,
        messages: Arc<InMemoryChatMessageRepository>,
        cast: Arc<Cast>,
    }

    const CAST_SIZE: usize = 3;

    impl World {
        async fn new() -> Self {
            let users = Arc::new(InMemoryUserRepository::new());
            let markets = Arc::new(InMemoryMarketRepository::new());
            let bets = Arc::new(InMemoryBetRepository::new(markets.clone(), users.clone()));
            let messages = Arc::new(InMemoryChatMessageRepository::new());
            let cast = Arc::new(provision(&users).await);

            Self {
                users,
                markets,
                bets,
                messages,
                cast,
            }
        }

        fn maker(&self) -> MarketMaker {
            MarketMaker::new(
                self.markets.clone(),
                self.bets.clone(),
                self.messages.clone(),
                self.users.clone(),
                self.cast.clone(),
                Duration::ZERO,
            )
        }

        fn bettor(&self) -> Bettor {
            Bettor::new(
                self.markets.clone(),
                self.bets.clone(),
                self.users.clone(),
                self.cast.clone(),
                Duration::ZERO,
            )
        }

        /// `None` is the canned path — which is also where every model failure
        /// lands, so it is the one that has to work.
        fn chatter(&self, llm: Option<Arc<Llm>>) -> Chatter {
            Chatter::new(
                self.markets.clone(),
                self.messages.clone(),
                self.users.clone(),
                self.cast.clone(),
                Duration::ZERO,
                llm,
            )
        }

        /// Everything said in the global room and in every market room.
        async fn transcript(&self) -> Vec<ChatMessage> {
            let mut said = self
                .messages
                .list_recent(ChatChannel::Global, 100, None)
                .await
                .unwrap();
            for market in self.markets.list(&MarketFilter::default()).await.unwrap() {
                said.extend(
                    self.messages
                        .list_recent(ChatChannel::Market(market.id()), 100, None)
                        .await
                        .unwrap(),
                );
            }
            said
        }

        /// How many reactions stand on everything said so far.
        async fn reactions(&self) -> i64 {
            let ids: Vec<MessageId> = self
                .transcript()
                .await
                .iter()
                .map(ChatMessage::id)
                .collect();
            self.messages
                .reactions_for(&ids, self.cast.host().actor.user_id)
                .await
                .unwrap()
                .values()
                .flatten()
                .map(|tally| tally.count)
                .sum()
        }
    }

    async fn provision(users: &InMemoryUserRepository) -> Cast {
        let config = DemoConfig {
            bots: CAST_SIZE,
            ..DemoConfig::default()
        };
        cast::provision(users, &Argon2PasswordHasher::new(), &config)
            .await
            .expect("provisioning")
    }

    /// The bots outlive any one run of the server, so a restart has to find the
    /// accounts it created last time instead of minting a second cast.
    #[tokio::test]
    async fn provisioning_resumes_an_existing_cast() {
        let users = InMemoryUserRepository::new();

        let first = provision(&users).await;
        let second = provision(&users).await;

        assert_eq!(first.host().actor.user_id, second.host().actor.user_id);
        assert_eq!(first.host().actor.role, domain::entities::Role::Admin);
        assert_eq!(users.list().await.unwrap().len(), CAST_SIZE);
    }

    #[tokio::test]
    async fn a_round_of_the_maker_opens_a_market_and_announces_it() {
        let world = World::new().await;
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();

        world.maker().tick(&mut rng).await.unwrap();

        let markets = world.markets.list(&MarketFilter::default()).await.unwrap();
        assert_eq!(markets.len(), 1);
        assert!(markets[0].accepts_bets(chrono::Utc::now()));

        let announcement = world.transcript().await;
        assert_eq!(announcement.len(), 1);
        assert!(
            announcement[0]
                .body()
                .as_str()
                .contains(markets[0].title().as_str())
        );
    }

    /// The stake has to land through the real use case: debited, priced, and
    /// counted into the market's volume.
    #[tokio::test]
    async fn a_bot_stakes_on_the_open_market() {
        let world = World::new().await;
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        world.maker().tick(&mut rng).await.unwrap();

        world.bettor().tick(&mut rng).await.unwrap();

        let placed = world.bets.feed(&BetFilter::default()).await.unwrap();
        assert_eq!(placed.len(), 1);

        let market = world.markets.list(&MarketFilter::default()).await.unwrap();
        assert_eq!(market[0].total_volume(), placed[0].amount());
        assert_eq!(market[0].participants_count(), 1);
    }

    /// Every turn either says something or reacts to something. Reacting is
    /// the minority case and taking a reaction back can find nothing to take,
    /// so this asks a handful of turns to leave *some* mark rather than
    /// pinning down which.
    #[tokio::test]
    async fn a_few_turns_of_chatter_leave_a_mark() {
        let world = World::new().await;
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        world.maker().tick(&mut rng).await.unwrap();

        let before = world.transcript().await.len() as i64 + world.reactions().await;
        let chatter = world.chatter(None);
        for _ in 0..8 {
            chatter.tick(&mut rng).await.unwrap();
        }

        let after = world.transcript().await.len() as i64 + world.reactions().await;
        assert!(after > before, "8 turns changed nothing");
    }

    /// The line a model answers with has to survive tidying, validation and
    /// the post use case to land in the room as that bot's message.
    #[tokio::test]
    async fn a_models_line_reaches_the_room() {
        const LINE: &str = "yes at 62% is free money";
        const ANSWER: &str = r#"{"choices":[{"message":{"content":"yes at 62% is free money"}}]}"#;

        let world = World::new().await;
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        world.maker().tick(&mut rng).await.unwrap();
        let announcement = world.transcript().await.len();

        let llm = Llm::new(&LlmConfig {
            url: llm::stub_server(ANSWER).await,
            model: "stub".to_owned(),
            timeout: Duration::from_secs(5),
        })
        .unwrap();
        let chatter = world.chatter(Some(Arc::new(llm)));

        // A turn can spend itself on a reaction, which posts nothing. Keep
        // taking turns until one is a message; 30 without a single one is not
        // a run of bad luck, it is a broken chatter.
        for _ in 0..30 {
            chatter.tick(&mut rng).await.unwrap();
            if world.transcript().await.len() > announcement {
                break;
            }
        }

        let said: Vec<String> = world
            .transcript()
            .await
            .iter()
            .map(|message| message.body().as_str().to_owned())
            .collect();

        assert!(
            said.iter().any(|body| body == LINE),
            "the model's line never reached the room: {said:?}",
        );
        // Nothing but the maker's announcement should be canned while the
        // model is answering every request.
        assert!(
            said.iter()
                .all(|body| body == LINE || body.starts_with("new market open:")),
            "a canned line slipped in: {said:?}",
        );
    }
}
