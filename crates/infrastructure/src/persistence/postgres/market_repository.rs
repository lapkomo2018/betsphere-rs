use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool, QueryBuilder};
use uuid::Uuid;

use domain::entities::{Market, MarketId, MarketStatus, Outcome, OutcomeId, PricePoint};
use domain::repositories::{
    MarketFilter, MarketRepository, MarketSort, PriceHistoryQuery, PriceInterval, RepositoryError,
};
use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};

use super::map_sqlx_err;

const MARKET_COLUMNS: &str = "id, title, description, category, thumbnail_url, status, resolved_outcome_id, \
     total_volume, participants_count, created_at, closes_at";
const OUTCOME_COLUMNS: &str = "id, market_id, label, current_price, volume";

// --- Row types ---

#[derive(sqlx::FromRow)]
struct MarketRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    category: Option<String>,
    thumbnail_url: Option<String>,
    status: String,
    resolved_outcome_id: Option<Uuid>,
    total_volume: i64,
    participants_count: i32,
    created_at: DateTime<Utc>,
    closes_at: Option<DateTime<Utc>>,
}

impl TryFrom<MarketRow> for Market {
    type Error = RepositoryError;

    fn try_from(row: MarketRow) -> Result<Self, Self::Error> {
        let corrupt = |e: domain::DomainError| {
            RepositoryError::Storage(format!("corrupt market row {}: {e}", row.id))
        };
        Ok(Market::from_parts(
            row.id.into(),
            MarketTitle::new(&row.title).map_err(corrupt)?,
            row.description,
            row.category,
            row.thumbnail_url,
            row.status.parse::<MarketStatus>().map_err(corrupt)?,
            row.resolved_outcome_id.map(Into::into),
            row.total_volume,
            row.participants_count,
            row.created_at,
            row.closes_at,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct OutcomeRow {
    id: Uuid,
    market_id: Uuid,
    label: String,
    current_price: i32,
    volume: i64,
}

impl TryFrom<OutcomeRow> for Outcome {
    type Error = RepositoryError;

    fn try_from(row: OutcomeRow) -> Result<Self, Self::Error> {
        let corrupt = |e: domain::DomainError| {
            RepositoryError::Storage(format!("corrupt outcome row {}: {e}", row.id))
        };
        Ok(Outcome::from_parts(
            row.id.into(),
            row.market_id.into(),
            OutcomeLabel::new(&row.label).map_err(corrupt)?,
            Price::from_ten_thousandths(row.current_price).map_err(corrupt)?,
            row.volume,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct PricePointRow {
    outcome_id: Uuid,
    price: i32,
    recorded_at: DateTime<Utc>,
}

impl TryFrom<PricePointRow> for PricePoint {
    type Error = RepositoryError;

    fn try_from(row: PricePointRow) -> Result<Self, Self::Error> {
        let price = Price::from_ten_thousandths(row.price).map_err(|e| {
            RepositoryError::Storage(format!(
                "corrupt price_history row for outcome {}: {e}",
                row.outcome_id
            ))
        })?;
        Ok(PricePoint::from_parts(
            row.outcome_id.into(),
            price,
            row.recorded_at,
        ))
    }
}

// --- Query helpers ---

async fn insert_market(exec: impl PgExecutor<'_>, market: &Market) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO markets
             (id, title, description, category, thumbnail_url, status, resolved_outcome_id,
              total_volume, participants_count, created_at, closes_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(market.id().as_uuid())
    .bind(market.title().as_str())
    .bind(market.description())
    .bind(market.category())
    .bind(market.thumbnail_url())
    .bind(market.status().as_str())
    .bind(market.resolved_outcome_id().map(|id| id.as_uuid()))
    .bind(market.total_volume())
    .bind(market.participants_count())
    .bind(market.created_at())
    .bind(market.closes_at())
    .execute(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

async fn insert_outcome(
    exec: impl PgExecutor<'_>,
    outcome: &Outcome,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO outcomes (id, market_id, label, current_price, volume)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(outcome.id().as_uuid())
    .bind(outcome.market_id().as_uuid())
    .bind(outcome.label().as_str())
    .bind(outcome.current_price().as_ten_thousandths())
    .bind(outcome.volume())
    .execute(exec)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

pub(super) async fn insert_price_point(
    exec: impl PgExecutor<'_>,
    point: &PricePoint,
) -> Result<(), RepositoryError> {
    sqlx::query("INSERT INTO price_history (outcome_id, price, recorded_at) VALUES ($1, $2, $3)")
        .bind(point.outcome_id().as_uuid())
        .bind(point.price().as_ten_thousandths())
        .bind(point.recorded_at())
        .execute(exec)
        .await
        .map_err(map_sqlx_err)?;
    Ok(())
}

/// Fixed, injection-safe ORDER BY clause for each sort mode.
fn order_by(sort: MarketSort) -> &'static str {
    match sort {
        MarketSort::Popular => {
            " ORDER BY (total_volume + participants_count) DESC, created_at DESC"
        }
        MarketSort::Newest => " ORDER BY created_at DESC",
        MarketSort::Volume => " ORDER BY total_volume DESC, created_at DESC",
        MarketSort::ClosingSoon => " ORDER BY closes_at ASC NULLS LAST, created_at DESC",
    }
}

/// `date_trunc` unit for a price-history interval. Fixed strings, never user input.
fn trunc_unit(interval: PriceInterval) -> &'static str {
    match interval {
        PriceInterval::Minute => "minute",
        PriceInterval::Hour => "hour",
        PriceInterval::Day => "day",
    }
}

// --- Repository ---

pub struct PgMarketRepository {
    pool: PgPool,
}

impl PgMarketRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MarketRepository for PgMarketRepository {
    async fn create(&self, market: &Market, outcomes: &[Outcome]) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;
        insert_market(&mut *tx, market).await?;
        for outcome in outcomes {
            insert_outcome(&mut *tx, outcome).await?;
            // A starting point so the chart has data from creation.
            let point = PricePoint::new(outcome.id(), outcome.current_price());
            insert_price_point(&mut *tx, &point).await?;
        }
        tx.commit().await.map_err(map_sqlx_err)
    }

    async fn find_by_id(&self, id: MarketId) -> Result<Option<Market>, RepositoryError> {
        let query = format!("SELECT {MARKET_COLUMNS} FROM markets WHERE id = $1");
        let row = sqlx::query_as::<_, MarketRow>(&query)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        row.map(Market::try_from).transpose()
    }

    async fn find_by_ids(&self, ids: &[MarketId]) -> Result<Vec<Market>, RepositoryError> {
        let ids: Vec<Uuid> = ids.iter().map(|id| id.as_uuid()).collect();
        let query = format!("SELECT {MARKET_COLUMNS} FROM markets WHERE id = ANY($1)");
        let rows = sqlx::query_as::<_, MarketRow>(&query)
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Market::try_from).collect()
    }

    async fn outcomes_for(&self, market_id: MarketId) -> Result<Vec<Outcome>, RepositoryError> {
        let query =
            format!("SELECT {OUTCOME_COLUMNS} FROM outcomes WHERE market_id = $1 ORDER BY seq");
        let rows = sqlx::query_as::<_, OutcomeRow>(&query)
            .bind(market_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Outcome::try_from).collect()
    }

    async fn outcome_by_id(
        &self,
        outcome_id: OutcomeId,
    ) -> Result<Option<Outcome>, RepositoryError> {
        let query = format!("SELECT {OUTCOME_COLUMNS} FROM outcomes WHERE id = $1");
        let row = sqlx::query_as::<_, OutcomeRow>(&query)
            .bind(outcome_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        row.map(Outcome::try_from).transpose()
    }

    async fn outcomes_for_markets(
        &self,
        market_ids: &[MarketId],
    ) -> Result<Vec<Outcome>, RepositoryError> {
        let ids: Vec<Uuid> = market_ids.iter().map(|id| id.as_uuid()).collect();
        let query = format!(
            "SELECT {OUTCOME_COLUMNS} FROM outcomes WHERE market_id = ANY($1) ORDER BY market_id, seq"
        );
        let rows = sqlx::query_as::<_, OutcomeRow>(&query)
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Outcome::try_from).collect()
    }

    async fn list(&self, filter: &MarketFilter) -> Result<Vec<Market>, RepositoryError> {
        let mut qb: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new(format!("SELECT {MARKET_COLUMNS} FROM markets WHERE TRUE"));
        if let Some(status) = filter.status {
            qb.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(category) = &filter.category {
            qb.push(" AND category = ").push_bind(category.clone());
        }
        if let Some(search) = &filter.search {
            qb.push(" AND title ILIKE ")
                .push_bind(format!("%{search}%"));
        }
        qb.push(order_by(filter.sort));
        qb.push(" LIMIT ").push_bind(filter.limit);
        qb.push(" OFFSET ").push_bind(filter.offset);

        let rows = qb
            .build_query_as::<MarketRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Market::try_from).collect()
    }

    async fn featured(&self) -> Result<Option<Market>, RepositoryError> {
        let query = format!(
            "SELECT {MARKET_COLUMNS} FROM markets
             WHERE status = 'open'
             ORDER BY (total_volume + participants_count) DESC, created_at DESC
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, MarketRow>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        row.map(Market::try_from).transpose()
    }

    async fn resolve(&self, market: &Market) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE markets SET status = $1, resolved_outcome_id = $2 WHERE id = $3")
            .bind(market.status().as_str())
            .bind(market.resolved_outcome_id().map(|id| id.as_uuid()))
            .bind(market.id().as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn update_thumbnail(&self, market: &Market) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE markets SET thumbnail_url = $1 WHERE id = $2")
            .bind(market.thumbnail_url())
            .bind(market.id().as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn price_history(
        &self,
        market_id: MarketId,
        query: &PriceHistoryQuery,
    ) -> Result<Vec<PricePoint>, RepositoryError> {
        // Keep the last recorded price in each time bucket per outcome, so the
        // chart shows one point per interval step rather than every raw tick.
        let unit = trunc_unit(query.interval);
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT ph.outcome_id, ph.price, ph.recorded_at FROM (
                 SELECT DISTINCT ON (ph.outcome_id, date_trunc('",
        );
        qb.push(unit); // fixed literal from the interval enum
        qb.push(
            "', ph.recorded_at))
                     ph.outcome_id, ph.price, ph.recorded_at
                 FROM price_history ph
                 JOIN outcomes o ON o.id = ph.outcome_id
                 WHERE o.market_id = ",
        );
        qb.push_bind(market_id.as_uuid());
        if let Some(from) = query.from {
            qb.push(" AND ph.recorded_at >= ").push_bind(from);
        }
        if let Some(to) = query.to {
            qb.push(" AND ph.recorded_at <= ").push_bind(to);
        }
        qb.push(" ORDER BY ph.outcome_id, date_trunc('");
        qb.push(unit);
        qb.push(
            "', ph.recorded_at), ph.recorded_at DESC
             ) ph
             ORDER BY ph.recorded_at ASC",
        );

        let rows = qb
            .build_query_as::<PricePointRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(PricePoint::try_from).collect()
    }
}
