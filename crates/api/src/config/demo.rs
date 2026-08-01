use std::time::Duration;

use demo::DemoConfig;

use super::env::{optional, parse_or};
use super::error::ConfigError;

/// Whether to run the demo activity simulation, and how busy it should be.
///
/// Off unless `DEMO_MODE` says otherwise: the bots write markets, bets and
/// messages into whatever database the server is pointed at, and one of them
/// holds the admin role.
#[derive(Debug, Clone)]
pub struct DemoModeConfig {
    pub enabled: bool,
    /// Knobs handed to the simulation; only read when it is enabled.
    pub simulation: DemoConfig,
}

impl DemoModeConfig {
    pub(super) fn from_env() -> Result<Self, ConfigError> {
        let defaults = DemoConfig::default();
        Ok(Self {
            enabled: parse_or("DEMO_MODE", false)?,
            simulation: DemoConfig {
                bots: parse_or("DEMO_BOTS", defaults.bots)?,
                bot_password: optional("DEMO_BOT_PASSWORD").unwrap_or(defaults.bot_password),
                chat_every: secs("DEMO_CHAT_INTERVAL_SECS", defaults.chat_every)?,
                bet_every: secs("DEMO_BET_INTERVAL_SECS", defaults.bet_every)?,
                market_every: secs("DEMO_MARKET_INTERVAL_SECS", defaults.market_every)?,
            },
        })
    }
}

/// Reads an interval given in whole seconds, falling back to the simulation's
/// own default.
fn secs(key: &'static str, default: Duration) -> Result<Duration, ConfigError> {
    Ok(Duration::from_secs(parse_or(key, default.as_secs())?))
}
