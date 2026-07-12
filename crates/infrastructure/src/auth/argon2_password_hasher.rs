use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{
    Error as PhcError, PasswordHash as PhcHash, PasswordHasher as _, PasswordVerifier as _,
    SaltString,
};
use argon2::Argon2;

use application::ports::{AuthPortError, PasswordHasher};
use domain::value_objects::user::{Password, PasswordHash};

/// Argon2id password hasher producing PHC-format hash strings.
#[derive(Default)]
pub struct Argon2PasswordHasher {
    argon: Argon2<'static>,
}

impl Argon2PasswordHasher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PasswordHasher for Argon2PasswordHasher {
    fn hash(&self, password: &Password) -> Result<PasswordHash, AuthPortError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon
            .hash_password(password.as_str().as_bytes(), &salt)
            .map_err(|e| AuthPortError::Internal(format!("argon2 hashing failed: {e}")))?;
        Ok(PasswordHash::new(hash.to_string()))
    }

    fn verify(&self, password: &Password, hash: &PasswordHash) -> Result<bool, AuthPortError> {
        let parsed = PhcHash::new(hash.as_str())
            .map_err(|e| AuthPortError::Internal(format!("stored hash is malformed: {e}")))?;
        match self
            .argon
            .verify_password(password.as_str().as_bytes(), &parsed)
        {
            Ok(()) => Ok(true),
            Err(PhcError::Password) => Ok(false),
            Err(e) => Err(AuthPortError::Internal(format!(
                "argon2 verification failed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let hasher = Argon2PasswordHasher::new();
        let password = Password::new("correct horse battery").unwrap();
        let hash = hasher.hash(&password).unwrap();

        assert!(hasher.verify(&password, &hash).unwrap());

        let wrong = Password::new("wrong password").unwrap();
        assert!(!hasher.verify(&wrong, &hash).unwrap());
    }
}
