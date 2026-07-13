//! End-to-end smoke tests: the real router over in-memory infrastructure.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Duration;
use infrastructure::auth::{Argon2PasswordHasher, JwtAccessTokens};
use infrastructure::persistence::in_memory::{
    InMemoryRefreshTokenRepository, InMemoryUnitOfWork, InMemoryUserRepository,
};
use infrastructure::storage::LocalFileStorage;
use tower::ServiceExt;

use crate::routes;
use crate::state::{AppState, AuthState, FileState, UserState};

const APP_URL: &str = "http://localhost:8080";

fn test_app() -> Router {
    let users = Arc::new(InMemoryUserRepository::new());
    let refresh_tokens = Arc::new(InMemoryRefreshTokenRepository::new());
    let uow = Arc::new(InMemoryUnitOfWork::new(
        users.clone(),
        refresh_tokens.clone(),
    ));
    let storage_dir =
        std::env::temp_dir().join(format!("betsphere-api-test-{}", uuid::Uuid::new_v4()));
    let storage = Arc::new(LocalFileStorage::new(
        storage_dir,
        format!("{APP_URL}{}", routes::FILES_PUBLIC_BASE),
    ));
    let state = AppState {
        auth: AuthState::new(
            users.clone(),
            refresh_tokens,
            uow,
            Arc::new(Argon2PasswordHasher::new()),
            Arc::new(JwtAccessTokens::new("test-secret", Duration::minutes(5))),
            Duration::days(1),
            false,
        ),
        users: UserState::new(users, storage.clone()),
        files: FileState::new(storage),
    };
    routes::router(state)
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Registers a user and returns their access token.
async fn register(app: &Router) -> String {
    let request = Request::post("/api/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"username":"alice","email":"alice@example.com","password":"correct horse"}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    body_json(response).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn multipart_body(content_type: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    const BOUNDARY: &str = "test-boundary";
    let mut body = format!(
        "--{BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"a\"\r\n\
         Content-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={BOUNDARY}"), body)
}

async fn upload_avatar(
    app: &Router,
    token: &str,
    content_type: &str,
    bytes: &[u8],
) -> axum::response::Response {
    let (mime, body) = multipart_body(content_type, bytes);
    let request = Request::post("/api/users/me/avatar")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(body))
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn avatar_upload_and_read_round_trip() {
    let app = test_app();
    let token = register(&app).await;

    let response = upload_avatar(&app, &token, "image/png", b"fake-png-bytes").await;
    assert_eq!(response.status(), StatusCode::OK);
    let profile = body_json(response).await;
    let avatar_url = profile["avatar_url"].as_str().unwrap();
    assert!(
        avatar_url.starts_with("http://localhost:8080/api/files/avatars/"),
        "unexpected avatar_url: {avatar_url}"
    );

    // The URL from the profile serves the uploaded bytes back.
    let path = avatar_url.strip_prefix(APP_URL).unwrap();
    let request = Request::get(path).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE.as_str()],
        "image/png"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&bytes[..], b"fake-png-bytes");
}

#[tokio::test]
async fn avatar_upload_rejects_unsupported_content_type() {
    let app = test_app();
    let token = register(&app).await;

    let response = upload_avatar(&app, &token, "application/pdf", b"%PDF").await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn avatar_upload_requires_auth() {
    let app = test_app();
    let (mime, body) = multipart_body("image/png", b"x");
    let request = Request::post("/api/users/me/avatar")
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_file_returns_404() {
    let app = test_app();
    let request = Request::get("/api/files/avatars/nope.png")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
