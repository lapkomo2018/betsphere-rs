use std::sync::Arc;

use application::ports::{AccessTokenService, PasswordHasher};
use application::use_cases::auth::{Login, Logout, RefreshSession, Register, SessionIssuer};
use application::use_cases::user::GetUser;
use chrono::Duration;
use domain::repositories::{RefreshTokenRepository, UnitOfWork, UserRepository};

/// Shared handler state holding the wired-up use cases.
#[derive(Clone)]
pub struct AppState {
    pub register: Arc<Register>,
    pub login: Arc<Login>,
    pub refresh_session: Arc<RefreshSession>,
    pub logout: Arc<Logout>,
    pub get_user: Arc<GetUser>,
    /// Used by the auth extractor to verify bearer tokens.
    pub access_tokens: Arc<dyn AccessTokenService>,
    /// Whether the refresh cookie carries the `Secure` flag.
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(
        users: Arc<dyn UserRepository>,
        refresh_tokens: Arc<dyn RefreshTokenRepository>,
        uow: Arc<dyn UnitOfWork>,
        hasher: Arc<dyn PasswordHasher>,
        access_tokens: Arc<dyn AccessTokenService>,
        refresh_ttl: Duration,
        cookie_secure: bool,
    ) -> Self {
        let sessions = Arc::new(SessionIssuer::new(access_tokens.clone(), refresh_ttl));

        Self {
            register: Arc::new(Register::new(uow.clone(), hasher.clone(), sessions.clone())),
            login: Arc::new(Login::new(
                users.clone(),
                refresh_tokens.clone(),
                hasher,
                sessions.clone(),
            )),
            refresh_session: Arc::new(RefreshSession::new(uow, sessions)),
            logout: Arc::new(Logout::new(refresh_tokens)),
            get_user: Arc::new(GetUser::new(users)),
            access_tokens,
            cookie_secure,
        }
    }
}
