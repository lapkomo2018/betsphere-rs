//! The loop that puts money on the board.
//!
//! Bets go through [`PlaceBet`] like anyone else's: the stake is debited, the
//! outcome's volume and every price on the market are recalculated, a price
//! point is appended, and the resulting events reach live subscribers. So the
//! demo's charts, feed and balances are produced the same way a real one's
//! would be, not written in.

use std::sync::Arc;
use std::time::Duration;

use application::ApplicationError;
use application::use_cases::bet::{NewBet, PlaceBet};
use application::use_cases::market::{ListMarkets, MarketView};
use chrono::Utc;
use domain::entities::{Outcome, User};
use domain::repositories::{BetRepository, MarketRepository, UserRepository};
use rand::rngs::{StdRng, SysRng};
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

use crate::cast::{Bot, Cast};
use crate::{open_markets, sleep_around};

/// Stake range, in minimal currency units, rounded to whole [`STAKE_STEP`]s.
/// Small against the starting balance so a bot lasts many bets, and wide
/// enough that the feed does not read as a metronome.
const MIN_STAKE: i64 = 50;
const MAX_STAKE: i64 = 900;
const STAKE_STEP: i64 = 25;

/// How often a bot backs the outcome already in front. Betting purely at
/// random pulls every price back toward an even split and leaves the charts
/// flat; letting the favourite attract most of the flow makes prices diverge
/// and trend, which is what a price chart is for.
const FAVOURITE_BIAS: f64 = 0.6;

pub(crate) struct Bettor {
    list: ListMarkets,
    place: PlaceBet,
    users: Arc<dyn UserRepository>,
    cast: Arc<Cast>,
    interval: Duration,
}

impl Bettor {
    pub fn new(
        markets: Arc<dyn MarketRepository>,
        bets: Arc<dyn BetRepository>,
        users: Arc<dyn UserRepository>,
        cast: Arc<Cast>,
        interval: Duration,
    ) -> Self {
        Self {
            list: ListMarkets::new(markets.clone()),
            place: PlaceBet::new(markets, bets, users.clone()),
            users,
            cast,
            interval,
        }
    }

    pub async fn run(self) {
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        loop {
            sleep_around(self.interval, &mut rng).await;
            if let Err(e) = self.tick(&mut rng).await {
                tracing::warn!("demo bettor: {e}");
            }
        }
    }

    /// One bet. Crate-visible so the simulation can be stepped from a test
    /// without waiting on the loop's own clock.
    pub(crate) async fn tick(&self, rng: &mut StdRng) -> Result<(), ApplicationError> {
        let now = Utc::now();
        let markets = open_markets(&self.list).await?;
        let tradable: Vec<&MarketView> = markets
            .iter()
            .filter(|view| view.market.accepts_bets(now) && view.outcomes.len() >= 2)
            .collect();

        let Some(view) = tradable.choose(rng) else {
            return Ok(()); // Nothing to bet on yet; the maker will see to it.
        };

        let bot = self.cast.anyone(rng);
        let outcome = pick_outcome(&view.outcomes, rng);
        let amount = stake(rng);
        self.top_up(bot, amount).await?;

        self.place
            .execute(
                &bot.actor,
                NewBet {
                    outcome_id: outcome.id(),
                    amount,
                },
            )
            .await?;

        tracing::debug!(
            bot = bot.name,
            market = %view.market.id(),
            outcome = outcome.label().as_str(),
            amount,
            "demo bet placed",
        );

        Ok(())
    }

    /// Refills a bot that can no longer cover its next stake, back to the
    /// balance a new account starts with. Bots lose money like anyone else and
    /// a demo that quietly runs dry after an hour is worse than one that never
    /// started, so their funds are topped up rather than accounted for.
    async fn top_up(&self, bot: &Bot, amount: i64) -> Result<(), ApplicationError> {
        let mut user = self
            .users
            .find_by_id(bot.actor.user_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("bot {}", bot.name)))?;

        if user.balance() >= amount {
            return Ok(());
        }

        let target = User::STARTING_BALANCE.max(amount);
        user.credit(target - user.balance())?;
        self.users.save(&user).await?;
        tracing::debug!(
            bot = bot.name,
            balance = user.balance(),
            "demo bot topped up"
        );

        Ok(())
    }
}

/// Picks what to back: usually the market's favourite, otherwise anything.
fn pick_outcome<'a>(outcomes: &'a [Outcome], rng: &mut StdRng) -> &'a Outcome {
    let fallback = || &outcomes[0];

    if rng.random_bool(FAVOURITE_BIAS) {
        return outcomes
            .iter()
            .max_by_key(|outcome| outcome.current_price())
            .unwrap_or_else(fallback);
    }
    outcomes.choose(rng).unwrap_or_else(fallback)
}

/// A stake in whole [`STAKE_STEP`]s, so the feed reads like money rather than
/// like a random number generator.
fn stake(rng: &mut StdRng) -> i64 {
    let steps = rng.random_range((MIN_STAKE / STAKE_STEP)..=(MAX_STAKE / STAKE_STEP));
    steps * STAKE_STEP
}
