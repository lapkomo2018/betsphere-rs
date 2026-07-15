use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::{Bet, BetStatus, Market, MarketId, Outcome, PricePoint, UserId};
use domain::repositories::{
    BetFilter, BetRepository, BetSort, MarketRepository, RepositoryError, UserRepository, UserStats,
};

use super::{InMemoryMarketRepository, InMemoryUserRepository};

/// Thread-safe in-memory bet store. Placement and settlement mutate the
/// sibling in-memory user and market stores the way the Postgres
/// implementation mutates its tables (without real transactionality, which
/// tests don't need). Useful for development and tests.
pub struct InMemoryBetRepository {
    bets: RwLock<Vec<Bet>>,
    markets: Arc<InMemoryMarketRepository>,
    users: Arc<InMemoryUserRepository>,
}

impl InMemoryBetRepository {
    pub fn new(markets: Arc<InMemoryMarketRepository>, users: Arc<InMemoryUserRepository>) -> Self {
        Self {
            bets: RwLock::new(Vec::new()),
            markets,
            users,
        }
    }

    async fn filtered(
        &self,
        keep: impl Fn(&Bet) -> bool,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        let mut bets: Vec<Bet> = self
            .bets
            .read()
            .await
            .iter()
            .filter(|b| keep(b))
            .filter(|b| filter.status.is_none_or(|status| b.status() == status))
            .cloned()
            .collect();

        match filter.sort {
            BetSort::Newest => bets.sort_by_key(|b| std::cmp::Reverse(b.created_at())),
            BetSort::Popular => bets.sort_by_key(|b| {
                (
                    std::cmp::Reverse(b.amount()),
                    std::cmp::Reverse(b.created_at()),
                )
            }),
        }

        let offset = filter.offset.max(0) as usize;
        let limit = filter.limit.max(0) as usize;
        Ok(bets.into_iter().skip(offset).take(limit).collect())
    }
}

#[async_trait]
impl BetRepository for InMemoryBetRepository {
    async fn place(
        &self,
        bet: &Bet,
        priced_outcomes: &[Outcome],
        points: &[PricePoint],
    ) -> Result<(), RepositoryError> {
        let mut user = self.users.find_by_id(bet.user_id()).await?.ok_or_else(|| {
            RepositoryError::Storage(format!("bettor {} not found", bet.user_id()))
        })?;
        user.debit(bet.amount())
            .map_err(|_| RepositoryError::Conflict("balance no longer covers the stake".into()))?;
        self.users.save(&user).await?;

        let mut bets = self.bets.write().await;
        let new_participant = !bets
            .iter()
            .any(|b| b.user_id() == bet.user_id() && b.market_id() == bet.market_id());
        bets.push(bet.clone());
        drop(bets);

        self.markets
            .apply_bet(
                bet.market_id(),
                bet.amount(),
                new_participant,
                priced_outcomes,
                points,
            )
            .await;
        Ok(())
    }

    async fn active_for_market(&self, market_id: MarketId) -> Result<Vec<Bet>, RepositoryError> {
        Ok(self
            .bets
            .read()
            .await
            .iter()
            .filter(|b| b.market_id() == market_id && b.status() == BetStatus::Active)
            .cloned()
            .collect())
    }

    async fn settle(&self, market: &Market, settled: &[Bet]) -> Result<(), RepositoryError> {
        self.markets.resolve(market).await?;

        let mut bets = self.bets.write().await;
        for bet in settled {
            if let Some(slot) = bets.iter_mut().find(|b| b.id() == bet.id()) {
                *slot = bet.clone();
            }
            if let Some(payout) = bet.payout() {
                let mut user = self.users.find_by_id(bet.user_id()).await?.ok_or_else(|| {
                    RepositoryError::Storage(format!("winner {} not found", bet.user_id()))
                })?;
                user.credit(payout)
                    .map_err(|e| RepositoryError::Storage(format!("payout failed: {e}")))?;
                self.users.save(&user).await?;
            }
        }
        Ok(())
    }

    async fn find_by_user(
        &self,
        user_id: UserId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        self.filtered(|b| b.user_id() == user_id, filter).await
    }

    async fn find_by_market(
        &self,
        market_id: MarketId,
        filter: &BetFilter,
    ) -> Result<Vec<Bet>, RepositoryError> {
        self.filtered(|b| b.market_id() == market_id, filter).await
    }

    async fn feed(&self, filter: &BetFilter) -> Result<Vec<Bet>, RepositoryError> {
        self.filtered(|_| true, filter).await
    }

    async fn stats_for_user(&self, user_id: UserId) -> Result<UserStats, RepositoryError> {
        let mut stats = UserStats::default();
        for bet in self
            .bets
            .read()
            .await
            .iter()
            .filter(|b| b.user_id() == user_id)
        {
            stats.total_bets += 1;
            stats.total_volume += bet.amount();
            match bet.status() {
                BetStatus::Won => stats.wins += 1,
                BetStatus::Lost => stats.losses += 1,
                BetStatus::Active | BetStatus::Refunded => {}
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::{Market, Outcome, User};
    use domain::services::pricing;
    use domain::value_objects::market::{MarketTitle, OutcomeLabel, Price};
    use domain::value_objects::user::{Email, PasswordHash, Username};

    struct Fixture {
        repo: InMemoryBetRepository,
        markets: Arc<InMemoryMarketRepository>,
        users: Arc<InMemoryUserRepository>,
        user: User,
        market: Market,
        outcomes: Vec<Outcome>,
    }

    async fn fixture() -> Fixture {
        let markets = Arc::new(InMemoryMarketRepository::new());
        let users = Arc::new(InMemoryUserRepository::new());

        let user = User::new(
            Username::new("bettor").unwrap(),
            Email::new("bettor@example.com").unwrap(),
            PasswordHash::new("$argon2id$fake"),
        );
        users.save(&user).await.unwrap();

        let market = Market::new(MarketTitle::new("Coin flip").unwrap(), None, None, None);
        let outcomes = vec![
            Outcome::new(
                market.id(),
                OutcomeLabel::new("Heads").unwrap(),
                Price::from_ten_thousandths(5_000).unwrap(),
            ),
            Outcome::new(
                market.id(),
                OutcomeLabel::new("Tails").unwrap(),
                Price::from_ten_thousandths(5_000).unwrap(),
            ),
        ];
        markets.create(&market, &outcomes).await.unwrap();

        Fixture {
            repo: InMemoryBetRepository::new(markets.clone(), users.clone()),
            markets,
            users,
            user,
            market,
            outcomes,
        }
    }

    /// Builds the placement artifacts the way `PlaceBet` does: bet at the
    /// current price, chosen outcome's volume bumped, prices recalculated.
    fn placement(f: &Fixture, amount: i64) -> (Bet, Vec<Outcome>, Vec<PricePoint>) {
        let bet = Bet::place(
            f.user.id(),
            f.market.id(),
            f.outcomes[0].id(),
            amount,
            f.outcomes[0].current_price(),
        )
        .unwrap();
        let mut priced = f.outcomes.clone();
        priced[0].add_volume(amount);
        pricing::recalculate_prices(&mut priced);
        let points = priced
            .iter()
            .map(|o| PricePoint::new(o.id(), o.current_price()))
            .collect();
        (bet, priced, points)
    }

    #[tokio::test]
    async fn place_debits_balance_and_updates_market() {
        let f = fixture().await;
        let (bet, priced, points) = placement(&f, 1_000);
        f.repo.place(&bet, &priced, &points).await.unwrap();

        let user = f.users.find_by_id(f.user.id()).await.unwrap().unwrap();
        assert_eq!(user.balance(), User::STARTING_BALANCE - 1_000);

        let market = f.markets.find_by_id(f.market.id()).await.unwrap().unwrap();
        assert_eq!(market.total_volume(), 1_000);
        assert_eq!(market.participants_count(), 1);

        // All volume sits on "Heads", so its price is 1.0000 now.
        let outcomes = f.markets.outcomes_for(f.market.id()).await.unwrap();
        assert_eq!(outcomes[0].current_price(), Price::ONE);
        assert_eq!(outcomes[0].volume(), 1_000);
    }

    #[tokio::test]
    async fn place_rejects_stake_beyond_balance() {
        let f = fixture().await;
        let (bet, priced, points) = placement(&f, User::STARTING_BALANCE + 1);
        let err = f.repo.place(&bet, &priced, &points).await.unwrap_err();
        assert!(matches!(err, RepositoryError::Conflict(_)));
        // Nothing was applied.
        let user = f.users.find_by_id(f.user.id()).await.unwrap().unwrap();
        assert_eq!(user.balance(), User::STARTING_BALANCE);
        assert!(f.repo.feed(&BetFilter::default()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn settle_pays_winner_and_updates_statuses() {
        let f = fixture().await;
        let (bet, priced, points) = placement(&f, 1_000); // fixed at 0.5000
        f.repo.place(&bet, &priced, &points).await.unwrap();

        let mut market = f.markets.find_by_id(f.market.id()).await.unwrap().unwrap();
        market.resolve(f.outcomes[0].id()).unwrap();
        let mut settled = f.repo.active_for_market(f.market.id()).await.unwrap();
        settled[0].settle_as_winner().unwrap();
        f.repo.settle(&market, &settled).await.unwrap();

        // 1000 staked at 0.5000 pays 2000; net +1000 over the debit.
        let user = f.users.find_by_id(f.user.id()).await.unwrap().unwrap();
        assert_eq!(user.balance(), User::STARTING_BALANCE + 1_000);

        let stored = &f.repo.feed(&BetFilter::default()).await.unwrap()[0];
        assert_eq!(stored.status(), BetStatus::Won);
        assert_eq!(stored.payout(), Some(2_000));
    }

    #[tokio::test]
    async fn stats_aggregate_the_users_bets() {
        let f = fixture().await;
        let (bet, priced, points) = placement(&f, 1_000);
        f.repo.place(&bet, &priced, &points).await.unwrap();

        // One active bet: counted in totals, not yet won or lost.
        let stats = f.repo.stats_for_user(f.user.id()).await.unwrap();
        assert_eq!(
            stats,
            UserStats {
                total_bets: 1,
                wins: 0,
                losses: 0,
                total_volume: 1_000,
            }
        );
        assert_eq!(stats.win_rate(), 0.0);

        let mut market = f.markets.find_by_id(f.market.id()).await.unwrap().unwrap();
        market.resolve(f.outcomes[0].id()).unwrap();
        let mut settled = f.repo.active_for_market(f.market.id()).await.unwrap();
        settled[0].settle_as_winner().unwrap();
        f.repo.settle(&market, &settled).await.unwrap();

        let stats = f.repo.stats_for_user(f.user.id()).await.unwrap();
        assert_eq!(stats.wins, 1);
        assert_eq!(stats.win_rate(), 1.0);

        // Another user's stats stay empty.
        let stranger = f.repo.stats_for_user(UserId::new()).await.unwrap();
        assert_eq!(stranger, UserStats::default());
    }
}
