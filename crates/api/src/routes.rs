mod auth;
mod bets;
mod chat;
mod files;
mod health;
mod internal;
mod markets;
mod tests_page;
mod users;
mod ws;

pub use files::PUBLIC_BASE as FILES_PUBLIC_BASE;

use axum::Router;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Betsphere API",
        description = "Betting platform backend",
        version = env!("CARGO_PKG_VERSION"),
    ),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

/// Registers the `bearer_auth` scheme referenced by protected endpoints.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        components.add_security_scheme(
            "internal_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Internal-Key"))),
        );
    }
}

pub fn router(state: AppState) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::router())
        .merge(tests_page::router())
        .merge(ws::router())
        .nest("/api/auth", auth::router())
        .nest("/api/users", users::router())
        .nest("/api/markets", markets::router())
        .nest("/api/bets", bets::router())
        .nest("/api/chat", chat::router())
        .nest("/api/internal", internal::router())
        .nest(files::PUBLIC_BASE, files::router())
        .split_for_parts();

    router
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
}
