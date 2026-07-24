//! Request extractors.

use application::ApplicationError;
use application::ports::AccessClaims;
use axum::body::Bytes;
use axum::extract::{FromRef, FromRequestParts, Multipart};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use domain::DomainError;

use crate::error::ApiError;
use crate::state::{AppState, InternalState};

/// Header carrying the shared secret for the internal/system endpoints.
const INTERNAL_KEY_HEADER: &str = "X-Internal-Key";

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

/// Guards the internal/system endpoints. Succeeds only when the request carries
/// an `X-Internal-Key` header matching the configured secret; if no secret is
/// configured the internal API is treated as disabled and every request fails.
pub struct InternalAuth;

impl FromRequestParts<AppState> for InternalAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let internal = InternalState::from_ref(state);

        // Fail closed: no secret configured means the internal API is off.
        let Some(expected) = internal.api_key.as_deref() else {
            return Err(forbidden("internal API is disabled"));
        };

        let provided = parts
            .headers
            .get(INTERNAL_KEY_HEADER)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| forbidden("missing or malformed internal API key"))?;

        if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
            return Err(forbidden("invalid internal API key"));
        }

        Ok(Self)
    }
}

/// Pulls the `file` field out of a `multipart/form-data` upload, returning its
/// content type and bytes. Anything malformed or missing is a 422.
pub async fn multipart_file(mut multipart: Multipart) -> Result<(String, Bytes), ApiError> {
    let invalid =
        |msg: String| ApiError::from(ApplicationError::from(DomainError::Validation(msg)));

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| invalid(format!("invalid multipart body: {e}")))?
    {
        if field.name() == Some("file") {
            let content_type = field.content_type().unwrap_or_default().to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| invalid(format!("failed to read `file` field: {e}")))?;
            return Ok((content_type, bytes));
        }
    }
    Err(invalid("missing `file` field".into()))
}

fn forbidden(msg: &str) -> ApiError {
    ApiError::from(ApplicationError::Forbidden(msg.to_owned()))
}

/// Length-independent comparison that avoids leaking the secret through timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
