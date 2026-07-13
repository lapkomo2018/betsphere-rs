//! Request extractors.

use application::ports::AccessClaims;
use application::ApplicationError;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use crate::error::ApiError;
use crate::state::AppState;

/// Extracts and verifies the `Authorization: Bearer <jwt>` header. Handlers
/// that take this parameter reject unauthenticated requests with 401.
pub struct CurrentUser(pub AccessClaims);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let unauthorized = || {
            ApiError::from(ApplicationError::Unauthorized(
                "missing bearer token".into(),
            ))
        };

        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(unauthorized)?;

        let claims = state
            .auth
            .access_tokens
            .verify(token)
            .map_err(ApplicationError::from)?;

        Ok(Self(claims))
    }
}
