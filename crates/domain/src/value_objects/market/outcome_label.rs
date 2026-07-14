use crate::DomainError;

/// A validated outcome label (e.g. "Yes" / "No" / a specific option):
/// non-empty after trimming, at most 100 chars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeLabel(String);

impl OutcomeLabel {
    /// Longest label we accept, in Unicode scalar values.
    pub const MAX_CHARS: usize = 100;

    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = value.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::Validation(
                "outcome label must not be empty".into(),
            ));
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(DomainError::Validation(format!(
                "outcome label must be at most {} characters",
                Self::MAX_CHARS
            )));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OutcomeLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_valid_label() {
        assert_eq!(OutcomeLabel::new("  Yes ").unwrap().as_str(), "Yes");
    }

    #[test]
    fn rejects_empty_or_too_long() {
        assert!(OutcomeLabel::new("").is_err());
        assert!(OutcomeLabel::new("x".repeat(OutcomeLabel::MAX_CHARS + 1)).is_err());
    }
}
