//! The bot accounts the simulation acts as.
//!
//! Bots are ordinary users: real rows, real balances, real bets. They are
//! identified by an email under [`EMAIL_DOMAIN`], which is what makes
//! provisioning idempotent — a restart finds the accounts a previous run
//! created and picks up where it left off instead of flooding the database
//! with new ones.

use application::ApplicationError;
use application::actor::Actor;
use application::ports::PasswordHasher;
use domain::entities::{Role, User};
use domain::repositories::UserRepository;
use domain::value_objects::user::{Email, Password, PasswordHash, Username};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;

use crate::config::DemoConfig;

/// Host part of every bot's email. Not a routable domain: nothing should ever
/// try to mail these accounts, and a real user cannot register under it.
pub(crate) const EMAIL_DOMAIN: &str = "bots.betsphere.invalid";

/// The market maker. The only bot with the admin role, because opening and
/// resolving markets is an admin action.
const HOST_NAME: &str = "the_oracle";

/// Everyone else, in the order they are provisioned.
const BOT_NAMES: &[&str] = &[
    "hedge_hana",
    "long_leo",
    "short_sasha",
    "odds_omar",
    "delta_dana",
    "spread_sven",
    "chalk_chloe",
    "parlay_pia",
    "tilt_tomas",
    "arb_arjun",
    "punt_priya",
    "edge_elena",
    "fade_felix",
    "vega_vik",
    "yield_yuki",
    "moon_mika",
];

/// One bot account: who its actions are attributed to.
#[derive(Debug, Clone)]
pub(crate) struct Bot {
    pub name: &'static str,
    pub actor: Actor,
}

/// The provisioned cast.
pub(crate) struct Cast {
    host: Bot,
    /// Everyone, the host included — it bets and chats like the rest.
    bots: Vec<Bot>,
}

impl Cast {
    /// The admin bot, which opens and resolves markets.
    pub fn host(&self) -> &Bot {
        &self.host
    }

    /// Someone to act next. Never empty: the host alone is a valid cast.
    pub fn anyone(&self, rng: &mut StdRng) -> &Bot {
        self.bots.choose(rng).unwrap_or(&self.host)
    }

    pub fn len(&self) -> usize {
        self.bots.len()
    }
}

/// Finds or creates the bot accounts and returns them ready to act.
///
/// Every bot is created with the same password hash: one Argon2 run rather
/// than one per bot, and the salt inside the hash is shared, which is exactly
/// as safe as sharing the password itself — which these accounts do by design.
pub(crate) async fn provision(
    users: &dyn UserRepository,
    hasher: &dyn PasswordHasher,
    config: &DemoConfig,
) -> Result<Cast, ApplicationError> {
    let password = Password::new(config.bot_password.clone())?;
    let hash = hasher.hash(&password)?;

    let host = account(users, HOST_NAME, Role::Admin, &hash).await?;
    let mut bots = vec![host.clone()];

    // `bots` counts the host, and the cast can't outgrow its name list.
    let extras = config.bots.saturating_sub(1).min(BOT_NAMES.len());
    for name in &BOT_NAMES[..extras] {
        bots.push(account(users, name, Role::User, &hash).await?);
    }

    Ok(Cast { host, bots })
}

/// Loads the demo account called `name`, creating it if this is a fresh
/// database. An existing one keeps its balance, bets and messages — the demo
/// resumes rather than restarts — but has its role corrected, so raising
/// `DEMO_BOTS` or renaming the host can never leave the cast unable to open a
/// market.
async fn account(
    users: &dyn UserRepository,
    name: &'static str,
    role: Role,
    hash: &PasswordHash,
) -> Result<Bot, ApplicationError> {
    let email = Email::new(format!("{name}@{EMAIL_DOMAIN}"))?;

    let user = match users.find_by_email(&email).await? {
        Some(mut user) => {
            if user.role() != role {
                user.set_role(role);
                users.save(&user).await?;
            }
            user
        }
        None => {
            let mut user = User::new(Username::new(name)?, email, hash.clone());
            user.set_role(role);
            users.save(&user).await?;
            tracing::info!(bot = name, %role, "created demo account");
            user
        }
    };

    Ok(Bot {
        name,
        actor: Actor {
            user_id: user.id(),
            role,
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn names() -> impl Iterator<Item = &'static str> {
        std::iter::once(HOST_NAME).chain(BOT_NAMES.iter().copied())
    }

    /// Provisioning builds a username and an email out of each name, and both
    /// are validated. A name the domain rejects would take the whole
    /// simulation down at startup.
    #[test]
    fn every_name_is_a_valid_identity() {
        for name in names() {
            assert!(Username::new(name).is_ok(), "bad username: {name:?}");
            assert!(
                Email::new(format!("{name}@{EMAIL_DOMAIN}")).is_ok(),
                "bad email for {name:?}",
            );
        }
    }

    /// Names are the accounts' identity: a duplicate would have the second bot
    /// silently share the first one's balance and bets.
    #[test]
    fn names_are_distinct() {
        let unique: HashSet<&str> = names().collect();
        assert_eq!(unique.len(), names().count());
    }
}
