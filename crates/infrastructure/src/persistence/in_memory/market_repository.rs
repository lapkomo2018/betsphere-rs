use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::{Market, MarketId, MarketStatus, Outcome, PricePoint};
use domain::repositories::{
    MarketFilter, MarketRepository, MarketSort, PriceHistoryQuery, RepositoryError,
};

/// Thread-safe in-memory market store. Filtering, sorting, and pagination are
/// applied in Rust. Useful for development and tests.
#[derive(Default)]
pub struct InMemoryMarketRepository {
    markets: RwLock<Vec<Market>>,
    outcomes: RwLock<Vec<Outcome>>,
    price_points: RwLock<Vec<PricePoint>>,
}

impl InMemoryMarketRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Combined activity score used to rank "popular" markets.
    fn popularity(market: &Market) -> i64 {
        market.total_volume() + i64::from(market.participants_count())
    }

    /// Applies a placed bet's effects: outcome volumes and prices, the
    /// market's aggregates, and the new price points. Test-double counterpart
    /// of the SQL updates in `PgBetRepository::place`.
    pub(crate) async fn apply_bet(
        &self,
        market_id: MarketId,
        amount: i64,
        new_participant: bool,
        priced_outcomes: &[Outcome],
        points: &[PricePoint],
    ) {
        if let Some(market) = self
            .markets
            .write()
            .await
            .iter_mut()
            .find(|m| m.id() == market_id)
        {
            market.record_stake(amount, new_participant);
        }
        let mut outcomes = self.outcomes.write().await;
        for priced in priced_outcomes {
            if let Some(slot) = outcomes.iter_mut().find(|o| o.id() == priced.id()) {
                *slot = priced.clone();
            }
        }
        self.price_points.write().await.extend_from_slice(points);
    }
}

#[async_trait]
impl MarketRepository for InMemoryMarketRepository {
    async fn create(&self, market: &Market, outcomes: &[Outcome]) -> Result<(), RepositoryError> {
        self.markets.write().await.push(market.clone());
        let mut stored_outcomes = self.outcomes.write().await;
        let mut points = self.price_points.write().await;
        for outcome in outcomes {
            stored_outcomes.push(outcome.clone());
            points.push(PricePoint::new(outcome.id(), outcome.current_price()));
        }
        Ok(())
    }

    async fn find_by_id(&self, id: MarketId) -> Result<Option<Market>, RepositoryError> {
        Ok(self
            .markets
            .read()
            .await
            .iter()
            .find(|m| m.id() == id)
            .cloned())
    }

    async fn find_by_ids(&self, ids: &[MarketId]) -> Result<Vec<Market>, RepositoryError> {
        Ok(self
            .markets
            .read()
            .await
            .iter()
            .filter(|m| ids.contains(&m.id()))
            .cloned()
            .collect())
    }

    async fn outcomes_for(&self, market_id: MarketId) -> Result<Vec<Outcome>, RepositoryError> {
        Ok(self
            .outcomes
            .read()
            .await
            .iter()
            .filter(|o| o.market_id() == market_id)
            .cloned()
            .collect())
    }

    async fn outcomes_for_markets(
        &self,
        market_ids: &[MarketId],
    ) -> Result<Vec<Outcome>, RepositoryError> {
        Ok(self
            .outcomes
            .read()
            .await
            .iter()
            .filter(|o| market_ids.contains(&o.market_id()))
            .cloned()
            .collect())
    }

    async fn list(&self, filter: &MarketFilter) -> Result<Vec<Market>, RepositoryError> {
        let markets = self.markets.read().await;
        let mut filtered: Vec<Market> = markets
            .iter()
            .filter(|m| {
                filter.status.is_none_or(|status| m.status() == status)
                    && filter
                        .category
                        .as_deref()
                        .is_none_or(|c| m.category() == Some(c))
                    && filter.search.as_deref().is_none_or(|q| {
                        m.title()
                            .as_str()
                            .to_lowercase()
                            .contains(&q.to_lowercase())
                    })
            })
            .cloned()
            .collect();

        match filter.sort {
            MarketSort::Popular => {
                filtered.sort_by(|a, b| Self::popularity(b).cmp(&Self::popularity(a)))
            }
            MarketSort::Newest => filtered.sort_by_key(|m| std::cmp::Reverse(m.created_at())),
            MarketSort::Volume => filtered.sort_by_key(|m| std::cmp::Reverse(m.total_volume())),
            MarketSort::ClosingSoon => {
                filtered.sort_by(|a, b| match (a.closes_at(), b.closes_at()) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
            }
        }

        let offset = filter.offset.max(0) as usize;
        let limit = filter.limit.max(0) as usize;
        Ok(filtered.into_iter().skip(offset).take(limit).collect())
    }

    async fn featured(&self) -> Result<Option<Market>, RepositoryError> {
        Ok(self
            .markets
            .read()
            .await
            .iter()
            .filter(|m| m.status() == MarketStatus::Open)
            .max_by_key(|m| Self::popularity(m))
            .cloned())
    }

    async fn resolve(&self, market: &Market) -> Result<(), RepositoryError> {
        let mut markets = self.markets.write().await;
        if let Some(slot) = markets.iter_mut().find(|m| m.id() == market.id()) {
            *slot = market.clone();
        }
        Ok(())
    }

    async fn price_history(
        &self,
        market_id: MarketId,
        query: &PriceHistoryQuery,
    ) -> Result<Vec<PricePoint>, RepositoryError> {
        let outcome_ids: Vec<_> = self
            .outcomes
            .read()
            .await
            .iter()
            .filter(|o| o.market_id() == market_id)
            .map(|o| o.id())
            .collect();

        let mut points: Vec<PricePoint> = self
            .price_points
            .read()
            .await
            .iter()
            .filter(|p| outcome_ids.contains(&p.outcome_id()))
            .filter(|p| query.from.is_none_or(|from| p.recorded_at() >= from))
            .filter(|p| query.to.is_none_or(|to| p.recorded_at() <= to))
            .cloned()
            .collect();
        points.sort_by_key(|p| p.recorded_at());
        Ok(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::Market;
    use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};

    fn market(title: &str) -> (Market, Vec<Outcome>) {
        let market = Market::new(
            MarketTitle::new(title).unwrap(),
            None,
            Some("sports".into()),
            None,
        );
        let outcomes = vec![
            Outcome::new(
                market.id(),
                OutcomeLabel::new("Yes").unwrap(),
                Price::from_ten_thousandths(5000).unwrap(),
            ),
            Outcome::new(
                market.id(),
                OutcomeLabel::new("No").unwrap(),
                Price::from_ten_thousandths(5000).unwrap(),
            ),
        ];
        (market, outcomes)
    }

    #[tokio::test]
    async fn create_stores_market_outcomes_and_initial_points() {
        let repo = InMemoryMarketRepository::new();
        let (m, outcomes) = market("Match A");
        repo.create(&m, &outcomes).await.unwrap();

        let found = repo.find_by_id(m.id()).await.unwrap().unwrap();
        assert_eq!(found.title().as_str(), "Match A");
        assert_eq!(repo.outcomes_for(m.id()).await.unwrap().len(), 2);

        let history = repo
            .price_history(m.id(), &PriceHistoryQuery::default())
            .await
            .unwrap();
        assert_eq!(history.len(), 2); // one starting point per outcome
    }

    #[tokio::test]
    async fn list_filters_by_search_and_paginates() {
        let repo = InMemoryMarketRepository::new();
        for title in ["Alpha match", "Beta match", "Gamma game"] {
            let (m, o) = market(title);
            repo.create(&m, &o).await.unwrap();
        }

        let filter = MarketFilter {
            search: Some("match".into()),
            limit: 1,
            ..Default::default()
        };
        let page = repo.list(&filter).await.unwrap();
        assert_eq!(page.len(), 1);
    }
}
