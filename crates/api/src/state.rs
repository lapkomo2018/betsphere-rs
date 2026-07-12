use std::sync::Arc;

use application::use_cases::user::{CreateUser, GetUser, ListUsers};
use domain::repositories::UserRepository;

/// Shared handler state holding the wired-up use cases.
#[derive(Clone)]
pub struct AppState {
    pub create_user: Arc<CreateUser>,
    pub get_user: Arc<GetUser>,
    pub list_users: Arc<ListUsers>,
}

impl AppState {
    pub fn new(users: Arc<dyn UserRepository>) -> Self {
        Self {
            create_user: Arc::new(CreateUser::new(users.clone())),
            get_user: Arc::new(GetUser::new(users.clone())),
            list_users: Arc::new(ListUsers::new(users)),
        }
    }
}
