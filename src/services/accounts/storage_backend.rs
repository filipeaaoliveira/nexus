use std::path::Path;

use crate::services::accounts::storage::{LoadOutcome, StoreResult};
use nexus_domain::accounts::{DomainProfile, EditableProfile, Provider};

pub trait StorageBackend: Send + Sync + 'static {
    fn provider(&self) -> Provider;
    fn scan_profile_ids(&self) -> Vec<String>;
    fn load_raw_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>>;
    fn load_raw_editable(&self, profile_id: &str) -> StoreResult<EditableProfile>;
    fn save_raw_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile>;
    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile>;
    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()>;
    fn delete(&self, profile_id: &str) -> StoreResult<()>;
}
