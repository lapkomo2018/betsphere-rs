use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::ports::{AccessClaims, AccessTokenService, AuthPortError};
use domain::entities::{Role, UserId};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    role: String,
    iat: i64,
    exp: i64,
}

/// HS256-signed JWT access tokens.
pub struct JwtAccessTokens {
    encoding: EncodingKey,
    decoding: DecodingKey,
    ttl: Duration,
}

impl JwtAccessTokens {
    pub fn new(secret: &str, ttl: Duration) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            ttl,
        }
    }
}

impl AccessTokenService for JwtAccessTokens {
    fn issue(&self, user_id: UserId, role: Role) -> Result<String, AuthPortError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.as_uuid(),
            role: role.as_str().to_owned(),
            iat: now.timestamp(),
            exp: (now + self.ttl).timestamp(),
        };
        encode(&Header::default(), &claims, &self.encoding)
            .map_err(|e| AuthPortError::Internal(format!("jwt signing failed: {e}")))
    }

    fn verify(&self, token: &str) -> Result<AccessClaims, AuthPortError> {
        // Validation::default() checks the HS256 signature and `exp`.
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())
            .map_err(|_| AuthPortError::InvalidToken)?;
        let role = data
            .claims
            .role
            .parse()
            .map_err(|_| AuthPortError::InvalidToken)?;
        Ok(AccessClaims {
            user_id: data.claims.sub.into(),
            role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_and_verifies_round_trip() {
        let service = JwtAccessTokens::new("test-secret", Duration::minutes(15));
        let user_id = UserId::new();

        let token = service.issue(user_id, Role::Admin).unwrap();
        let claims = service.verify(&token).unwrap();

        assert_eq!(claims.user_id, user_id);
        assert_eq!(claims.role, Role::Admin);
    }

    #[test]
    fn rejects_token_signed_with_other_secret() {
        let service = JwtAccessTokens::new("secret-a", Duration::minutes(15));
        let other = JwtAccessTokens::new("secret-b", Duration::minutes(15));

        let token = other.issue(UserId::new(), Role::User).unwrap();
        assert!(matches!(
            service.verify(&token),
            Err(AuthPortError::InvalidToken)
        ));
    }

    #[test]
    fn rejects_expired_token() {
        // jsonwebtoken's default validation allows 60s leeway, so back-date well past it.
        let service = JwtAccessTokens::new("test-secret", Duration::seconds(-300));
        let token = service.issue(UserId::new(), Role::User).unwrap();
        assert!(matches!(
            service.verify(&token),
            Err(AuthPortError::InvalidToken)
        ));
    }
}
