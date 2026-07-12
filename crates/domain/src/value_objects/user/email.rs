use crate::DomainError;

/// A validated email address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

impl Email {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_lowercase();
        let valid = value.split_once('@').is_some_and(|(local, host)| {
            !local.is_empty()
                && host.contains('.')
                && !host.starts_with('.')
                && !host.ends_with('.')
        });
        if !valid {
            return Err(DomainError::Validation(format!("invalid email: {value}")));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_email() {
        assert!(Email::new("User@Example.com").is_ok());
    }

    #[test]
    fn normalizes_case() {
        assert_eq!(
            Email::new("User@Example.com").unwrap().as_str(),
            "user@example.com"
        );
    }

    #[test]
    fn rejects_invalid_email() {
        assert!(Email::new("not-an-email").is_err());
        assert!(Email::new("@missing-local.com").is_err());
        assert!(Email::new("user@nodot").is_err());
    }
}
