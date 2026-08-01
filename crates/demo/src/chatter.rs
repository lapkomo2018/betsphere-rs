//! The loop that keeps the rooms talking.
//!
//! A turn is one of three things: a fresh line, a reply to something recent,
//! or a reaction. Which room it happens in is picked per turn, so the global
//! feed and the market rooms both stay alive, and what gets said in a market
//! room is filled in from that market's own outcomes and prices.
//!
//! Replies and reactions read the room's recent history first, which means the
//! bots answer real users as readily as they answer each other.

use std::sync::Arc;
use std::time::Duration;

use application::ApplicationError;
use application::use_cases::chat::{
    ChatMessageView, HistoryWindow, ListRecentMessages, PostMessage, ReactToMessage,
};
use application::use_cases::market::{ListMarkets, MarketView};
use chrono::Utc;
use domain::entities::{ChatChannel, UserId};
use domain::repositories::{ChatMessageRepository, MarketRepository, UserRepository};
use rand::rngs::{StdRng, SysRng};
use rand::seq::{IndexedRandom, IteratorRandom};
use rand::{RngExt, SeedableRng};

use crate::cast::{Bot, Cast};
use crate::content::{GLOBAL_LINES, MARKET_LINES, REACTIONS, REPLY_LINES};
use crate::{open_markets, sleep_around};

/// How much recent history a bot reads before deciding what to do with it.
const HISTORY_DEPTH: i64 = 20;

/// Chance a turn is spent reacting to a message instead of writing one.
const REACT_CHANCE: f64 = 0.35;
/// Chance a reaction is taken back rather than given. Deliberately low: it
/// only lands when that bot really did hold that emoji on that message.
const UNREACT_CHANCE: f64 = 0.15;
/// Chance a written line answers something already in the room.
const REPLY_CHANCE: f64 = 0.3;
/// Chance a turn happens in a market room rather than the global one.
const MARKET_ROOM_CHANCE: f64 = 0.55;
/// Chance a line in a market room is about that market, rather than small talk.
const ON_TOPIC_CHANCE: f64 = 0.75;

pub(crate) struct Chatter {
    list: ListMarkets,
    post: PostMessage,
    react: ReactToMessage,
    history: ListRecentMessages,
    cast: Arc<Cast>,
    interval: Duration,
}

impl Chatter {
    pub fn new(
        markets: Arc<dyn MarketRepository>,
        messages: Arc<dyn ChatMessageRepository>,
        users: Arc<dyn UserRepository>,
        cast: Arc<Cast>,
        interval: Duration,
    ) -> Self {
        Self {
            list: ListMarkets::new(markets.clone()),
            post: PostMessage::new(messages.clone(), users.clone(), markets.clone()),
            react: ReactToMessage::new(messages.clone()),
            history: ListRecentMessages::new(messages, users, markets),
            cast,
            interval,
        }
    }

    pub async fn run(self) {
        let mut rng = StdRng::try_from_rng(&mut SysRng).unwrap();
        loop {
            sleep_around(self.interval, &mut rng).await;
            if let Err(e) = self.tick(&mut rng).await {
                tracing::warn!("demo chatter: {e}");
            }
        }
    }

    /// One turn: a line, a reply, or a reaction. Crate-visible so the
    /// simulation can be stepped from a test without waiting on the loop's own
    /// clock.
    pub(crate) async fn tick(&self, rng: &mut StdRng) -> Result<(), ApplicationError> {
        let bot = self.cast.anyone(rng);
        let room = self.pick_room(rng).await?;
        let channel = room.as_ref().map_or(ChatChannel::Global, |view| {
            ChatChannel::Market(view.market.id())
        });

        // Read the room first — everything but a cold open depends on it.
        let recent = self
            .history
            .execute(
                bot.actor.user_id,
                channel,
                HISTORY_DEPTH,
                HistoryWindow::default(),
            )
            .await?;

        if !recent.is_empty() && rng.random_bool(REACT_CHANCE) {
            return self.react(bot, &recent, rng).await;
        }

        let reply_to = someone_else(&recent, bot.actor.user_id)
            .filter(|_| rng.random_bool(REPLY_CHANCE))
            .and_then(|target| target.choose(rng).copied());

        let body = match reply_to {
            Some(_) => pick(REPLY_LINES, rng).to_owned(),
            None => self.line(room.as_ref(), rng),
        };

        self.post
            .execute(
                bot.actor.user_id,
                channel,
                body,
                reply_to.map(|view| view.message.id()),
            )
            .await?;

        tracing::debug!(bot = bot.name, ?channel, "demo message posted");
        Ok(())
    }

    /// Gives or takes back one emoji on a recent message. Both directions are
    /// idempotent, so a take-back that finds nothing simply does nothing.
    async fn react(
        &self,
        bot: &Bot,
        recent: &[ChatMessageView],
        rng: &mut StdRng,
    ) -> Result<(), ApplicationError> {
        let Some(target) = recent.choose(rng) else {
            return Ok(());
        };
        let emoji = pick(REACTIONS, rng);
        let message = target.message.id();

        if rng.random_bool(UNREACT_CHANCE) {
            self.react.remove(bot.actor.user_id, message, emoji).await?;
        } else {
            self.react.add(bot.actor.user_id, message, emoji).await?;
        }

        tracing::debug!(bot = bot.name, emoji, "demo reaction");
        Ok(())
    }

    /// Which room to speak in: the global feed, or one of the markets still
    /// taking bets. `None` is the global room.
    async fn pick_room(&self, rng: &mut StdRng) -> Result<Option<MarketView>, ApplicationError> {
        if !rng.random_bool(MARKET_ROOM_CHANCE) {
            return Ok(None);
        }

        let now = Utc::now();
        let mut markets = open_markets(&self.list).await?;
        markets.retain(|view| view.market.accepts_bets(now) && !view.outcomes.is_empty());

        // Picking an index keeps the chosen market owned, so the caller can
        // read its outcomes without borrowing the whole listing.
        let Some(index) = (0..markets.len()).choose(rng) else {
            return Ok(None);
        };
        Ok(Some(markets.swap_remove(index)))
    }

    /// A line to open with. In a market room it is usually about the market,
    /// with the placeholders filled in from a real outcome and its price.
    fn line(&self, room: Option<&MarketView>, rng: &mut StdRng) -> String {
        let Some(view) = room.filter(|_| rng.random_bool(ON_TOPIC_CHANCE)) else {
            return pick(GLOBAL_LINES, rng).to_owned();
        };
        let Some(outcome) = view.outcomes.choose(rng) else {
            return pick(GLOBAL_LINES, rng).to_owned();
        };

        pick(MARKET_LINES, rng)
            .replace("{outcome}", outcome.label().as_str())
            .replace(
                "{price}",
                &format!("{:.0}%", outcome.current_price().as_fraction() * 100.0),
            )
            .replace("{title}", view.market.title().as_str())
    }
}

/// The messages in `recent` written by anyone but `bot`, or `None` when there
/// are none — a bot replying to itself reads as a glitch, not as chatter.
fn someone_else(recent: &[ChatMessageView], bot: UserId) -> Option<Vec<&ChatMessageView>> {
    let others: Vec<&ChatMessageView> = recent
        .iter()
        .filter(|view| view.author.id() != bot)
        .collect();
    (!others.is_empty()).then_some(others)
}

/// Picks one line out of a pool. The pools in [`content`](crate::content) are
/// non-empty consts, so the fallback is unreachable — it exists only so that a
/// pool someone later empties costs a dull message rather than a panic in a
/// background task.
fn pick<'a>(lines: &[&'a str], rng: &mut StdRng) -> &'a str {
    lines.choose(rng).copied().unwrap_or("gm")
}
