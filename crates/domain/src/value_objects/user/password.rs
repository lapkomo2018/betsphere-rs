use crate::DomainError;

/// A validated plaintext password. Exists only in memory during
/// registration/login; never persisted or logged.
pub struct Password(String);

impl Password {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let len = value.chars().count();
        if !(8..=256).contains(&len) {
            return Err(DomainError::Validation(
                "password must be 8-256 characters".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Never leak the password through Debug output.
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// An opaque password hash (PHC string produced by Argon2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordHash(String);

impl PasswordHash {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_password() {
        assert!(Password::new("secret-password").is_ok());
    }

    #[test]
    fn rejects_short_and_long_passwords() {
        assert!(Password::new("short").is_err());
        assert!(Password::new("a".repeat(257)).is_err());
    }

    #[test]
    fn debug_does_not_leak() {
        let p = Password::new("secret-password").unwrap();
        assert_eq!(format!("{p:?}"), "Password(***)");
    }
}
