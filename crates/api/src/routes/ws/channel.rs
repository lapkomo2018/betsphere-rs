//! Wire channel names and the streams they name.
//!
//! Parsing lives apart from the connection loop because it is the one piece of
//! the endpoint that is pure: a name in, a stream identity out, no socket and
//! no broker involved.

use application::realtime::BetFeed;
use domain::entities::{ChatChannel, MarketId};
use uuid::Uuid;

/// Wire name of the global chat room.
const GLOBAL_CHAT: &str = "global_chat";

/// Wire name of the cross-market bet feed.
const GLOBAL_BETS: &str = "global_bets";

/// Wire-name prefix of a market's chat room: `market_chat:<market uuid>`.
const MARKET_CHAT_PREFIX: &str = "market_chat:";

/// Wire-name prefix of a market's bet feed: `market_bets:<market uuid>`.
const MARKET_BETS_PREFIX: &str = "market_bets:";

/// Wire-name prefix of a market's live feed: `market:<market uuid>`.
const MARKET_FEED_PREFIX: &str = "market:";

/// What a client's frame says when its channel name doesn't parse.
pub(super) const HINT: &str = "expected {\"type\": \"subscribe|unsubscribe|chat_message\", \"channel\": \
     \"global_chat\" | \"market_chat:<id>\" | \"market:<id>\" | \
     \"market_bets:<id>\" | \"global_bets\", ...}";

/// A stream a client can subscribe to, parsed from its wire name.
#[derive(Debug, Clone, Copy)]
pub(super) enum Channel {
    /// A chat room; carries `chat_message` frames both ways.
    Chat(ChatChannel),
    /// A bet feed, global or scoped to one market; server-to-client only
    /// (`bet_placed` frames).
    Bets(BetFeed),
    /// A market's live feed; server-to-client only (`price_update` frames).
    MarketFeed(MarketId),
}

/// Parses a wire channel name, or `None` if it names no stream.
///
/// `market_bets:` is matched before `market:` on purpose — the latter is a
/// prefix of nothing else here, but keeping the two bet feeds adjacent is
/// what makes the ordering obvious rather than incidental.
pub(super) fn parse(name: &str) -> Option<Channel> {
    if name == GLOBAL_CHAT {
        return Some(Channel::Chat(ChatChannel::Global));
    }
    if name == GLOBAL_BETS {
        return Some(Channel::Bets(BetFeed::Global));
    }
    if let Some(id) = name.strip_prefix(MARKET_CHAT_PREFIX) {
        let id = Uuid::parse_str(id).ok()?;
        return Some(Channel::Chat(ChatChannel::Market(id.into())));
    }
    if let Some(id) = name.strip_prefix(MARKET_BETS_PREFIX) {
        let id = Uuid::parse_str(id).ok()?;
        return Some(Channel::Bets(BetFeed::Market(id.into())));
    }
    let id = name.strip_prefix(MARKET_FEED_PREFIX)?;
    let id = Uuid::parse_str(id).ok()?;
    Some(Channel::MarketFeed(id.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_global_channels() {
        assert!(matches!(
            parse("global_chat"),
            Some(Channel::Chat(ChatChannel::Global))
        ));
        assert!(matches!(
            parse("global_bets"),
            Some(Channel::Bets(BetFeed::Global))
        ));
    }

    #[test]
    fn parses_the_market_scoped_channels() {
        let id = Uuid::new_v4();
        assert!(matches!(
            parse(&format!("market_chat:{id}")),
            Some(Channel::Chat(ChatChannel::Market(m))) if m.as_uuid() == id
        ));
        assert!(matches!(
            parse(&format!("market_bets:{id}")),
            Some(Channel::Bets(BetFeed::Market(m))) if m.as_uuid() == id
        ));
        assert!(matches!(
            parse(&format!("market:{id}")),
            Some(Channel::MarketFeed(m)) if m.as_uuid() == id
        ));
    }

    /// `market_bets:<id>` also starts with neither `market:` nor `market_chat:`,
    /// but the shared `market` stem makes it worth pinning down.
    #[test]
    fn market_prefixes_do_not_shadow_each_other() {
        let id = Uuid::new_v4();
        assert!(matches!(
            parse(&format!("market_bets:{id}")),
            Some(Channel::Bets(_))
        ));
        assert!(matches!(
            parse(&format!("market:{id}")),
            Some(Channel::MarketFeed(_))
        ));
    }

    #[test]
    fn rejects_unknown_names_and_malformed_ids() {
        assert!(parse("").is_none());
        assert!(parse("global_markets").is_none());
        assert!(parse("market:not-a-uuid").is_none());
        assert!(parse("market_bets:").is_none());
        // A trailing colon is required: the bare prefix names nothing.
        assert!(parse("market").is_none());
    }
}
