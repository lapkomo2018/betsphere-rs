mod list_recent;
mod post_message;
mod react;

pub use list_recent::{HistoryWindow, ListRecentMessages};
pub use post_message::PostMessage;
pub use react::ReactToMessage;

use domain::entities::{ChatMessage, User};
use domain::repositories::ReactionTally;

/// A chat message paired with everything needed to render it. The author is
/// resolved at read time so display names and avatars stay current rather than
/// being snapshotted at send time; so are the quote and the reactions, which
/// are mutable state of their own.
pub struct ChatMessageView {
    pub message: ChatMessage,
    pub author: User,
    /// The message this one quotes, or `None` when it is not a reply — or when
    /// the quoted message is no longer readable, which reads to a client as an
    /// ordinary message rather than as a reply to a hole.
    pub reply_to: Option<RepliedMessage>,
    /// The reactions on this message, tallied for whoever is reading it.
    pub reactions: Vec<ReactionTally>,
}

/// The message quoted above a reply. Deliberately not a [`ChatMessageView`]:
/// a quote is rendered one level deep, so the parent's own quote and reactions
/// would be dead weight — and, down a chain of replies, unbounded.
pub struct RepliedMessage {
    pub message: ChatMessage,
    pub author: User,
}
