use std::path::Path;

use anyhow::Result;

use crate::domain::accounts::{DomainProfile, EditableProfile, Provider};
use crate::services::accounts::storage::LoadOutcome;

/// Abstracts profile storage operations for a single provider.
///
/// Implementations are cheap `PathBuf` wrappers that can be moved into
/// `spawn_blocking` closures (which require `'static`).
pub trait ProfileStore: Send + Sync + 'static {
    fn provider(&self) -> Provider;
    fn scan_profile_ids(&self) -> Vec<String>;
    fn load_profile(&self, profile_id: &str) -> Result<LoadOutcome<DomainProfile>>;
    fn load_editable(&self, profile_id: &str) -> Result<EditableProfile>;
    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> Result<DomainProfile>;
    fn import_from_home(&self, home: &Path) -> Result<DomainProfile>;
    fn write_to_home(&self, profile_id: &str, home: &Path) -> Result<()>;
    fn delete(&self, profile_id: &str) -> Result<()>;
}
