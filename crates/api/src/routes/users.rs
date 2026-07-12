use application::use_cases::user::CreateUserInput;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::entities::User;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ErrorResponse};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users, create_user))
        .routes(routes!(get_user))
}

// --- DTOs ---

#[derive(Debug, Deserialize, ToSchema)]
struct CreateUserRequest {
    #[schema(example = "alice_01")]
    username: String,
    #[schema(example = "alice@example.com")]
    email: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct UserResponse {
    id: Uuid,
    username: String,
    email: String,
    created_at: DateTime<Utc>,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().to_string(),
            email: user.email().to_string(),
            created_at: user.created_at(),
        }
    }
}

// --- Handlers ---

#[utoipa::path(
    post,
    path = "",
    tag = "users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = UserResponse),
        (status = 409, description = "Email already in use", body = ErrorResponse),
        (status = 422, description = "Validation failed", body = ErrorResponse),
    )
)]
async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    let user = state
        .create_user
        .execute(CreateUserInput {
            username: body.username,
            email: body.email,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(&user))))
}

#[utoipa::path(
    get,
    path = "/{id}",
    tag = "users",
    params(("id" = Uuid, Path, description = "User id")),
    responses(
        (status = 200, description = "User found", body = UserResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserResponse>, ApiError> {
    let user = state.get_user.execute(id).await?;
    Ok(Json(UserResponse::from(&user)))
}

#[utoipa::path(
    get,
    path = "",
    tag = "users",
    responses((status = 200, description = "All users", body = [UserResponse]))
)]
async fn list_users(State(state): State<AppState>) -> Result<Json<Vec<UserResponse>>, ApiError> {
    let users = state.list_users.execute().await?;
    Ok(Json(users.iter().map(UserResponse::from).collect()))
}
