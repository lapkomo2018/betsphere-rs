use std::sync::Arc;

use application::ports::FileStorage;
use application::use_cases::user::{GetUser, UploadAvatar};
use domain::repositories::UserRepository;

#[derive(Clone)]
pub struct UserState {
    pub get_user: Arc<GetUser>,
    pub upload_avatar: Arc<UploadAvatar>,
}

impl UserState {
    pub fn new(users: Arc<dyn UserRepository>, storage: Arc<dyn FileStorage>) -> Self {
        Self {
            get_user: Arc::new(GetUser::new(users.clone())),
            upload_avatar: Arc::new(UploadAvatar::new(users, storage)),
        }
    }
}
