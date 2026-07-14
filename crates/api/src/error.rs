use application::ApplicationError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// Error body returned by every failing endpoint.
#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// Maps application-layer errors onto HTTP responses.
pub struct ApiError(ApplicationError);

impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            ApplicationError::Domain(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApplicationError::NotFound(_) => StatusCode::NOT_FOUND,
            ApplicationError::Conflict(_) => StatusCode::CONFLICT,
            ApplicationError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApplicationError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApplicationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "internal error");
        }

        let body = ErrorResponse {
            error: self.0.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
