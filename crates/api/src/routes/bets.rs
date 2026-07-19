use application::use_cases::bet::BetView;
use application::{Actor, ApplicationError};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::entities::OutcomeId;
use domain::repositories::{BetFilter, BetSort};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ErrorResponse};
use crate::extract::CurrentUser;
use crate::state::{AppState, BetState};

/// Hard cap on page size, matching the non-functional requirements.
const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 20;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(place_bet))
        .routes(routes!(get_feed))
}

// --- DTOs (shared with the market/user bet listings) ---

/// One bet with the display names a feed entry needs. `price` is the price
/// fixed at placement, as a fraction in `[0.0, 1.0]`.
#[derive(Debug, Serialize, ToSchema)]
pub(super) struct BetResponse {
    id: Uuid,
    user_id: Uuid,
    username: String,
    market_id: Uuid,
    market_title: String,
    outcome_id: Uuid,
    outcome_label: String,
    amount: i64,
    price: f64,
    status: String,
    payout: Option<i64>,
    created_at: DateTime<Utc>,
}

impl From<&BetView> for BetResponse {
    fn from(view: &BetView) -> Self {
        let bet = &view.bet;
        Self {
            id: bet.id().as_uuid(),
            user_id: bet.user_id().as_uuid(),
            username: view.username.clone(),
            market_id: bet.market_id().as_uuid(),
            market_title: view.market_title.clone(),
            outcome_id: bet.outcome_id().as_uuid(),
            outcome_label: view.outcome_label.clone(),
            amount: bet.amount(),
            price: bet.price().as_fraction(),
            status: bet.status().to_string(),
            payout: bet.payout(),
            created_at: bet.created_at(),
        }
    }
}

pub(super) fn to_responses(views: &[BetView]) -> Vec<BetResponse> {
    views.iter().map(BetResponse::from).collect()
}

/// Query string shared by every bet listing.
#[derive(Debug, Deserialize)]
pub(super) struct BetListQuery {
    sort: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

impl BetListQuery {
    pub(super) fn into_filter(self) -> Result<BetFilter, ApplicationError> {
        let sort = match self.sort.as_deref() {
            None | Some("newest") => BetSort::Newest,
            Some("popular") => BetSort::Popular,
            Some(other) => {
                return Err(DomainError::Validation(format!("unknown sort: {other}")).into());
            }
        };
        let status = self
            .status
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .transpose()
            .map_err(ApplicationError::from)?;

        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let page = self.page.unwrap_or(1).max(1);
        Ok(BetFilter {
            status,
            sort,
            limit,
            offset: (page - 1) * limit,
        })
    }
}

/// Body for `POST /api/bets`.
#[derive(Debug, Deserialize, ToSchema)]
struct PlaceBetRequest {
    outcome_id: Uuid,
    /// Stake in minimal currency units; must be positive.
    amount: i64,
}

// --- Handlers ---

#[utoipa::path(
    post,
    path = "/",
    tag = "bets",
    security(("bearer_auth" = [])),
    request_body = PlaceBetRequest,
    responses(
        (status = 201, description = "Placed bet", body = BetResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Market not found", body = ErrorResponse),
        (status = 409, description = "Balance changed concurrently; retry", body = ErrorResponse),
        (status = 422, description = "Market closed, outcome not in market, bad amount, or insufficient balance", body = ErrorResponse),
    )
)]
async fn place_bet(
    State(state): State<BetState>,
    CurrentUser(claims): CurrentUser,
    Json(body): Json<PlaceBetRequest>,
) -> Result<(StatusCode, Json<BetResponse>), ApiError> {
    let view = state
        .place
        .execute(
            &Actor::from(claims),
            application::use_cases::bet::NewBet {
                outcome_id: OutcomeId::from(body.outcome_id),
                amount: body.amount,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(BetResponse::from(&view))))
}

#[utoipa::path(
    get,
    path = "/feed",
    tag = "bets",
    params(
        ("sort" = Option<String>, Query, description = "newest | popular (biggest stakes)"),
        ("status" = Option<String>, Query, description = "active | won | lost | refunded"),
        ("page" = Option<i64>, Query, description = "1-based page number"),
        ("limit" = Option<i64>, Query, description = "Page size (max 100, default 20)"),
    ),
    responses(
        (status = 200, description = "Cross-market bet feed", body = [BetResponse]),
        (status = 422, description = "Invalid filter", body = ErrorResponse),
    )
)]
async fn get_feed(
    State(state): State<BetState>,
    Query(query): Query<BetListQuery>,
) -> Result<Json<Vec<BetResponse>>, ApiError> {
    let filter = query.into_filter()?;
    let views = state.feed.execute(&filter).await?;
    Ok(Json(to_responses(&views)))
}
