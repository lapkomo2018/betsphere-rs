use crate::DomainError;

/// A validated chat message body: non-empty after trimming, at most 2000 chars.
/// The stored value is trimmed of surrounding whitespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageBody(String);

impl MessageBody {
    /// Longest message we accept, in Unicode scalar values.
    pub const MAX_CHARS: usize = 2000;

    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::Validation("message must not be empty".into()));
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(DomainError::Validation(format!(
                "message must be at most {} characters",
                Self::MAX_CHARS
            )));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MessageBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_valid_body() {
        let body = MessageBody::new("  hello world  ").unwrap();
        assert_eq!(body.as_str(), "hello world");
    }

    #[test]
    fn rejects_empty_or_whitespace() {
        assert!(MessageBody::new("").is_err());
        assert!(MessageBody::new("   \n\t ").is_err());
    }

    #[test]
    fn rejects_too_long() {
        assert!(MessageBody::new("a".repeat(MessageBody::MAX_CHARS + 1)).is_err());
        assert!(MessageBody::new("a".repeat(MessageBody::MAX_CHARS)).is_ok());
    }
}
