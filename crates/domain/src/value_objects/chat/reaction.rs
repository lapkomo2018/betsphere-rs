use crate::DomainError;

/// A validated reaction emoji, stored exactly as written.
///
/// Emoji are *sequences*, not single characters — a flag is two scalars, a
/// family four joined by zero-width joiners, a keycap an ASCII digit followed
/// by two combining scalars — so validation is about which scalars a sequence
/// may be built from rather than about length alone: pictographs and symbols,
/// the keycap bases `0`-`9`, `#` and `*`, and the joiners that glue them
/// together. At least one of those scalars must be pictographic, which is what
/// keeps a reaction from being arbitrary text.
///
/// The check is deliberately a shape check, not a lookup against the Unicode
/// emoji tables: it costs no dependency and no data to maintain, and the worst
/// a caller gets away with is an unusual-but-harmless sequence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReactionEmoji(String);

impl ReactionEmoji {
    /// Longest sequence we accept, in Unicode scalar values. The longest ones
    /// in daily use are subdivision flags (8 scalars) and skin-toned family
    /// emoji (11); the rest is headroom.
    pub const MAX_SCALARS: usize = 16;

    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let invalid = || DomainError::Validation("reaction must be a single emoji".to_owned());

        if value.is_empty() {
            return Err(invalid());
        }
        if value.chars().count() > Self::MAX_SCALARS {
            return Err(DomainError::Validation(format!(
                "reaction must be at most {} characters",
                Self::MAX_SCALARS
            )));
        }
        if !value.chars().all(is_sequence_scalar) || !value.chars().any(is_emoji_scalar) {
            return Err(invalid());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReactionEmoji {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `c` is pictographic — a scalar that renders as an emoji on its own,
/// or the enclosing keycap that turns a digit into one. A sequence needs at
/// least one of these to be an emoji rather than punctuation or a bare digit.
fn is_emoji_scalar(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00A9 | 0x00AE                     // © ®
        | 0x203C | 0x2049                   // ‼ ⁉
        | 0x20E3                            // combining enclosing keycap
        | 0x2122 | 0x2139                   // ™ ℹ
        | 0x2194..=0x21AA                   // arrows
        | 0x231A..=0x231B | 0x2328 | 0x23CF // ⌚ ⌛ ⌨ ⏏
        | 0x23E9..=0x23FA                   // media controls, clocks
        | 0x24C2 | 0x25AA..=0x25FE          // Ⓜ, geometric shapes
        | 0x2600..=0x27BF                   // misc symbols and dingbats
        | 0x2934..=0x2935 | 0x2B00..=0x2BFF // arrows, misc symbols and arrows
        | 0x3030 | 0x303D | 0x3297 | 0x3299 // 〰 〽 ㊗ ㊙
        | 0x1F000..=0x1FAFF                 // the emoji planes, skin tones included
    )
}

/// Whether `c` may appear anywhere in a sequence: a pictograph, one of the
/// joiners that combine them, or a keycap base.
fn is_sequence_scalar(c: char) -> bool {
    is_emoji_scalar(c)
        || matches!(
            u32::from(c),
            0x200D                  // zero-width joiner
            | 0xFE0E | 0xFE0F       // text / emoji presentation selectors
            | 0xE0020..=0xE007F // tag characters, for subdivision flags
        )
        || c.is_ascii_digit()
        || matches!(c, '#' | '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_and_composed_emoji() {
        for value in [
            "🔥",
            "👍",
            "❤️", // pictograph + presentation selector
            "👍🏽", // skin tone modifier
            "👨‍👩‍👧‍👦", // ZWJ family
            "🇺🇦", // regional indicator pair
            "🏴󠁧󠁢󠁥󠁮󠁧󠁿", // tag sequence
            "1️⃣", // keycap
        ] {
            assert!(ReactionEmoji::new(value).is_ok(), "rejected {value:?}");
        }
    }

    #[test]
    fn preserves_the_sequence_verbatim() {
        // Trimming or normalizing would change which emoji is rendered.
        let emoji = ReactionEmoji::new("👍🏽").unwrap();
        assert_eq!(emoji.as_str(), "👍🏽");
    }

    #[test]
    fn rejects_text_and_whitespace() {
        for value in ["", " ", "lol", "🔥 ", " 🔥", "🔥x", "42", "#"] {
            assert!(ReactionEmoji::new(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn rejects_a_string_of_emoji() {
        // A reaction is one emoji; a wall of them would blow out the tally UI.
        assert!(ReactionEmoji::new("🔥".repeat(ReactionEmoji::MAX_SCALARS + 1)).is_err());
    }
}
