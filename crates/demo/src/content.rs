//! What the bots trade on and talk about.
//!
//! Everything here is fiction written for a demo database. Market templates
//! are picked one at a time and only while their title is not already live, so
//! the board fills up with distinct questions rather than a wall of the same
//! one; chat lines are drawn at random and, in a market room, filled in with
//! that market's own outcomes so the talk tracks the board.

/// A market the maker bot can open: the question, how it can resolve, and how
/// long it stays open for.
pub(crate) struct MarketTemplate {
    pub title: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    /// At least two, in the order they appear on the board.
    pub outcomes: &'static [&'static str],
    /// Hours from creation to the market's deadline. Short-lived markets keep
    /// the demo resolving things while someone is watching; the longer ones
    /// give the price charts room to move.
    pub open_for_hours: i64,
}

pub(crate) const MARKET_TEMPLATES: &[MarketTemplate] = &[
    MarketTemplate {
        title: "Will Bitcoin close above $150,000 this year?",
        description: "Resolves on the closing price of the last trading day of the year.",
        category: "crypto",
        outcomes: &["Yes", "No"],
        open_for_hours: 720,
    },
    MarketTemplate {
        title: "Which team lifts the Champions League trophy?",
        description: "Resolves to the winner of the final. Anyone not listed resolves to Field.",
        category: "sports",
        outcomes: &["Real Madrid", "Manchester City", "Bayern Munich", "Field"],
        open_for_hours: 480,
    },
    MarketTemplate {
        title: "What does the central bank do at the next meeting?",
        description: "Resolves on the published rate decision.",
        category: "economics",
        outcomes: &["Cut", "Hold", "Hike"],
        open_for_hours: 336,
    },
    MarketTemplate {
        title: "Will a crewed mission leave for Mars before 2030?",
        description: "Resolves Yes on a crewed launch on a Mars trajectory before Jan 1, 2030.",
        category: "space",
        outcomes: &["Yes", "No"],
        open_for_hours: 2160,
    },
    MarketTemplate {
        title: "How many goals in Sunday's derby?",
        description: "Combined goals, extra time excluded.",
        category: "sports",
        outcomes: &["0-1", "2-3", "4 or more"],
        open_for_hours: 20,
    },
    MarketTemplate {
        title: "Will it snow in Kyiv on New Year's Eve?",
        description: "Resolves on the official observation for Dec 31.",
        category: "weather",
        outcomes: &["Yes", "No"],
        open_for_hours: 168,
    },
    MarketTemplate {
        title: "Will the sequel open above $200M domestically?",
        description: "Resolves on reported opening-weekend domestic box office.",
        category: "movies",
        outcomes: &["Yes", "No"],
        open_for_hours: 240,
    },
    MarketTemplate {
        title: "Which party forms the next coalition?",
        description: "Resolves to the party leading the government after talks conclude.",
        category: "politics",
        outcomes: &[
            "Left bloc",
            "Right bloc",
            "Grand coalition",
            "Snap election",
        ],
        open_for_hours: 600,
    },
    MarketTemplate {
        title: "Will ETH flip BTC in market cap this cycle?",
        description: "Resolves Yes if ETH market cap exceeds BTC's for a full trading day.",
        category: "crypto",
        outcomes: &["Yes", "No"],
        open_for_hours: 1440,
    },
    MarketTemplate {
        title: "Which console sells the most units this quarter?",
        description: "Resolves on reported quarterly hardware sales.",
        category: "gaming",
        outcomes: &["Switch", "PlayStation", "Xbox"],
        open_for_hours: 900,
    },
    MarketTemplate {
        title: "Will remote roles pass 30% of new postings by year end?",
        description: "Resolves on the December share of postings advertised as remote.",
        category: "work",
        outcomes: &["Yes", "No"],
        open_for_hours: 1000,
    },
    MarketTemplate {
        title: "Will this feed pass 10,000 bets this month?",
        description: "Settles on the platform's own counter. Yes, we bet on ourselves.",
        category: "meta",
        outcomes: &["Yes", "No"],
        open_for_hours: 72,
    },
    MarketTemplate {
        title: "Coffee or tea: what does the office run out of first?",
        description: "Deadly serious. Resolves on the empty tin.",
        category: "meta",
        outcomes: &["Coffee", "Tea", "Neither, someone restocked"],
        open_for_hours: 48,
    },
];

/// Small talk for the global room, where there is no market to react to.
pub(crate) const GLOBAL_LINES: &[&str] = &[
    "gm ☕",
    "this feed moves fast today",
    "anyone else up three markets in a row?",
    "my portfolio is 90% vibes at this point",
    "just topped up, feeling reckless",
    "who's fading the favourite today?",
    "new here — what's everyone trading?",
    "liquidity looks healthy this morning",
    "screenshot this, I called it first",
    "brb making coffee, nobody move the odds",
    "someone explain why I keep buying the top",
    "back to break even, I'll take it",
    "the charts on the new markets are wild",
    "I only bet on things I can't influence, keeps it honest",
    "reading the board like tea leaves",
    "quiet in here, someone open a market",
];

/// Lines for a market room. `{outcome}` is filled in with one of that
/// market's outcomes, `{price}` with its current price, `{title}` with the
/// question itself.
pub(crate) const MARKET_LINES: &[&str] = &[
    "{outcome} at {price} is a steal",
    "no way {outcome} stays this cheap",
    "loaded up on {outcome}, see you at resolution",
    "{outcome} is overpriced, I'm fading it",
    "who keeps pumping {outcome}?",
    "this resolves {outcome}, mark it",
    "{price} on {outcome}? free money",
    "I'd take the other side of {outcome} all day",
    "waiting for {outcome} to dip before I add",
    "the whole board is mispricing {outcome}",
    "still can't believe {outcome} is the favourite",
    "\"{title}\" is the only question that matters today",
    "in on {outcome}, small size, big conviction",
    "moved my stake off {outcome}, the news changed",
    "{outcome} at {price} and nobody's talking about it",
];

/// Replies, which quote whatever was said last rather than starting a thought.
pub(crate) const REPLY_LINES: &[&str] = &[
    "this",
    "^ exactly my read",
    "bold",
    "you'll regret that 😄",
    "hard disagree",
    "screenshotting this for later",
    "sir, this is a prediction market",
    "let's see how that ages",
    "finally someone said it",
    "the odds disagree with you",
    "same trade, different reasoning",
    "respectfully, no",
];

/// Reactions the bots hand out. Kept to ones that render everywhere.
pub(crate) const REACTIONS: &[&str] = &[
    "🔥", "👍", "😂", "🚀", "💀", "🤔", "💰", "👀", "🎯", "😭", "🙌", "📈", "📉", "🧠", "🍿",
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use domain::value_objects::chat::{MessageBody, ReactionEmoji};
    use domain::value_objects::market::{MarketTitle, OutcomeLabel};

    use super::*;

    /// The bots' content is validated by the same value objects a user's input
    /// is, so a typo here fails the build rather than a background task at
    /// three in the morning.
    #[test]
    fn every_template_describes_a_market_the_domain_accepts() {
        for template in MARKET_TEMPLATES {
            assert!(
                MarketTitle::new(template.title).is_ok(),
                "bad title: {:?}",
                template.title
            );
            assert!(
                template.outcomes.len() >= 2,
                "{:?} needs at least two outcomes",
                template.title
            );
            for label in template.outcomes {
                assert!(OutcomeLabel::new(*label).is_ok(), "bad label: {label:?}");
            }
            assert!(
                template.open_for_hours > 0,
                "{:?} would close before it opened",
                template.title
            );
        }
    }

    /// The maker keeps a question off the board while a market with that title
    /// is live, so two templates sharing one would lock each other out.
    #[test]
    fn template_titles_are_distinct() {
        let titles: HashSet<&str> = MARKET_TEMPLATES.iter().map(|t| t.title).collect();
        assert_eq!(titles.len(), MARKET_TEMPLATES.len());
    }

    #[test]
    fn every_line_is_a_postable_message() {
        for line in GLOBAL_LINES
            .iter()
            .chain(MARKET_LINES)
            .chain(REPLY_LINES)
            .copied()
        {
            assert!(MessageBody::new(line).is_ok(), "rejected {line:?}");
        }
    }

    /// A market line with nothing to fill in is just small talk in the wrong
    /// pool — it would never mention the market it was posted in.
    #[test]
    fn market_lines_carry_a_placeholder() {
        for line in MARKET_LINES {
            assert!(
                ["{outcome}", "{price}", "{title}"]
                    .iter()
                    .any(|slot| line.contains(slot)),
                "no placeholder in {line:?}",
            );
        }
    }

    #[test]
    fn every_reaction_is_a_valid_emoji() {
        for emoji in REACTIONS {
            assert!(ReactionEmoji::new(*emoji).is_ok(), "rejected {emoji:?}");
        }
    }
}
