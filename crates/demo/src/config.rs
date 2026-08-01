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
    /// A local model to write the chat with. `None` — the default — draws
    /// every line from the pools in [`content`](crate::content) instead.
    pub llm: Option<LlmConfig>,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            bots: 12,
            bot_password: "demo-password".to_owned(),
            chat_every: Duration::from_secs(15),
            bet_every: Duration::from_secs(25),
            market_every: Duration::from_secs(300),
            llm: None,
        }
    }
}

/// A local model server the bots write their chat with.
///
/// Any server speaking the OpenAI chat-completions shape will do — Ollama,
/// llama.cpp's server, LM Studio — and the smallest model on the machine is
/// the point rather than a compromise: the lines are one sentence of small
/// talk, and anything the model gets wrong falls back to a canned line.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Full chat-completions endpoint, e.g.
    /// `http://localhost:11434/v1/chat/completions`.
    pub url: String,
    /// Model name as that server knows it.
    pub model: String,
    /// How long a bot waits for a line before giving up on it. A demo does not
    /// stall for a model: the turn simply falls back to the canned pools.
    pub timeout: Duration,
}

impl LlmConfig {
    /// Small, quick on a CPU, and coherent enough for one line of chat.
    pub const DEFAULT_MODEL: &'static str = "qwen2.5:0.5b";

    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
}
