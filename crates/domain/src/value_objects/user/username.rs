use crate::DomainError;

/// A validated username: 3-32 chars, alphanumeric plus `_` and `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

impl Username {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let len = value.chars().count();
        if !(3..=32).contains(&len) {
            return Err(DomainError::Validation(
                "username must be 3-32 characters".into(),
            ));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DomainError::Validation(
                "username may only contain letters, digits, '_' and '-'".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_username() {
        assert!(Username::new("alice_01").is_ok());
    }

    #[test]
    fn rejects_bad_usernames() {
        assert!(Username::new("ab").is_err());
        assert!(Username::new("has spaces").is_err());
        assert!(Username::new("a".repeat(33)).is_err());
    }
}
