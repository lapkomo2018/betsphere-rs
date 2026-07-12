use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use domain::entities::{User, UserId};
use domain::repositories::{RepositoryError, UserRepository};
use domain::value_objects::user::Email;

/// Thread-safe in-memory user store. Useful for development and tests.
#[derive(Default)]
pub struct InMemoryUserRepository {
    users: RwLock<HashMap<UserId, User>>,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn save(&self, user: &User) -> Result<(), RepositoryError> {
        self.users.write().await.insert(user.id(), user.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        Ok(self.users.read().await.get(&id).cloned())
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepositoryError> {
        Ok(self
            .users
            .read()
            .await
            .values()
            .find(|u| u.email() == email)
            .cloned())
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        let mut users: Vec<User> = self.users.read().await.values().cloned().collect();
        users.sort_by_key(|u| u.created_at());
        Ok(users)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::value_objects::user::Username;

    fn sample_user() -> User {
        User::new(
            Username::new("alice").unwrap(),
            Email::new("alice@example.com").unwrap(),
        )
    }

    #[tokio::test]
    async fn saves_and_finds_by_id() {
        let repo = InMemoryUserRepository::new();
        let user = sample_user();
        repo.save(&user).await.unwrap();

        let found = repo.find_by_id(user.id()).await.unwrap().unwrap();
        assert_eq!(found.id(), user.id());
    }

    #[tokio::test]
    async fn finds_by_email() {
        let repo = InMemoryUserRepository::new();
        let user = sample_user();
        repo.save(&user).await.unwrap();

        let found = repo.find_by_email(user.email()).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn missing_user_returns_none() {
        let repo = InMemoryUserRepository::new();
        assert!(repo.find_by_id(UserId::new()).await.unwrap().is_none());
    }
}
