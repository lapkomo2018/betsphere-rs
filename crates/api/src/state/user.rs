use std::sync::Arc;

use application::ports::FileStorage;
use application::use_cases::user::{GetUser, GetUserStats, UploadAvatar};
use domain::repositories::{BetRepository, UserRepository};

#[derive(Clone)]
pub struct UserState {
    pub get_user: Arc<GetUser>,
    pub get_user_stats: Arc<GetUserStats>,
    pub upload_avatar: Arc<UploadAvatar>,
}

impl UserState {
    pub fn new(
        users: Arc<dyn UserRepository>,
        bets: Arc<dyn BetRepository>,
        storage: Arc<dyn FileStorage>,
    ) -> Self {
        Self {
            get_user: Arc::new(GetUser::new(users.clone())),
            get_user_stats: Arc::new(GetUserStats::new(bets)),
            upload_avatar: Arc::new(UploadAvatar::new(users, storage)),
        }
    }
}
