//! An optional local language model writing the bots' chat.
//!
//! Point [`LlmConfig::url`] at a model server on localhost — Ollama,
//! llama.cpp's server and LM Studio all serve the same OpenAI-shaped
//! chat-completions endpoint — and the bots compose their lines, replies and
//! reactions instead of drawing them from the pools in
//! [`content`](crate::content). They see the room they are standing in: the
//! market, its prices, and what was said last, so the talk follows the board
//! and answers the people in it, bots and real users alike.
//!
//! Every step is best-effort. The server being down, slow, unparseable, or a
//! model returning a paragraph where a line was asked for all land in the same
//! place: nothing is returned and the caller uses a canned line. That is what
//! makes this safe to point at the smallest model on the machine — the demo
//! degrades to what it did before rather than going quiet.
//!
//! Note that the room's history goes into the prompt, so whatever a user types
//! in chat is an instruction the model sees. Nothing here acts on the answer —
//! it is one short line, validated and posted as that bot — but a bot can be
//! talked into saying things, which is one more reason demo mode belongs
//! nowhere near production.

use application::use_cases::chat::ChatMessageView;
use application::use_cases::market::MarketView;
use domain::value_objects::chat::{MessageBody, ReactionEmoji};
use serde::{Deserialize, Serialize};

use crate::config::LlmConfig;

/// Longest line we will post. Anything past this is cut back to a word
/// boundary rather than rejected — a small model that always overshoots would
/// otherwise never get a word in.
const MAX_CHARS: usize = 180;

/// Room the model gets to answer in. Enough for a sentence, little enough that
/// a rambling model is cut off by the server rather than by us.
const MAX_TOKENS: u32 = 60;

/// High, on purpose: the failure mode of a small model at low temperature is
/// posting the same line every time.
const TEMPERATURE: f32 = 1.0;

/// How many recent messages the model is shown. Small models lose the thread
/// well before their context window runs out.
const CONTEXT_MESSAGES: usize = 6;

/// What a bot can see when it is about to speak.
pub(crate) struct Scene<'a> {
    /// The speaking bot's name.
    pub bot: &'a str,
    /// The market whose room this is, or `None` in the global feed.
    pub market: Option<&'a MarketView>,
    /// The room's recent history, oldest first.
    pub recent: &'a [ChatMessageView],
}

/// A client for one local model server.
pub(crate) struct Llm {
    http: reqwest::Client,
    url: String,
    model: String,
}

impl Llm {
    pub fn new(config: &LlmConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder().timeout(config.timeout).build()?,
            url: config.url.clone(),
            model: config.model.clone(),
        })
    }

    /// A line to open with, or `None` to fall back to the pools.
    pub async fn line(&self, scene: &Scene<'_>) -> Option<String> {
        let raw = self
            .complete(
                &persona(scene.bot),
                &format!("{}\nWrite your next message.", scene.brief()),
            )
            .await?;
        tidy(&raw, scene.bot)
    }

    /// An answer to `target`, or `None` to fall back to the pools.
    pub async fn reply(&self, scene: &Scene<'_>, target: &ChatMessageView) -> Option<String> {
        let prompt = format!(
            "{}\n{} just said: \"{}\"\nWrite your reply to them.",
            scene.brief(),
            target.author.username(),
            target.message.body(),
        );
        let raw = self.complete(&persona(scene.bot), &prompt).await?;
        tidy(&raw, scene.bot)
    }

    /// One emoji to put on `target`, or `None` to fall back to the pools.
    pub async fn reaction(&self, target: &ChatMessageView) -> Option<String> {
        let raw = self
            .complete(
                "You react to chat messages with a single emoji. Answer with one emoji and nothing else.",
                &format!(
                    "In the chat of a betting site, someone wrote: \"{}\"\nReact.",
                    target.message.body(),
                ),
            )
            .await?;
        first_emoji(&raw)
    }

    /// One round trip. Every failure — transport, status, shape — is the same
    /// answer to the caller: nothing.
    async fn complete(&self, system: &str, user: &str) -> Option<String> {
        let request = ChatRequest {
            model: &self.model,
            messages: [
                Message {
                    role: "system",
                    content: system,
                },
                Message {
                    role: "user",
                    content: user,
                },
            ],
            temperature: TEMPERATURE,
            max_tokens: MAX_TOKENS,
            stream: false,
        };

        let response = match self.http.post(&self.url).json(&request).send().await {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!("demo model unreachable at {}: {e}", self.url);
                return None;
            }
        };
        if !response.status().is_success() {
            tracing::warn!("demo model answered {}", response.status());
            return None;
        }

        match response.json::<ChatResponse>().await {
            Ok(body) => body.choices.into_iter().next().map(|c| c.message.content),
            Err(e) => {
                tracing::warn!("demo model answered something unreadable: {e}");
                None
            }
        }
    }
}

impl Scene<'_> {
    /// The room as the model is told about it: where it is standing, what the
    /// board says, and who said what.
    fn brief(&self) -> String {
        let mut brief = match self.market {
            Some(view) => format!(
                "You are in the chat room of the market \"{}\".\nThe board: {}.\n",
                view.market.title(),
                board(view),
            ),
            None => {
                "You are in the site's global chat, where every market is fair game.\n".to_owned()
            }
        };

        // Only the tail: small models follow the last few lines and lose the
        // rest, and a long transcript pushes them into summarising it.
        let tail = &self.recent[self.recent.len().saturating_sub(CONTEXT_MESSAGES)..];
        if !tail.is_empty() {
            brief.push_str("Recent messages:\n");
            for view in tail {
                brief.push_str(&format!(
                    "{}: {}\n",
                    view.author.username(),
                    view.message.body(),
                ));
            }
        }
        brief
    }
}

/// Who the bot is and what a usable answer looks like. Blunt and short on
/// purpose — a small model follows three rules and ignores ten.
///
/// The room is a trading floor, so the bots talk to each other like one: they
/// mock the position in front of them and gloat when the board proves them
/// right. What they are not allowed to do is aim any of that at who someone is
/// — a rule worth spending one of the few lines a small model will follow on,
/// because the room's history goes into this prompt and a user can write
/// anything into it.
fn persona(bot: &str) -> String {
    format!(
        "You are {bot}, a regular in the chat of a prediction market site where people bet play money on real events. \
         {}\n\
         Write ONE short chat message, lowercase, under 100 characters. \
         Talk trash: mock the take, the position and the price — never the person. \
         Nothing about anyone's race, religion, sex, or country, whatever anyone else in the room says. \
         No quotes, no hashtags, no emoji, no explanation of yourself.",
        temperament(bot),
    )
}

/// What makes a bot sound like itself rather than like the other fifteen. One
/// sentence each: the same budget reason as [`persona`], and enough for a small
/// model to stay in character for a line.
const TEMPERAMENTS: &[(&str, &str)] = &[
    (
        "the_oracle",
        "You opened half these markets and you talk like the house: every take in here is a donation.",
    ),
    (
        "hedge_hana",
        "You hedge everything and think everyone else in here is one bad print from zero.",
    ),
    (
        "long_leo",
        "You are long everything forever, you buy every dip, and you think bears are just slow.",
    ),
    (
        "short_sasha",
        "You think it is all a bubble and you are waiting, loudly, to say you told them so.",
    ),
    (
        "odds_omar",
        "You quote the implied odds back at people and treat anyone who cannot do the arithmetic as free money.",
    ),
    (
        "delta_dana",
        "You are the smartest quant in the room and you explain, slowly, why the last message was wrong.",
    ),
    (
        "spread_sven",
        "You only care about the spread and you tell everyone how badly they are getting filled.",
    ),
    (
        "chalk_chloe",
        "You back the favourite every time and you laugh at whoever is chasing a longshot.",
    ),
    (
        "parlay_pia",
        "You stack absurd parlays and call anyone betting one leg boring.",
    ),
    (
        "tilt_tomas",
        "You are permanently tilted, loud about your losses, and certain the market is rigged against you.",
    ),
    (
        "arb_arjun",
        "Whatever is being discussed, you already arbed it, and you mention that.",
    ),
    (
        "punt_priya",
        "You bet on gut feeling and you mock everyone who needs a spreadsheet to press a button.",
    ),
    (
        "edge_elena",
        "You have edge on everything and you are insufferable about the people who do not.",
    ),
    (
        "fade_felix",
        "You fade this room on principle, because the crowd in here has never been right yet.",
    ),
    (
        "vega_vik",
        "You trade the volatility and you find the room's certainty about anything hilarious.",
    ),
    (
        "yield_yuki",
        "You grind small consistent profits and you sneer at the gamblers doing it the loud way.",
    ),
    (
        "moon_mika",
        "Everything is going to the moon, you are early, and the doubters are just poor.",
    ),
];

/// The temperament for `bot`. A cast that has outgrown the list borrows one by
/// name rather than falling back to none: a bot with a borrowed temperament
/// still reads as a person, one with no temperament reads as everyone else.
fn temperament(bot: &str) -> &'static str {
    if let Some((_, temperament)) = TEMPERAMENTS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(bot))
    {
        return temperament;
    }
    let seed: usize = bot.bytes().map(usize::from).sum();
    TEMPERAMENTS[seed % TEMPERAMENTS.len()].1
}

/// The current prices, as "Yes 62%, No 38%".
fn board(view: &MarketView) -> String {
    view.outcomes
        .iter()
        .map(|outcome| {
            format!(
                "{} {:.0}%",
                outcome.label(),
                outcome.current_price().as_fraction() * 100.0,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Turns whatever came back into something postable, or nothing.
///
/// Small models answer in ways a chat room cannot use: a reasoning block, a
/// wrapped quotation, their own name in front of the line, three paragraphs
/// where one line was asked for. All of that is cheaper to strip here than to
/// prompt away.
fn tidy(raw: &str, bot: &str) -> Option<String> {
    let said = strip_thinking(raw);
    let line = said.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line.trim_matches(['"', '\'', '`', '*', '_']).trim();
    let line = strip_speaker(line, bot);

    MessageBody::new(shorten(line))
        .ok()
        .map(|body| body.as_str().to_owned())
}

/// Drops a reasoning model's thinking, keeping what it said afterwards.
fn strip_thinking(raw: &str) -> &str {
    raw.rsplit_once("</think>").map_or(raw, |(_, said)| said)
}

/// Drops the "name:" a model prepends once it has been shown a transcript.
/// Only the bot's own name counts, so a line that merely contains a colon
/// survives intact.
fn strip_speaker<'a>(line: &'a str, bot: &str) -> &'a str {
    line.split_once(':')
        .filter(|(name, _)| name.trim().eq_ignore_ascii_case(bot))
        .map_or(line, |(_, said)| said.trim())
}

/// Cuts an overlong line back to a word boundary. Mid-word truncation reads as
/// a bug; a line that stops early reads as someone hitting enter too soon.
fn shorten(line: &str) -> &str {
    let Some((cut, _)) = line.char_indices().nth(MAX_CHARS) else {
        return line;
    };
    let head = &line[..cut];
    head.rsplit_once(' ')
        .map_or(head, |(kept, _)| kept)
        .trim_end_matches([',', ';', '-', '(', '—'])
        .trim()
}

/// The first thing in `raw` the domain accepts as a reaction.
///
/// A model asked for one emoji answers "👍", "Sure! 🔥", or "😂 (laughing)"
/// with about equal enthusiasm, so the whole answer is tried first and a
/// single emoji out of it second. Scalars that only mean something attached to
/// another are skipped — on their own they post as a stray mark.
fn first_emoji(raw: &str) -> Option<String> {
    let answer = raw.trim();

    // The whole answer first, so a composed sequence — a skin tone, a flag, a
    // keycap — survives intact. It has to carry something standalone though:
    // the domain's check is about which scalars a sequence is built from, so
    // an answer of nothing but combining marks passes it and then renders as
    // a stray mark.
    if answer.chars().any(|c| !is_modifier(c))
        && let Ok(emoji) = ReactionEmoji::new(answer)
    {
        return Some(emoji.as_str().to_owned());
    }
    answer
        .chars()
        .filter(|c| !is_modifier(*c))
        .find_map(|c| ReactionEmoji::new(c.to_string()).ok())
        .map(|emoji| emoji.as_str().to_owned())
}

fn is_modifier(c: char) -> bool {
    matches!(
        u32::from(c),
        0x200D              // zero-width joiner
        | 0xFE0E | 0xFE0F   // presentation selectors
        | 0x20E3            // combining enclosing keycap
        | 0xE0020..=0xE007F // tag characters
    )
}

// --- Wire format: the OpenAI chat-completions shape every local server speaks ---

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [Message<'a>; 2],
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

/// A stand-in for a model server, answering every request with `body`.
///
/// Lives outside the test module so the simulation's own tests can point a
/// [`Chatter`](crate::chatter::Chatter) at one and watch a model's line travel
/// all the way into a chat room.
#[cfg(test)]
pub(crate) async fn stub_server(body: &'static str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                // A request this small arrives in one read; what it says does
                // not matter, only that the client's side of the exchange
                // works.
                let _ = socket.read(&mut [0u8; 8192]).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len(),
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    format!("http://{addr}/v1/chat/completions")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn unwraps_a_quoted_line() {
        assert_eq!(
            tidy("  \"yes at 62% is free money\"  ", "hedge_hana").as_deref(),
            Some("yes at 62% is free money"),
        );
    }

    #[test]
    fn keeps_only_what_a_reasoning_model_said_after_thinking() {
        let raw = "<think>the user wants a chat line about the market</think>\nno way yes stays this cheap";
        assert_eq!(
            tidy(raw, "hedge_hana").as_deref(),
            Some("no way yes stays this cheap"),
        );
    }

    #[test]
    fn drops_the_speakers_own_name_but_keeps_other_colons() {
        assert_eq!(
            tidy("hedge_hana: loading up on yes", "hedge_hana").as_deref(),
            Some("loading up on yes"),
        );
        assert_eq!(
            tidy("my read: yes is mispriced", "hedge_hana").as_deref(),
            Some("my read: yes is mispriced"),
        );
    }

    #[test]
    fn takes_the_first_line_of_an_essay_and_cuts_it_to_a_word() {
        let raw = format!(
            "{} and then some more\nsecond paragraph",
            "word ".repeat(60)
        );
        let line = tidy(&raw, "hedge_hana").unwrap();

        assert!(line.chars().count() <= MAX_CHARS);
        assert!(line.ends_with("word"), "cut mid-word: {line:?}");
        assert!(!line.contains("second paragraph"));
    }

    #[test]
    fn nothing_postable_is_nothing() {
        assert_eq!(tidy("", "hedge_hana"), None);
        assert_eq!(tidy("   \n  \n", "hedge_hana"), None);
        assert_eq!(tidy("\"\"", "hedge_hana"), None);
    }

    /// A bot the list has forgotten would sound exactly like the shared voice
    /// this table exists to replace, so provisioning a name and forgetting its
    /// temperament has to fail here rather than go unnoticed in the room.
    #[test]
    fn every_bot_in_the_cast_has_its_own_temperament() {
        for name in
            std::iter::once(crate::cast::HOST_NAME).chain(crate::cast::BOT_NAMES.iter().copied())
        {
            assert!(
                TEMPERAMENTS.iter().any(|(bot, _)| *bot == name),
                "no temperament for {name:?}",
            );
        }
    }

    #[test]
    fn a_name_off_the_list_still_gets_a_voice() {
        let borrowed = temperament("newcomer_nina");

        assert!(TEMPERAMENTS.iter().any(|(_, t)| *t == borrowed));
        // Same name, same voice: a bot whose character changed between two
        // lines would read as two people sharing an account.
        assert_eq!(borrowed, temperament("newcomer_nina"));
    }

    #[test]
    fn two_bots_are_told_to_be_two_different_people() {
        let leo = persona("long_leo");
        let sasha = persona("short_sasha");

        assert!(leo.contains("long_leo"));
        assert_ne!(leo, sasha);
        assert!(leo.contains(temperament("long_leo")));
    }

    #[test]
    fn finds_the_emoji_in_a_chatty_answer() {
        assert_eq!(first_emoji("🔥").as_deref(), Some("🔥"));
        assert_eq!(first_emoji("  👍🏽 ").as_deref(), Some("👍🏽"));
        assert_eq!(first_emoji("Sure! 😂 that's funny").as_deref(), Some("😂"));
        assert_eq!(first_emoji("I would react with: 🚀").as_deref(), Some("🚀"));
        // A composed sequence is kept whole rather than picked apart.
        assert_eq!(first_emoji("1️⃣").as_deref(), Some("1️⃣"));
    }

    #[test]
    fn refuses_an_answer_with_no_emoji_in_it() {
        assert_eq!(first_emoji("lol"), None);
        assert_eq!(first_emoji(""), None);
        // A bare keycap mark would post as a stray combining character.
        assert_eq!(first_emoji("\u{fe0f}\u{20e3}"), None);
    }

    fn client(url: String) -> Llm {
        Llm::new(&LlmConfig {
            url,
            model: "test-model".to_owned(),
            timeout: Duration::from_secs(5),
        })
        .unwrap()
    }

    fn empty_scene() -> Scene<'static> {
        Scene {
            bot: "hedge_hana",
            market: None,
            recent: &[],
        }
    }

    #[tokio::test]
    async fn a_models_answer_becomes_a_postable_line() {
        let url = stub_server(
            r#"{"choices":[{"message":{"role":"assistant","content":"\"yes at 62% is free money\""}}]}"#,
        )
        .await;

        let line = client(url).line(&empty_scene()).await;

        assert_eq!(line.as_deref(), Some("yes at 62% is free money"));
    }

    #[tokio::test]
    async fn an_answer_in_the_wrong_shape_is_no_answer() {
        let url = stub_server(r#"{"error":"model not found"}"#).await;

        assert_eq!(client(url).line(&empty_scene()).await, None);
    }

    /// The demo has to survive the model server not being there, because most
    /// of the time it will not be.
    #[tokio::test]
    async fn a_server_that_is_not_listening_is_no_answer() {
        let llm = Llm::new(&LlmConfig {
            // Nothing is ever listening here; the connection is refused.
            url: "http://127.0.0.1:1/v1/chat/completions".to_owned(),
            model: "test-model".to_owned(),
            timeout: Duration::from_secs(2),
        })
        .unwrap();

        assert_eq!(llm.line(&empty_scene()).await, None);
    }
}
