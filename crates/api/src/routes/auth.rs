use application::use_cases::auth::{AuthSession, LoginInput, RegisterInput};
use application::ApplicationError;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::{ApiError, ErrorResponse};
use crate::routes::users::PrivateUserResponse;
use crate::state::AppState;

/// Cookie carrying the refresh token. Scoped to the auth endpoints so the
/// browser never sends it anywhere else.
const REFRESH_COOKIE: &str = "refresh_token";
const REFRESH_COOKIE_PATH: &str = "/api/auth";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(login))
        .routes(routes!(refresh))
        .routes(routes!(logout))
}

// --- DTOs ---

#[derive(Debug, Deserialize, ToSchema)]
struct RegisterRequest {
    #[schema(example = "alice_01")]
    username: String,
    #[schema(example = "alice@example.com")]
    email: String,
    #[schema(example = "correct horse battery")]
    password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
struct LoginRequest {
    #[schema(example = "alice@example.com")]
    email: String,
    #[schema(example = "correct horse battery")]
    password: String,
}

/// Access token in the body; the refresh token travels in an httpOnly cookie.
#[derive(Debug, Serialize, ToSchema)]
struct AuthResponse {
    access_token: String,
    user: PrivateUserResponse,
}

impl From<&AuthSession> for AuthResponse {
    fn from(session: &AuthSession) -> Self {
        Self {
            access_token: session.access_token.clone(),
            user: PrivateUserResponse::from(&session.user),
        }
    }
}

// --- Cookie helpers ---

fn refresh_cookie(session: &AuthSession, secure: bool) -> Cookie<'static> {
    let max_age = (session.refresh_expires_at - Utc::now())
        .to_std()
        .unwrap_or_default();
    Cookie::build((REFRESH_COOKIE, session.refresh_token.clone()))
        .path(REFRESH_COOKIE_PATH)
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::try_from(max_age).unwrap_or(time::Duration::ZERO))
        .build()
}

fn removal_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(REFRESH_COOKIE, "");
    cookie.set_path(REFRESH_COOKIE_PATH);
    cookie
}

// --- Handlers ---

#[utoipa::path(
    post,
    path = "/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created and logged in", body = AuthResponse),
        (status = 409, description = "Email or username already taken", body = ErrorResponse),
        (status = 422, description = "Validation failed", body = ErrorResponse),
    )
)]
async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, CookieJar, Json<AuthResponse>), ApiError> {
    let session = state
        .register
        .execute(RegisterInput {
            username: body.username,
            email: body.email,
            password: body.password,
        })
        .await?;

    let jar = jar.add(refresh_cookie(&session, state.cookie_secure));
    Ok((StatusCode::CREATED, jar, Json(AuthResponse::from(&session))))
}

#[utoipa::path(
    post,
    path = "/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in", body = AuthResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
    )
)]
async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), ApiError> {
    let session = state
        .login
        .execute(LoginInput {
            email: body.email,
            password: body.password,
        })
        .await?;

    let jar = jar.add(refresh_cookie(&session, state.cookie_secure));
    Ok((jar, Json(AuthResponse::from(&session))))
}

#[utoipa::path(
    post,
    path = "/refresh",
    tag = "auth",
    responses(
        (status = 200, description = "New token pair issued", body = AuthResponse),
        (status = 401, description = "Missing, invalid or expired refresh token", body = ErrorResponse),
    )
)]
async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<AuthResponse>), ApiError> {
    let token = jar
        .get(REFRESH_COOKIE)
        .map(|c| c.value().to_owned())
        .ok_or_else(|| ApplicationError::Unauthorized("missing refresh token".into()))?;

    let session = state.refresh_session.execute(&token).await?;

    let jar = jar.add(refresh_cookie(&session, state.cookie_secure));
    Ok((jar, Json(AuthResponse::from(&session))))
}

#[utoipa::path(
    post,
    path = "/logout",
    tag = "auth",
    responses((status = 204, description = "Refresh token invalidated"))
)]
async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    if let Some(cookie) = jar.get(REFRESH_COOKIE) {
        state.logout.execute(cookie.value()).await?;
    }
    let jar = jar.remove(removal_cookie());
    Ok((StatusCode::NO_CONTENT, jar))
}
