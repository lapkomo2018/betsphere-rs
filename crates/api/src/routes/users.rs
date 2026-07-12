use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use domain::entities::User;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_me))
        .routes(routes!(get_user))
}

// --- DTOs ---

/// Full profile, returned only to the account owner.
#[derive(Debug, Serialize, ToSchema)]
pub(super) struct PrivateUserResponse {
    id: Uuid,
    username: String,
    email: String,
    avatar_url: Option<String>,
    balance: i64,
    role: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<&User> for PrivateUserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().to_string(),
            email: user.email().to_string(),
            avatar_url: user.avatar_url().map(str::to_owned),
            balance: user.balance(),
            role: user.role().to_string(),
            created_at: user.created_at(),
            updated_at: user.updated_at(),
        }
    }
}

/// Public profile: no email, no balance.
#[derive(Debug, Serialize, ToSchema)]
struct PublicUserResponse {
    id: Uuid,
    username: String,
    avatar_url: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<&User> for PublicUserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().to_string(),
            avatar_url: user.avatar_url().map(str::to_owned),
            created_at: user.created_at(),
        }
    }
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/me",
    tag = "users",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current user", body = PrivateUserResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_me(
    State(state): State<AppState>,
    CurrentUser(claims): CurrentUser,
) -> Result<Json<PrivateUserResponse>, ApiError> {
    let user = state.get_user.execute(claims.user_id.as_uuid()).await?;
    Ok(Json(PrivateUserResponse::from(&user)))
}

#[utoipa::path(
    get,
    path = "/{id}",
    tag = "users",
    params(("id" = Uuid, Path, description = "User id")),
    responses(
        (status = 200, description = "Public profile", body = PublicUserResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PublicUserResponse>, ApiError> {
    let user = state.get_user.execute(id).await?;
    Ok(Json(PublicUserResponse::from(&user)))
}
