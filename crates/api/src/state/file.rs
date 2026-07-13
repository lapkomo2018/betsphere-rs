use std::sync::Arc;

use application::ports::FileStorage;
use application::use_cases::file::GetFile;

#[derive(Clone)]
pub struct FileState {
    pub get_file: Arc<GetFile>,
}

impl FileState {
    pub fn new(storage: Arc<dyn FileStorage>) -> Self {
        Self {
            get_file: Arc::new(GetFile::new(storage)),
        }
    }
}
