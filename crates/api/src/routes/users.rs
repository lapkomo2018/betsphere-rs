use application::ApplicationError;
use application::use_cases::user::MAX_AVATAR_BYTES;
use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::entities::{User, UserId};
use domain::repositories::UserStats;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::bets::{BetListQuery, BetResponse, to_responses};
use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, BetState, UserState};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_me))
        .routes(routes!(get_user))
        .routes(routes!(get_user_bets))
        .routes(routes!(upload_avatar))
        // Room for the avatar plus multipart framing; other routes have no body.
        .layer(DefaultBodyLimit::max(MAX_AVATAR_BYTES + 64 * 1024))
}

// --- DTOs ---

/// Aggregated betting record, shown on every profile.
#[derive(Debug, Serialize, ToSchema)]
struct UserStatsResponse {
    total_bets: i64,
    wins: i64,
    losses: i64,
    /// `wins / (wins + losses)`; 0 while nothing has settled.
    win_rate: f64,
    total_volume: i64,
}

impl From<UserStats> for UserStatsResponse {
    fn from(stats: UserStats) -> Self {
        Self {
            total_bets: stats.total_bets,
            wins: stats.wins,
            losses: stats.losses,
            win_rate: stats.win_rate(),
            total_volume: stats.total_volume,
        }
    }
}

/// Full profile, returned only to the account owner.
#[derive(Debug, Serialize, ToSchema)]
pub(super) struct PrivateUserResponse {
    id: Uuid,
    username: String,
    email: String,
    avatar_url: Option<String>,
    balance: i64,
    role: String,
    stats: UserStatsResponse,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PrivateUserResponse {
    pub(super) fn new(user: &User, stats: UserStats) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().to_string(),
            email: user.email().to_string(),
            avatar_url: user.avatar_url().map(str::to_owned),
            balance: user.balance(),
            role: user.role().to_string(),
            stats: stats.into(),
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
    stats: UserStatsResponse,
    created_at: DateTime<Utc>,
}

impl PublicUserResponse {
    fn new(user: &User, stats: UserStats) -> Self {
        Self {
            id: user.id().as_uuid(),
            username: user.username().to_string(),
            avatar_url: user.avatar_url().map(str::to_owned),
            stats: stats.into(),
            created_at: user.created_at(),
        }
    }
}

/// Multipart form for the avatar upload, documented for Swagger.
#[derive(ToSchema)]
#[allow(dead_code)] // only used for the OpenAPI schema
struct AvatarUploadForm {
    /// Image file: png, jpeg, or webp, at most 2 MiB.
    #[schema(value_type = String, format = Binary)]
    file: String,
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
    State(state): State<UserState>,
    CurrentUser(claims): CurrentUser,
) -> Result<Json<PrivateUserResponse>, ApiError> {
    let user = state.get_user.execute(claims.user_id.as_uuid()).await?;
    let stats = state.get_user_stats.execute(user.id()).await?;
    Ok(Json(PrivateUserResponse::new(&user, stats)))
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
    State(state): State<UserState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PublicUserResponse>, ApiError> {
    let user = state.get_user.execute(id).await?;
    let stats = state.get_user_stats.execute(user.id()).await?;
    Ok(Json(PublicUserResponse::new(&user, stats)))
}

#[utoipa::path(
    get,
    path = "/{id}/bets",
    tag = "users",
    params(
        ("id" = Uuid, Path, description = "User id"),
        ("sort" = Option<String>, Query, description = "newest | popular (biggest stakes)"),
        ("status" = Option<String>, Query, description = "active | won | lost | refunded"),
        ("page" = Option<i64>, Query, description = "1-based page number"),
        ("limit" = Option<i64>, Query, description = "Page size (max 100, default 20)"),
    ),
    responses(
        (status = 200, description = "The user's bet history", body = [BetResponse]),
        (status = 404, description = "User not found", body = ErrorResponse),
        (status = 422, description = "Invalid filter", body = ErrorResponse),
    )
)]
async fn get_user_bets(
    State(state): State<BetState>,
    Path(id): Path<Uuid>,
    Query(query): Query<BetListQuery>,
) -> Result<Json<Vec<BetResponse>>, ApiError> {
    let filter = query.into_filter()?;
    let views = state.user_bets.execute(UserId::from(id), &filter).await?;
    Ok(Json(to_responses(&views)))
}

#[utoipa::path(
    post,
    path = "/me/avatar",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body(content = AvatarUploadForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Profile with the new avatar URL", body = PrivateUserResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 422, description = "Missing `file` field, unsupported image type, or file too large", body = ErrorResponse),
    )
)]
async fn upload_avatar(
    State(state): State<UserState>,
    CurrentUser(claims): CurrentUser,
    mut multipart: Multipart,
) -> Result<Json<PrivateUserResponse>, ApiError> {
    let invalid =
        |msg: String| ApiError::from(ApplicationError::from(DomainError::Validation(msg)));

    let mut file = None;
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
            file = Some((content_type, bytes));
            break;
        }
    }
    let (content_type, bytes) = file.ok_or_else(|| invalid("missing `file` field".into()))?;

    let user = state
        .upload_avatar
        .execute(claims.user_id, &content_type, &bytes)
        .await?;
    let stats = state.get_user_stats.execute(user.id()).await?;
    Ok(Json(PrivateUserResponse::new(&user, stats)))
}
