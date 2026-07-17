use std::sync::Arc;

use application::use_cases::user::UpdateUser;
use domain::repositories::UserRepository;

/// State for the internal/system endpoints. Holds their use cases plus the
/// shared secret the [`InternalAuth`](crate::extract::InternalAuth) extractor
/// checks; `None` disables the internal API entirely.
#[derive(Clone)]
pub struct InternalState {
    pub update_user: Arc<UpdateUser>,
    pub api_key: Option<Arc<str>>,
}

impl InternalState {
    pub fn new(users: Arc<dyn UserRepository>, api_key: Option<String>) -> Self {
        Self {
            update_user: Arc::new(UpdateUser::new(users)),
            api_key: api_key.map(Arc::from),
        }
    }
}
