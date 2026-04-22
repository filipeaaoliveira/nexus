use std::path::Path;

use crate::services::accounts::storage::{LoadOutcome, StoreResult};
use crate::services::accounts::storage_backend::StorageBackend;
use crate::services::accounts::traits::ProfileStore;
use crate::app::EditableProfile;
use nexus_domain::accounts::{DomainProfile, Provider};

pub struct GenericProfileStore<S> {
    backend: S,
}

impl<S: StorageBackend> GenericProfileStore<S> {
    pub fn new(backend: S) -> Self {
        Self { backend }
    }
}

impl<S: StorageBackend> ProfileStore for GenericProfileStore<S> {
    fn provider(&self) -> Provider {
        self.backend.provider()
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        self.backend.scan_profile_ids()
    }

    fn load_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
        self.backend.load_raw_profile(profile_id)
    }

    fn load_editable(&self, profile_id: &str) -> StoreResult<EditableProfile> {
        self.backend.load_raw_editable(profile_id)
    }

    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile> {
        self.backend.save_raw_edits(profile_id, draft)
    }

    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile> {
        self.backend.import_from_home(home)
    }

    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()> {
        self.backend.write_to_home(profile_id, home)
    }

    fn delete(&self, profile_id: &str) -> StoreResult<()> {
        self.backend.delete(profile_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use nexus_domain::accounts::ProfileId;

    struct FakeBackend;

    impl StorageBackend for FakeBackend {
        fn provider(&self) -> Provider { Provider::Codex }
        fn scan_profile_ids(&self) -> Vec<String> { vec!["p1".to_string()] }
        fn load_raw_profile(&self, _profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
            Ok(LoadOutcome::Clean(DomainProfile {
                id: ProfileId::new_unchecked("p1"),
                provider: Provider::Codex,
                label: "Test".to_string(),
                email: None,
                plan: None,
                notes: None,
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }))
        }
        fn load_raw_editable(&self, _profile_id: &str) -> StoreResult<EditableProfile> {
            Ok(EditableProfile { label: "Test".to_string(), notes: String::new(), enabled: true })
        }
        fn save_raw_edits(&self, _profile_id: &str, _draft: &EditableProfile) -> StoreResult<DomainProfile> {
            self.load_raw_profile("p1").map(|outcome| outcome.into_value())
        }
        fn import_from_home(&self, _home: &Path) -> StoreResult<DomainProfile> {
            self.load_raw_profile("p1").map(|outcome| outcome.into_value())
        }
        fn write_to_home(&self, _profile_id: &str, _home: &Path) -> StoreResult<()> { Ok(()) }
        fn delete(&self, _profile_id: &str) -> StoreResult<()> { Ok(()) }
    }

    #[test]
    fn generic_store_delegates_to_backend() {
        let store = GenericProfileStore::new(FakeBackend);
        assert_eq!(store.provider(), Provider::Codex);
        assert_eq!(store.scan_profile_ids(), vec!["p1".to_string()]);
    }
}
