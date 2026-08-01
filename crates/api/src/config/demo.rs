use std::time::Duration;

use demo::{DemoConfig, LlmConfig};

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
                llm: local_model()?,
            },
        })
    }
}

/// The local model the bots write their chat with, configured by pointing
/// `DEMO_LLM_URL` at a running model server. Unset — the default — leaves them
/// drawing lines from their canned pools.
fn local_model() -> Result<Option<LlmConfig>, ConfigError> {
    let Some(url) = optional("DEMO_LLM_URL") else {
        return Ok(None);
    };

    // The HTTP client is built without TLS: this talks to a model server on
    // the same machine, and an `https://` URL would fail per request at
    // runtime rather than here, where it can be explained.
    if !url.starts_with("http://") {
        return Err(ConfigError::Invalid {
            key: "DEMO_LLM_URL",
            reason: "must be an http:// URL — demo mode talks to a local model server".to_owned(),
        });
    }

    Ok(Some(LlmConfig {
        url,
        model: optional("DEMO_LLM_MODEL").unwrap_or_else(|| LlmConfig::DEFAULT_MODEL.to_owned()),
        timeout: secs("DEMO_LLM_TIMEOUT_SECS", LlmConfig::DEFAULT_TIMEOUT)?,
    }))
}

/// Reads an interval given in whole seconds, falling back to the simulation's
/// own default.
fn secs(key: &'static str, default: Duration) -> Result<Duration, ConfigError> {
    Ok(Duration::from_secs(parse_or(key, default.as_secs())?))
}
