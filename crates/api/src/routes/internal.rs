//! Internal/system endpoints, guarded by a shared secret rather than a user
//! token. Not meant to be exposed to end users — see [`InternalAuth`].

use application::use_cases::user::UpdateUserInput;
use axum::Json;
use axum::extract::{Path, State};
use domain::DomainError;
use domain::entities::Role;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::users::PrivateUserResponse;
use crate::error::{ApiError, ErrorResponse};
use crate::extract::InternalAuth;
use crate::state::{AppState, InternalState, UserState};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(update_user))
}

// --- DTOs ---

/// Patch for a user. Every field is optional; omit a field to leave it as-is.
#[derive(Debug, Deserialize, ToSchema)]
struct UpdateUserRequest {
    /// New role: `user` or `admin`.
    #[schema(example = "admin")]
    role: Option<String>,
}

impl UpdateUserRequest {
    fn into_input(self) -> Result<UpdateUserInput, DomainError> {
        let role = self.role.map(|r| r.parse::<Role>()).transpose()?;
        Ok(UpdateUserInput { role })
    }
}

// --- Handlers ---

#[utoipa::path(
    patch,
    path = "/users/{id}",
    tag = "internal",
    security(("internal_key" = [])),
    params(("id" = Uuid, Path, description = "User id")),
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "Updated profile", body = PrivateUserResponse),
        (status = 403, description = "Missing or invalid internal API key", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
        (status = 422, description = "No fields to update or invalid value", body = ErrorResponse),
    )
)]
async fn update_user(
    State(internal): State<InternalState>,
    State(users): State<UserState>,
    _auth: InternalAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<PrivateUserResponse>, ApiError> {
    let input = body
        .into_input()
        .map_err(application::ApplicationError::from)?;
    let user = internal.update_user.execute(id, input).await?;
    let stats = users.get_user_stats.execute(user.id()).await?;
    Ok(Json(PrivateUserResponse::new(&user, stats)))
}
