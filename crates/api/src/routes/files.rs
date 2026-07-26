use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::{ApiError, ErrorResponse};
use crate::state::{AppState, FileState};

/// Route prefix the files router is nested under; storage adapters build
/// public URLs from it.
pub const PUBLIC_BASE: &str = "/api/files";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_file))
}

#[utoipa::path(
    get,
    path = "/{folder}/{name}",
    tag = "files",
    params(
        ("folder" = String, Path, description = "Storage folder, e.g. `avatars`, `thumbnails`, or `outcome-thumbnails`"),
        ("name" = String, Path, description = "File name"),
    ),
    responses(
        (status = 200, description = "File contents"),
        (status = 404, description = "File not found", body = ErrorResponse),
    )
)]
async fn get_file(
    State(state): State<FileState>,
    Path((folder, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let file = state.get_file.execute(&format!("{folder}/{name}")).await?;
    Ok((
        [
            (CONTENT_TYPE, file.content_type),
            // Safe to cache: image URLs carry a `?v=` that changes on re-upload.
            (CACHE_CONTROL, "public, max-age=86400".to_owned()),
        ],
        file.bytes,
    ))
}
