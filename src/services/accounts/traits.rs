use std::path::Path;

use crate::services::accounts::storage::{LoadOutcome, StoreResult};
use nexus_domain::accounts::{DomainProfile, EditableProfile, Provider};

/// Abstracts profile storage operations for a single provider.
///
/// Implementations are cheap `PathBuf` wrappers that can be moved into
/// `spawn_blocking` closures (which require `'static`).
pub trait ProfileStore: Send + Sync + 'static {
    fn provider(&self) -> Provider;
    fn scan_profile_ids(&self) -> Vec<String>;
    fn load_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>>;
    fn load_editable(&self, profile_id: &str) -> StoreResult<EditableProfile>;
    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile>;
    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile>;
    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()>;
    fn delete(&self, profile_id: &str) -> StoreResult<()>;
}
