//! End-to-end smoke tests: the real router over in-memory infrastructure.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Duration;
use futures::{SinkExt, StreamExt};
use infrastructure::auth::{Argon2PasswordHasher, JwtAccessTokens};
use infrastructure::messaging::InMemoryMessageBroker;
use infrastructure::persistence::in_memory::{
    InMemoryBetRepository, InMemoryChatMessageRepository, InMemoryMarketRepository,
    InMemoryRefreshTokenRepository, InMemoryUnitOfWork, InMemoryUserRepository,
};
use infrastructure::storage::LocalFileStorage;
use std::net::SocketAddr;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tower::ServiceExt;

use crate::routes;
use crate::state::{AppState, AuthState, BetState, ChatState, FileState, MarketState, UserState};

const APP_URL: &str = "http://localhost:8080";

fn test_app() -> Router {
    test_env().0
}

/// The router plus the market store, so tests can seed markets without an
/// admin account.
fn test_env() -> (Router, Arc<InMemoryMarketRepository>) {
    let users = Arc::new(InMemoryUserRepository::new());
    let markets = Arc::new(InMemoryMarketRepository::new());
    let bets = Arc::new(InMemoryBetRepository::new(markets.clone(), users.clone()));
    let refresh_tokens = Arc::new(InMemoryRefreshTokenRepository::new());
    let chat_messages = Arc::new(InMemoryChatMessageRepository::new());
    let uow = Arc::new(InMemoryUnitOfWork::new(
        users.clone(),
        refresh_tokens.clone(),
    ));
    let access_tokens = Arc::new(JwtAccessTokens::new("test-secret", Duration::minutes(5)));
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
            access_tokens.clone(),
            Duration::days(1),
            false,
        ),
        users: UserState::new(users.clone(), bets.clone(), storage.clone()),
        files: FileState::new(storage),
        chat: ChatState::new(
            chat_messages,
            users.clone(),
            markets.clone(),
            access_tokens,
            Arc::new(InMemoryMessageBroker::new()),
        ),
        markets: MarketState::new(markets.clone(), bets.clone()),
        bets: BetState::new(bets, markets.clone(), users),
    };
    (routes::router(state), markets)
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

// --- Chat / WebSocket ---

/// Serves `app` on an ephemeral local port and returns the bound address.
async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// Reads the next text frame, skipping ping/pong control frames.
async fn next_text<S>(ws: &mut S) -> String
where
    S: StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match ws.next().await.expect("stream closed").expect("ws error") {
            WsMessage::Text(text) => return text.to_string(),
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

/// Sends a subscribe frame and asserts the history frame that answers it,
/// returning its `data` array.
async fn subscribe<S>(ws: &mut S, channel: &str) -> serde_json::Value
where
    S: StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + SinkExt<WsMessage>
        + Unpin,
    <S as futures::Sink<WsMessage>>::Error: std::fmt::Debug,
{
    ws.send(WsMessage::text(format!(
        r#"{{"type":"subscribe","channel":"{channel}"}}"#
    )))
    .await
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&next_text(ws).await).unwrap();
    assert_eq!(value["type"], "history", "unexpected frame: {value}");
    assert_eq!(value["channel"], channel);
    value["data"].clone()
}

#[tokio::test]
async fn chat_ws_posts_and_echoes_message() {
    let app = test_app();
    let token = register(&app).await;
    let addr = serve(app).await;

    let url = format!("ws://{addr}/api/chat/ws?token={token}");
    let (mut ws, _) = connect_async(url).await.unwrap();

    let history = subscribe(&mut ws, "global_chat").await;
    assert_eq!(history, serde_json::json!([]));

    ws.send(WsMessage::text(
        r#"{"type":"chat_message","channel":"global_chat","body":"hello world"}"#,
    ))
    .await
    .unwrap();

    let value: serde_json::Value = serde_json::from_str(&next_text(&mut ws).await).unwrap();
    assert_eq!(value["type"], "chat_message");
    assert_eq!(value["channel"], "global_chat");
    assert_eq!(value["data"]["body"], "hello world");
    assert_eq!(value["data"]["author"]["username"], "alice");
    assert!(value["data"]["id"].is_string());
    assert!(value["data"]["created_at"].is_string());
}

#[tokio::test]
async fn chat_ws_replays_history_to_new_client() {
    let app = test_app();
    let token = register(&app).await;
    let addr = serve(app).await;
    let url = format!("ws://{addr}/api/chat/ws?token={token}");

    // First client posts a message and waits for its own echo, which only
    // fires after the message is persisted.
    let (mut first, _) = connect_async(url.clone()).await.unwrap();
    subscribe(&mut first, "global_chat").await;
    first
        .send(WsMessage::text(
            r#"{"type":"chat_message","channel":"global_chat","body":"earlier message"}"#,
        ))
        .await
        .unwrap();
    let _ = next_text(&mut first).await;

    // A freshly subscribing client receives that message as history.
    let (mut second, _) = connect_async(url).await.unwrap();
    let history = subscribe(&mut second, "global_chat").await;
    assert_eq!(history[0]["body"], "earlier message");
}

#[tokio::test]
async fn market_chat_is_scoped_to_its_market() {
    let (app, markets) = test_env();
    let (market_id, _) = seed_market(&markets).await;
    let token = register(&app).await;
    let addr = serve(app).await;
    let url = format!("ws://{addr}/api/chat/ws?token={token}");

    let market_channel = format!("market_chat:{market_id}");
    let (mut ws, _) = connect_async(url.clone()).await.unwrap();
    subscribe(&mut ws, &market_channel).await;
    ws.send(WsMessage::text(format!(
        r#"{{"type":"chat_message","channel":"{market_channel}","body":"market talk"}}"#
    )))
    .await
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&next_text(&mut ws).await).unwrap();
    assert_eq!(value["channel"], market_channel.as_str());
    assert_eq!(value["data"]["body"], "market talk");

    // The market room's message does not leak into the global room.
    let (mut global, _) = connect_async(url).await.unwrap();
    let history = subscribe(&mut global, "global_chat").await;
    assert_eq!(history, serde_json::json!([]));
}

#[tokio::test]
async fn chat_ws_rejects_message_to_unknown_market() {
    let app = test_app();
    let token = register(&app).await;
    let addr = serve(app).await;

    let url = format!("ws://{addr}/api/chat/ws?token={token}");
    let (mut ws, _) = connect_async(url).await.unwrap();

    let ghost = uuid::Uuid::new_v4();
    ws.send(WsMessage::text(format!(
        r#"{{"type":"chat_message","channel":"market_chat:{ghost}","body":"anyone?"}}"#
    )))
    .await
    .unwrap();

    let value: serde_json::Value = serde_json::from_str(&next_text(&mut ws).await).unwrap();
    assert_eq!(value["type"], "error");
}

#[tokio::test]
async fn chat_ws_rejects_invalid_token() {
    let app = test_app();
    let addr = serve(app).await;

    let url = format!("ws://{addr}/api/chat/ws?token=not-a-real-token");
    assert!(connect_async(url).await.is_err());
}

#[tokio::test]
async fn chat_history_endpoint_requires_auth() {
    let app = test_app();
    let request = Request::get("/api/chat/messages")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- Markets ---

#[tokio::test]
async fn markets_listing_is_public_and_starts_empty() {
    let app = test_app();
    let request = Request::get("/api/markets").body(Body::empty()).unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, serde_json::json!([]));
}

#[tokio::test]
async fn creating_a_market_requires_admin() {
    let app = test_app();
    let token = register(&app).await; // registers as a regular `user`
    let request = Request::post("/api/markets")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"title":"Will it rain?","outcomes":["Yes","No"]}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_market_returns_404() {
    let app = test_app();
    let id = uuid::Uuid::new_v4();
    let request = Request::get(format!("/api/markets/{id}"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// --- Bets ---

/// Seeds an open Yes/No market at even prices and returns its id and the
/// "Yes" outcome's id.
async fn seed_market(markets: &InMemoryMarketRepository) -> (uuid::Uuid, uuid::Uuid) {
    use domain::entities::{Market, Outcome};
    use domain::repositories::MarketRepository;
    use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};

    let market = Market::new(MarketTitle::new("Will it rain?").unwrap(), None, None, None);
    let outcomes = vec![
        Outcome::new(
            market.id(),
            OutcomeLabel::new("Yes").unwrap(),
            Price::from_ten_thousandths(5_000).unwrap(),
        ),
        Outcome::new(
            market.id(),
            OutcomeLabel::new("No").unwrap(),
            Price::from_ten_thousandths(5_000).unwrap(),
        ),
    ];
    markets.create(&market, &outcomes).await.unwrap();
    (market.id().as_uuid(), outcomes[0].id().as_uuid())
}

async fn post_bet(
    app: &Router,
    token: &str,
    market_id: uuid::Uuid,
    outcome_id: uuid::Uuid,
    amount: i64,
) -> axum::response::Response {
    let request = Request::post("/api/bets")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"market_id":"{market_id}","outcome_id":"{outcome_id}","amount":{amount}}}"#
        )))
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn placing_a_bet_debits_balance_and_shows_in_feed() {
    let (app, markets) = test_env();
    let (market_id, yes_id) = seed_market(&markets).await;
    let token = register(&app).await;

    let response = post_bet(&app, &token, market_id, yes_id, 1_000).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let bet = body_json(response).await;
    assert_eq!(bet["status"], "active");
    assert_eq!(bet["username"], "alice");
    assert_eq!(bet["amount"], 1_000);
    assert_eq!(bet["price"], 0.5); // fixed at the pre-bet even split

    // The stake left the balance.
    let request = Request::get("/api/users/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let me = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(me["balance"], 9_000);

    // The bet shows up in the public feed with its display names joined on.
    let request = Request::get("/api/bets/feed").body(Body::empty()).unwrap();
    let feed = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(feed.as_array().unwrap().len(), 1);
    assert_eq!(feed[0]["market_title"], "Will it rain?");
    assert_eq!(feed[0]["outcome_label"], "Yes");
}

#[tokio::test]
async fn placing_a_bet_requires_auth() {
    let (app, markets) = test_env();
    let (market_id, yes_id) = seed_market(&markets).await;
    let request = Request::post("/api/bets")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"market_id":"{market_id}","outcome_id":"{yes_id}","amount":100}}"#
        )))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn profile_stats_reflect_placed_bets() {
    let (app, markets) = test_env();
    let (market_id, yes_id) = seed_market(&markets).await;
    let token = register(&app).await;

    let response = post_bet(&app, &token, market_id, yes_id, 1_000).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let request = Request::get("/api/users/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let me = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(me["stats"]["total_bets"], 1);
    assert_eq!(me["stats"]["total_volume"], 1_000);

    // The public profile carries the same stats but no balance or email.
    let id = me["id"].as_str().unwrap();
    let request = Request::get(format!("/api/users/{id}"))
        .body(Body::empty())
        .unwrap();
    let profile = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(profile["stats"]["total_bets"], 1);
    assert_eq!(profile["stats"]["total_volume"], 1_000);
    assert_eq!(profile["stats"]["wins"], 0);
    assert_eq!(profile["stats"]["losses"], 0);
    assert_eq!(profile["stats"]["win_rate"], 0.0);
    assert!(profile.get("balance").is_none());
    assert!(profile.get("email").is_none());
}

#[tokio::test]
async fn bet_beyond_balance_is_rejected() {
    let (app, markets) = test_env();
    let (market_id, yes_id) = seed_market(&markets).await;
    let token = register(&app).await;

    // Starting balance is 10 000; the stake must not clear.
    let response = post_bet(&app, &token, market_id, yes_id, 20_000).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let request = Request::get("/api/users/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let me = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(me["balance"], 10_000);
}
