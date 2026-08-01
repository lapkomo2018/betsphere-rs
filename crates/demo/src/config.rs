use std::time::Duration;

/// How busy the simulation is and who it runs as. The API's configuration
/// layer fills this from the environment; the defaults are what turning the
/// demo on without further tuning gives — enough movement to watch, slow
/// enough to read.
#[derive(Debug, Clone)]
pub struct DemoConfig {
    /// Number of bot accounts, the market-making one included. Clamped to the
    /// size of the name cast when it is larger.
    pub bots: usize,
    /// Password every bot account is created with, so a developer can log in
    /// as one. Never reused for anything else — these accounts exist only in
    /// demo databases.
    pub bot_password: String,
    /// How often some bot posts, replies, or reacts in a chat room.
    pub chat_every: Duration,
    /// How often some bot stakes on an outcome.
    pub bet_every: Duration,
    /// How often the market maker opens a new market and settles a market
    /// that is past its deadline.
    pub market_every: Duration,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            bots: 12,
            bot_password: "demo-password".to_owned(),
            chat_every: Duration::from_secs(15),
            bet_every: Duration::from_secs(25),
            market_every: Duration::from_secs(300),
        }
    }
}
