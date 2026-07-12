mod health;
mod users;

use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(info(
    title = "Betsphere API",
    description = "Betting platform backend",
    version = env!("CARGO_PKG_VERSION"),
))]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::router())
        .nest("/api/users", users::router())
        .split_for_parts();

    router
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
}
