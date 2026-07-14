use application::use_cases::market::{MarketView, NewMarket};
use application::{Actor, ApplicationError};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::entities::{MarketId, OutcomeId, PricePoint};
use domain::repositories::{MarketFilter, MarketSort, PriceHistoryQuery, PriceInterval};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, MarketState};

/// Hard cap on page size, matching the non-functional requirements.
const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 20;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_markets, create_market))
        .routes(routes!(get_featured))
        .routes(routes!(get_market))
        .routes(routes!(get_price_history))
        .routes(routes!(resolve_market))
}

// --- Response DTOs ---

/// One outcome as sent to clients. `price` is the implied probability in
/// `[0.0, 1.0]`.
#[derive(Debug, Serialize, ToSchema)]
struct OutcomeResponse {
    id: Uuid,
    label: String,
    price: f64,
    volume: i64,
}

/// A market with its outcomes.
#[derive(Debug, Serialize, ToSchema)]
struct MarketResponse {
    id: Uuid,
    title: String,
    description: Option<String>,
    category: Option<String>,
    status: String,
    resolved_outcome_id: Option<Uuid>,
    total_volume: i64,
    participants_count: i32,
    created_at: DateTime<Utc>,
    closes_at: Option<DateTime<Utc>>,
    outcomes: Vec<OutcomeResponse>,
}

impl From<&MarketView> for MarketResponse {
    fn from(view: &MarketView) -> Self {
        let m = &view.market;
        Self {
            id: m.id().as_uuid(),
            title: m.title().to_string(),
            description: m.description().map(str::to_owned),
            category: m.category().map(str::to_owned),
            status: m.status().to_string(),
            resolved_outcome_id: m.resolved_outcome_id().map(|id| id.as_uuid()),
            total_volume: m.total_volume(),
            participants_count: m.participants_count(),
            created_at: m.created_at(),
            closes_at: m.closes_at(),
            outcomes: view
                .outcomes
                .iter()
                .map(|o| OutcomeResponse {
                    id: o.id().as_uuid(),
                    label: o.label().to_string(),
                    price: o.current_price().as_fraction(),
                    volume: o.volume(),
                })
                .collect(),
        }
    }
}

/// A single point on an outcome's price chart.
#[derive(Debug, Serialize, ToSchema)]
struct PricePointResponse {
    price: f64,
    recorded_at: DateTime<Utc>,
}

/// Price history grouped per outcome, ready for a multi-series chart.
#[derive(Debug, Serialize, ToSchema)]
struct OutcomeHistoryResponse {
    outcome_id: Uuid,
    points: Vec<PricePointResponse>,
}

fn group_price_history(points: Vec<PricePoint>) -> Vec<OutcomeHistoryResponse> {
    // BTreeMap keeps a deterministic outcome order in the response.
    let mut by_outcome: BTreeMap<Uuid, Vec<PricePointResponse>> = BTreeMap::new();
    for point in points {
        by_outcome
            .entry(point.outcome_id().as_uuid())
            .or_default()
            .push(PricePointResponse {
                price: point.price().as_fraction(),
                recorded_at: point.recorded_at(),
            });
    }
    by_outcome
        .into_iter()
        .map(|(outcome_id, points)| OutcomeHistoryResponse { outcome_id, points })
        .collect()
}

// --- Request DTOs ---

/// Query string for `GET /api/markets`.
#[derive(Debug, Deserialize)]
struct ListQuery {
    sort: Option<String>,
    category: Option<String>,
    status: Option<String>,
    search: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

impl ListQuery {
    fn into_filter(self) -> Result<MarketFilter, ApplicationError> {
        let sort = match self.sort.as_deref() {
            None | Some("popular") => MarketSort::Popular,
            Some("newest") => MarketSort::Newest,
            Some("volume") => MarketSort::Volume,
            Some("closing_soon") => MarketSort::ClosingSoon,
            Some(other) => return Err(invalid(format!("unknown sort: {other}"))),
        };
        let status = self
            .status
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(ApplicationError::from)?;

        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let page = self.page.unwrap_or(1).max(1);
        Ok(MarketFilter {
            sort,
            category: self.category.filter(|c| !c.is_empty()),
            status,
            search: self.search.filter(|s| !s.is_empty()),
            limit,
            offset: (page - 1) * limit,
        })
    }
}

/// Query string for `GET /api/markets/{id}/price-history`.
#[derive(Debug, Deserialize)]
struct PriceHistoryParams {
    interval: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

impl PriceHistoryParams {
    fn into_query(self) -> Result<PriceHistoryQuery, ApplicationError> {
        let interval = match self.interval.as_deref() {
            None | Some("1m") | Some("minute") => PriceInterval::Minute,
            Some("1h") | Some("hour") => PriceInterval::Hour,
            Some("1d") | Some("day") => PriceInterval::Day,
            Some(other) => return Err(invalid(format!("unknown interval: {other}"))),
        };
        Ok(PriceHistoryQuery {
            interval,
            from: self.from,
            to: self.to,
        })
    }
}

/// Body for `POST /api/markets`.
#[derive(Debug, Deserialize, ToSchema)]
struct CreateMarketRequest {
    title: String,
    description: Option<String>,
    category: Option<String>,
    closes_at: Option<DateTime<Utc>>,
    /// Outcome labels; at least two are required.
    outcomes: Vec<String>,
}

/// Body for `POST /api/markets/{id}/resolve`.
#[derive(Debug, Deserialize, ToSchema)]
struct ResolveRequest {
    winning_outcome_id: Uuid,
}

fn invalid(message: String) -> ApplicationError {
    ApplicationError::Domain(DomainError::Validation(message))
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/",
    tag = "markets",
    params(
        ("sort" = Option<String>, Query, description = "popular | newest | volume | closing_soon"),
        ("category" = Option<String>, Query, description = "Filter by category"),
        ("status" = Option<String>, Query, description = "open | closed | resolved"),
        ("search" = Option<String>, Query, description = "Case-insensitive title search"),
        ("page" = Option<i64>, Query, description = "1-based page number"),
        ("limit" = Option<i64>, Query, description = "Page size (max 100, default 20)"),
    ),
    responses(
        (status = 200, description = "Matching markets", body = [MarketResponse]),
        (status = 422, description = "Invalid filter", body = ErrorResponse),
    )
)]
async fn get_markets(
    State(state): State<MarketState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<MarketResponse>>, ApiError> {
    let filter = query.into_filter()?;
    let views = state.list.execute(&filter).await?;
    Ok(Json(views.iter().map(MarketResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/featured",
    tag = "markets",
    responses(
        (status = 200, description = "The most popular market right now", body = MarketResponse),
        (status = 404, description = "No markets exist yet", body = ErrorResponse),
    )
)]
async fn get_featured(State(state): State<MarketState>) -> Result<Json<MarketResponse>, ApiError> {
    let view = state.featured.execute().await?;
    Ok(Json(MarketResponse::from(&view)))
}

#[utoipa::path(
    get,
    path = "/{id}",
    tag = "markets",
    params(("id" = Uuid, Path, description = "Market id")),
    responses(
        (status = 200, description = "Market with outcomes", body = MarketResponse),
        (status = 404, description = "Market not found", body = ErrorResponse),
    )
)]
async fn get_market(
    State(state): State<MarketState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MarketResponse>, ApiError> {
    let view = state.get.execute(MarketId::from(id)).await?;
    Ok(Json(MarketResponse::from(&view)))
}

#[utoipa::path(
    get,
    path = "/{id}/price-history",
    tag = "markets",
    params(
        ("id" = Uuid, Path, description = "Market id"),
        ("interval" = Option<String>, Query, description = "1m | 1h | 1d (default 1m)"),
        ("from" = Option<String>, Query, description = "RFC3339 lower bound"),
        ("to" = Option<String>, Query, description = "RFC3339 upper bound"),
    ),
    responses(
        (status = 200, description = "Price points grouped per outcome", body = [OutcomeHistoryResponse]),
        (status = 404, description = "Market not found", body = ErrorResponse),
    )
)]
async fn get_price_history(
    State(state): State<MarketState>,
    Path(id): Path<Uuid>,
    Query(params): Query<PriceHistoryParams>,
) -> Result<Json<Vec<OutcomeHistoryResponse>>, ApiError> {
    let query = params.into_query()?;
    let points = state
        .price_history
        .execute(MarketId::from(id), &query)
        .await?;
    Ok(Json(group_price_history(points)))
}

#[utoipa::path(
    post,
    path = "/",
    tag = "markets",
    security(("bearer_auth" = [])),
    request_body = CreateMarketRequest,
    responses(
        (status = 201, description = "Created market", body = MarketResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
        (status = 422, description = "Invalid market definition", body = ErrorResponse),
    )
)]
async fn create_market(
    State(state): State<MarketState>,
    CurrentUser(claims): CurrentUser,
    Json(body): Json<CreateMarketRequest>,
) -> Result<(StatusCode, Json<MarketResponse>), ApiError> {
    let view = state
        .create
        .execute(
            &Actor::from(claims),
            NewMarket {
                title: body.title,
                description: body.description,
                category: body.category,
                closes_at: body.closes_at,
                outcomes: body.outcomes,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(MarketResponse::from(&view))))
}

#[utoipa::path(
    post,
    path = "/{id}/resolve",
    tag = "markets",
    security(("bearer_auth" = [])),
    params(("id" = Uuid, Path, description = "Market id")),
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Resolved market", body = MarketResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
        (status = 404, description = "Market not found", body = ErrorResponse),
        (status = 422, description = "Outcome does not belong to the market, or already resolved", body = ErrorResponse),
    )
)]
async fn resolve_market(
    State(state): State<MarketState>,
    CurrentUser(claims): CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveRequest>,
) -> Result<Json<MarketResponse>, ApiError> {
    let view = state
        .resolve
        .execute(
            &Actor::from(claims),
            MarketId::from(id),
            OutcomeId::from(body.winning_outcome_id),
        )
        .await?;
    Ok(Json(MarketResponse::from(&view)))
}
