//! The bot that keeps the board stocked: it opens a market when a fresh
//! question is available and settles one whose deadline has passed, then
//! announces both in chat the way a human operator would.

use std::sync::Arc;
use std::time::Duration;

use application::ApplicationError;
use application::use_cases::chat::PostMessage;
use application::use_cases::market::{CreateMarket, ListMarkets, NewMarket, ResolveMarket};
use chrono::Utc;
use domain::entities::{ChatChannel, Outcome};
use domain::repositories::{
    BetRepository, ChatMessageRepository, MarketRepository, UserRepository,
};
use rand::SeedableRng;
use rand::rngs::{StdRng, SysRng};
use rand::seq::IndexedRandom;

use crate::cast::Cast;
use crate::content::{MARKET_TEMPLATES, MarketTemplate};
use crate::{open_markets, sleep_around};

pub(crate) struct MarketMaker {
    list: ListMarkets,
    create: CreateMarket,
    resolve: ResolveMarket,
    announce: PostMessage,
    cast: Arc<Cast>,
    interval: Duration,
}

impl MarketMaker {
    pub fn new(
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        cast: Arc<Cast>,
        interval: Duration,
    ) -> Self {
        Self {
            list: ListMarkets::new(markets.clone()),
            create: CreateMarket::new(markets.clone()),
            resolve: ResolveMarket::new(markets.clone(), bets),
            announce: PostMessage::new(messages, users, markets),
            cast,
            interval,
        }
    }

    pub async fn run(self) {
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        loop {
            sleep_around(self.interval, &mut rng).await;
            if let Err(e) = self.tick(&mut rng).await {
                tracing::warn!("demo market maker: {e}");
            }
        }
    }

    /// One round: settle what is due, then open one new question. Settling
    /// first keeps the board from growing past what the templates can fill.
    /// Crate-visible so the simulation can be stepped from a test without
    /// waiting on the loop's own clock.
    pub(crate) async fn tick(&self, rng: &mut StdRng) -> Result<(), ApplicationError> {
        self.settle_expired(rng).await?;
        self.open_market(rng).await
    }

    /// Opens one market whose question is not already live. Titles are the
    /// identity here: the same question can come back around once its previous
    /// round has resolved, but never twice at once.
    async fn open_market(&self, rng: &mut StdRng) -> Result<(), ApplicationError> {
        let live = open_markets(&self.list).await?;
        let available: Vec<&MarketTemplate> = MARKET_TEMPLATES
            .iter()
            .filter(|template| {
                !live
                    .iter()
                    .any(|view| view.market.title().as_str() == template.title)
            })
            .collect();

        let Some(template) = available.choose(rng) else {
            return Ok(()); // Every question is already on the board.
        };

        let view = self
            .create
            .execute(
                &self.cast.host().actor,
                NewMarket {
                    title: template.title.to_owned(),
                    description: Some(template.description.to_owned()),
                    category: Some(template.category.to_owned()),
                    closes_at: Some(Utc::now() + chrono::Duration::hours(template.open_for_hours)),
                    outcomes: template.outcomes.iter().map(|o| (*o).to_string()).collect(),
                },
            )
            .await?;

        tracing::info!(market = %view.market.id(), title = template.title, "demo market opened");

        // The global room is where anyone is watching, so that is where a new
        // market is worth mentioning; its own room is empty at this point.
        self.announce
            .execute(
                self.cast.host().actor.user_id,
                ChatChannel::Global,
                format!("new market open: {} 📊", template.title),
                None,
            )
            .await?;

        Ok(())
    }

    /// Settles one market that is past its deadline, if any is. One per round
    /// rather than all of them: a burst of resolutions reads as a batch job,
    /// and the payouts are more fun to watch arriving one at a time.
    async fn settle_expired(&self, rng: &mut StdRng) -> Result<(), ApplicationError> {
        let now = Utc::now();
        let live = open_markets(&self.list).await?;
        let expired: Vec<_> = live
            .iter()
            .filter(|view| !view.market.accepts_bets(now) && view.outcomes.len() >= 2)
            .collect();

        let Some(view) = expired.choose(rng) else {
            return Ok(());
        };
        let winner = pick_winner(&view.outcomes, rng);

        self.resolve
            .execute(&self.cast.host().actor, view.market.id(), winner.id())
            .await?;

        tracing::info!(
            market = %view.market.id(),
            outcome = winner.label().as_str(),
            "demo market resolved",
        );

        self.announce
            .execute(
                self.cast.host().actor.user_id,
                ChatChannel::Market(view.market.id()),
                format!("resolved: {} 🏁 paying out now", winner.label()),
                None,
            )
            .await?;

        Ok(())
    }
}

/// Picks the winning outcome weighted by its price, so the crowd is usually
/// right and occasionally spectacularly wrong — which is what makes the
/// resulting payouts, charts and chat worth looking at.
fn pick_winner<'a>(outcomes: &'a [Outcome], rng: &mut StdRng) -> &'a Outcome {
    outcomes
        .choose_weighted(rng, |outcome| {
            // An outcome nobody backed still has to be reachable.
            outcome.current_price().as_ten_thousandths().max(1)
        })
        .unwrap_or(&outcomes[0])
}
