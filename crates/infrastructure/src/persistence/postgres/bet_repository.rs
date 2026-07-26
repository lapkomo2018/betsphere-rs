use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

use domain::entities::{
    Bet, BetId, BetStatus, Market, MarketId, Outcome, OutcomeId, PricePoint, UserId,
};
use domain::events::{BetPlaced, MarketPricesUpdated, UserBalanceChanged};
use domain::repositories::{
    ActivePosition, BetFilter, BetRepository, BetSort, RepositoryError, UserStats,
};
use domain::value_objects::market::Price;

use super::map_sqlx_err;
use super::market_repository::insert_price_point;
use crate::events::publish;

const BET_COLUMNS: &str =
    "id, user_id, market_id, outcome_id, amount, price, status, payout, created_at";

#[derive(sqlx::FromRow)]
struct BetRow {
    id: Uuid,
    user_id: Uuid,
    market_id: Uuid,
    outcome_id: Uuid,
    amount: i64,
    price: i32,
    status: String,
    payout: Option<i64>,
    created_at: DateTime<Utc>,
}

impl TryFrom<BetRow> for Bet {
    type Error = RepositoryError;

    fn try_from(row: BetRow) -> Result<Self, Self::Error> {
        let corrupt = |e: domain::DomainError| {
            RepositoryError::Storage(format!("corrupt bet row {}: {e}", row.id))
        };
        Ok(Bet::from_parts(
            row.id.into(),
            row.user_id.into(),
            row.market_id.into(),
            row.outcome_id.into(),
            row.amount,
            Price::from_ten_thousandths(row.price).map_err(corrupt)?,
            row.status.parse::<BetStatus>().map_err(corrupt)?,
            row.payout,
            row.created_at,
        ))
    }
}

/// Fixed, injection-safe ORDER BY clause for each sort mode.
fn order_by(sort: BetSort) -> &'static str {
    match sort {
        BetSort::Newest => " ORDER BY created_at DESC",
        BetSort::Popular => " ORDER BY amount DESC, created_at DESC",
    }
}

pub struct PgBetRepository {
    pool: PgPool,
}

impl PgBetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Shared SELECT for the listing endpoints. `condition` is a fixed column
    /// name paired with its bound id — never user input.
    async fn list(
        &self,
        condition: Option<(&str, Uuid)>,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        let mut qb: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new(format!("SELECT {BET_COLUMNS} FROM bets WHERE TRUE"));
        if let Some((column, id)) = condition {
            qb.push(format!(" AND {column} = ")).push_bind(id);
        }
        if let Some(status) = filter.status {
            qb.push(" AND status = ").push_bind(status.as_str());
        }
        qb.push(order_by(filter.sort));
        qb.push(" LIMIT ").push_bind(filter.limit);
        qb.push(" OFFSET ").push_bind(filter.offset);

        let rows = qb
            .build_query_as::<BetRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Bet::try_from).collect()
    }
}

#[async_trait]
impl BetRepository for PgBetRepository {
    async fn place(
        &self,
        bet: &Bet,
        priced_outcomes: &[Outcome],
        points: &[PricePoint],
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

        // Conditional debit: zero rows means the balance no longer covers the
        // stake (a concurrent spend won the race), and the transaction rolls
        // back. This is what keeps balances from ever going negative.
        let debited = sqlx::query(
            "UPDATE users SET balance = balance - $1, updated_at = now()
             WHERE id = $2 AND balance >= $1",
        )
        .bind(bet.amount())
        .bind(bet.user_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;
        if debited.rows_affected() == 0 {
            return Err(RepositoryError::Conflict(
                "balance no longer covers the stake".into(),
            ));
        }

        let repeat_bettor: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM bets WHERE user_id = $1 AND market_id = $2)",
        )
        .bind(bet.user_id().as_uuid())
        .bind(bet.market_id().as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

        sqlx::query(
            "INSERT INTO bets (id, user_id, market_id, outcome_id, amount, price, status, payout, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
            .bind(bet.id().as_uuid())
            .bind(bet.user_id().as_uuid())
            .bind(bet.market_id().as_uuid())
            .bind(bet.outcome_id().as_uuid())
            .bind(bet.amount())
            .bind(bet.price().as_ten_thousandths())
            .bind(bet.status().as_str())
            .bind(bet.payout())
            .bind(bet.created_at())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_err)?;

        // Volumes are applied relatively so concurrent bets on one market
        // both land. Prices are last-writer-wins snapshots: under a rare
        // concurrent write one snapshot may briefly lag, and the next bet's
        // recalculation corrects it.
        for outcome in priced_outcomes {
            let staked = if outcome.id() == bet.outcome_id() {
                bet.amount()
            } else {
                0
            };
            sqlx::query(
                "UPDATE outcomes SET volume = volume + $2, current_price = $3 WHERE id = $1",
            )
            .bind(outcome.id().as_uuid())
            .bind(staked)
            .bind(outcome.current_price().as_ten_thousandths())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_err)?;
        }

        sqlx::query(
            "UPDATE markets
             SET total_volume = total_volume + $2, participants_count = participants_count + $3
             WHERE id = $1",
        )
        .bind(bet.market_id().as_uuid())
        .bind(bet.amount())
        .bind(if repeat_bettor { 0i32 } else { 1 })
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

        for point in points {
            insert_price_point(&mut *tx, point).await?;
        }

        // Committed together with the debit, so subscribers (e.g. the user
        // cache) hear about the balance change exactly when it becomes real.
        publish(
            &mut *tx,
            &UserBalanceChanged {
                user_id: bet.user_id(),
            },
        )
        .await?;

        // Likewise for the newly committed bet, which powers live bet feeds.
        publish(&mut *tx, &BetPlaced { bet_id: bet.id() }).await?;

        // Likewise for the price move, which live market feeds broadcast.
        publish(
            &mut *tx,
            &MarketPricesUpdated {
                market_id: bet.market_id(),
            },
        )
        .await?;

        tx.commit().await.map_err(map_sqlx_err)
    }

    async fn active_for_market(&self, market_id: MarketId) -> Result<Vec<Bet>, RepositoryError> {
        let query = format!("SELECT {BET_COLUMNS} FROM bets WHERE market_id = $1 AND status = $2");
        let rows = sqlx::query_as::<_, BetRow>(&query)
            .bind(market_id.as_uuid())
            .bind(BetStatus::Active.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        rows.into_iter().map(Bet::try_from).collect()
    }

    async fn settle(&self, market: &Market, settled: &[Bet]) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

        sqlx::query("UPDATE markets SET status = $1, resolved_outcome_id = $2 WHERE id = $3")
            .bind(market.status().as_str())
            .bind(market.resolved_outcome_id().map(|id| id.as_uuid()))
            .bind(market.id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_err)?;

        for bet in settled {
            sqlx::query("UPDATE bets SET status = $1, payout = $2 WHERE id = $3")
                .bind(bet.status().as_str())
                .bind(bet.payout())
                .bind(bet.id().as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_err)?;

            if let Some(payout) = bet.payout() {
                sqlx::query(
                    "UPDATE users SET balance = balance + $1, updated_at = now() WHERE id = $2",
                )
                .bind(payout)
                .bind(bet.user_id().as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_err)?;
            }
        }

        // One event per paid user (a user may hold several winning bets),
        // committed atomically with the credits.
        let paid: std::collections::HashSet<UserId> = settled
            .iter()
            .filter(|b| b.payout().is_some())
            .map(|b| b.user_id())
            .collect();
        for user_id in paid {
            publish(&mut *tx, &UserBalanceChanged { user_id }).await?;
        }

        tx.commit().await.map_err(map_sqlx_err)
    }

    async fn find_by_id(&self, id: BetId) -> Result<Option<Bet>, RepositoryError> {
        let query = format!("SELECT {BET_COLUMNS} FROM bets WHERE id = $1");
        sqlx::query_as::<_, BetRow>(&query)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?
            .map(Bet::try_from)
            .transpose()
    }

    async fn find_by_user(
        &self,
        user_id: UserId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        self.list(Some(("user_id", user_id.as_uuid())), filter)
            .await
    }

    async fn find_by_market(
        &self,
        market_id: MarketId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        self.list(Some(("market_id", market_id.as_uuid())), filter)
            .await
    }

    async fn feed(&self, filter: &BetFilter) -> Result<Vec<Bet>, RepositoryError> {
        self.list(None, filter).await
    }

    async fn stats_for_user(&self, user_id: UserId) -> Result<UserStats, RepositoryError> {
        // SUM(BIGINT) yields NUMERIC in Postgres, hence the cast back.
        let (total_bets, wins, losses, total_volume): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),
                    COUNT(*) FILTER (WHERE status = $2),
                    COUNT(*) FILTER (WHERE status = $3),
                    COALESCE(SUM(amount), 0)::BIGINT
             FROM bets WHERE user_id = $1",
        )
        .bind(user_id.as_uuid())
        .bind(BetStatus::Won.as_str())
        .bind(BetStatus::Lost.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(UserStats {
            total_bets,
            wins,
            losses,
            total_volume,
        })
    }

    async fn active_positions(
        &self,
        pairs: &[(UserId, OutcomeId)],
    ) -> Result<Vec<ActivePosition>, RepositoryError> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let (user_ids, outcome_ids): (Vec<Uuid>, Vec<Uuid>) = pairs
            .iter()
            .map(|(user_id, outcome_id)| (user_id.as_uuid(), outcome_id.as_uuid()))
            .unzip();

        // SUM over BIGINT yields NUMERIC, which keeps the weighted mean exact
        // before it is floored back onto the ten-thousandth grid. Both factors
        // are positive, so flooring can never drop below the minimum tick.
        let rows: Vec<(Uuid, Uuid, i32)> = sqlx::query_as(
            "SELECT user_id, outcome_id,
                    FLOOR(SUM(amount::NUMERIC * price) / SUM(amount))::INTEGER
             FROM bets
             WHERE status = $3
               AND (user_id, outcome_id) IN (SELECT * FROM UNNEST($1::UUID[], $2::UUID[]))
             GROUP BY user_id, outcome_id",
        )
        .bind(&user_ids)
        .bind(&outcome_ids)
        .bind(BetStatus::Active.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        rows.into_iter()
            .map(|(user_id, outcome_id, avg_price)| {
                Ok(ActivePosition {
                    user_id: user_id.into(),
                    outcome_id: outcome_id.into(),
                    avg_price: Price::from_ten_thousandths(avg_price).map_err(|e| {
                        RepositoryError::Storage(format!(
                            "corrupt average price for user {user_id} on outcome {outcome_id}: {e}"
                        ))
                    })?,
                })
            })
            .collect()
    }
}
