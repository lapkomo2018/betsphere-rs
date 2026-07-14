use crate::DomainError;

/// A validated market title: non-empty after trimming, at most 200 chars.
/// The stored value is trimmed of surrounding whitespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketTitle(String);

impl MarketTitle {
    /// Longest title we accept, in Unicode scalar values.
    pub const MAX_CHARS: usize = 200;

    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = value.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::Validation("title must not be empty".into()));
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(DomainError::Validation(format!(
                "title must be at most {} characters",
                Self::MAX_CHARS
            )));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MarketTitle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_valid_title() {
        let title = MarketTitle::new("  Will it rain?  ").unwrap();
        assert_eq!(title.as_str(), "Will it rain?");
    }

    #[test]
    fn rejects_empty_or_too_long() {
        assert!(MarketTitle::new("   ").is_err());
        assert!(MarketTitle::new("a".repeat(MarketTitle::MAX_CHARS + 1)).is_err());
        assert!(MarketTitle::new("a".repeat(MarketTitle::MAX_CHARS)).is_ok());
    }
}
