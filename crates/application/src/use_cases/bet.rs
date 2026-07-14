mod list_bet_feed;
mod list_market_bets;
mod list_user_bets;
mod place_bet;

pub use list_bet_feed::ListBetFeed;
pub use list_market_bets::ListMarketBets;
pub use list_user_bets::ListUserBets;
pub use place_bet::{NewBet, PlaceBet};

use std::collections::{HashMap, HashSet};

use domain::entities::{Bet, Market, Outcome, UserId};
use domain::repositories::{MarketRepository, UserRepository};

use crate::ApplicationError;

/// A bet joined with the display names a feed or history entry needs, so
/// clients don't have to resolve ids themselves.
pub struct BetView {
    pub bet: Bet,
    pub username: String,
    pub market_title: String,
    pub outcome_label: String,
}

/// Joins bets with their market titles, outcome labels, and usernames.
/// Markets and outcomes are fetched in one batch each; users one by one
/// (behind the read-through cache in production).
pub(crate) async fn enrich(
    bets: Vec<Bet>,
    markets: &dyn MarketRepository,
    users: &dyn UserRepository,
) -> Result<Vec<BetView>, ApplicationError> {
    let market_ids: Vec<_> = {
        let unique: HashSet<_> = bets.iter().map(Bet::market_id).collect();
        unique.into_iter().collect()
    };
    let markets_by_id: HashMap<_, _> = markets
        .find_by_ids(&market_ids)
        .await?
        .into_iter()
        .map(|m| (m.id(), m))
        .collect();
    let labels_by_id: HashMap<_, _> = markets
        .outcomes_for_markets(&market_ids)
        .await?
        .into_iter()
        .map(|o| (o.id(), o.label().to_string()))
        .collect();

    let mut usernames: HashMap<UserId, String> = HashMap::new();
    for user_id in bets.iter().map(Bet::user_id) {
        if let std::collections::hash_map::Entry::Vacant(entry) = usernames.entry(user_id) {
            let user = users
                .find_by_id(user_id)
                .await?
                .ok_or_else(|| broken_link(&format!("user {user_id}")))?;
            entry.insert(user.username().to_string());
        }
    }

    bets.into_iter()
        .map(|bet| {
            let market = markets_by_id
                .get(&bet.market_id())
                .ok_or_else(|| broken_link(&format!("market {}", bet.market_id())))?;
            let outcome_label = labels_by_id
                .get(&bet.outcome_id())
                .ok_or_else(|| broken_link(&format!("outcome {}", bet.outcome_id())))?
                .clone();
            let username = usernames[&bet.user_id()].clone();
            Ok(BetView {
                market_title: market.title().to_string(),
                outcome_label,
                username,
                bet,
            })
        })
        .collect()
}

/// A bet referencing a missing row is a data-integrity bug (foreign keys
/// forbid it), not a user error.
fn broken_link(what: &str) -> ApplicationError {
    ApplicationError::Internal(format!("bet references missing {what}"))
}

pub(crate) fn view_for(bet: Bet, market: &Market, outcome: &Outcome, username: String) -> BetView {
    BetView {
        market_title: market.title().to_string(),
        outcome_label: outcome.label().to_string(),
        username,
        bet,
    }
}
