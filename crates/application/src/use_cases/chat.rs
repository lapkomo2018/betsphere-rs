mod list_recent;
mod post_message;

pub use list_recent::ListRecentMessages;
pub use post_message::PostMessage;

use domain::entities::{ChatMessage, User};

/// A chat message paired with its author's profile, ready for presentation.
/// The author is resolved at read time so display names and avatars stay
/// current rather than being snapshotted at send time.
pub struct ChatMessageView {
    pub message: ChatMessage,
    pub author: User,
}
