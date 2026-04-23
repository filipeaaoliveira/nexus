use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::app::dispatch::effects::dispatch_effect;
use crate::app::effect::{Effect, EffectResult};
use crate::app::EditableProfile;
use crate::config::Config;
use crate::services::accounts::provider::StoreProvider;
use crate::services::accounts::storage::{LoadOutcome, ProfileStoreError, StoreResult};
use crate::services::accounts::traits::ProfileStore;
use nexus_domain::accounts::{DomainProfile, ProfileId, Provider};

// ---------------------------------------------------------------------------
// Mock ProfileStore
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockProfileStore {
    provider: Provider,
    profiles: Arc<Mutex<Vec<DomainProfile>>>,
}

impl MockProfileStore {
    fn new(provider: Provider, profiles: Vec<DomainProfile>) -> Self {
        Self {
            provider,
            profiles: Arc::new(Mutex::new(profiles)),
        }
    }

    fn empty(provider: Provider) -> Self {
        Self::new(provider, Vec::new())
    }
}

impl ProfileStore for MockProfileStore {
    fn provider(&self) -> Provider {
        self.provider
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        let lock = self.profiles.lock().unwrap();
        lock.iter().map(|p| p.profile_id().to_string()).collect()
    }

    fn load_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
        let lock = self.profiles.lock().unwrap();
        lock.iter()
            .find(|p| p.profile_id() == profile_id)
            .cloned()
            .map(LoadOutcome::Clean)
            .ok_or_else(|| ProfileStoreError::NotFound {
                profile_id: profile_id.to_string(),
            })
    }

    fn load_editable(&self, profile_id: &str) -> StoreResult<EditableProfile> {
        let lock = self.profiles.lock().unwrap();
        let profile = lock
            .iter()
            .find(|p| p.profile_id() == profile_id)
            .ok_or_else(|| ProfileStoreError::NotFound {
                profile_id: profile_id.to_string(),
            })?;
        Ok(EditableProfile {
            label: profile.label().to_string(),
            notes: profile.notes().unwrap_or_default().to_string(),
            enabled: profile.enabled(),
        })
    }

    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile> {
        let mut lock = self.profiles.lock().unwrap();
        let profile = lock
            .iter_mut()
            .find(|p| p.profile_id() == profile_id)
            .ok_or_else(|| ProfileStoreError::NotFound {
                profile_id: profile_id.to_string(),
            })?;
        *profile = DomainProfile::new(
            profile.id().clone(),
            profile.provider(),
            draft.label.clone(),
            profile.email().map(str::to_string),
            profile.plan().map(str::to_string),
            Some(draft.notes.clone()),
            draft.enabled,
            profile.created_at(),
            Utc::now(),
        );
        Ok(profile.clone())
    }

    fn import_from_home(&self, _home: &Path) -> StoreResult<DomainProfile> {
        Err(ProfileStoreError::Other {
            context: "not implemented".to_string(),
            source: "mock".into(),
        })
    }

    fn write_to_home(&self, _profile_id: &str, _home: &Path) -> StoreResult<()> {
        Err(ProfileStoreError::Other {
            context: "not implemented".to_string(),
            source: "mock".into(),
        })
    }

    fn delete(&self, profile_id: &str) -> StoreResult<()> {
        let mut lock = self.profiles.lock().unwrap();
        let idx = lock
            .iter()
            .position(|p| p.profile_id() == profile_id)
            .ok_or_else(|| ProfileStoreError::NotFound {
                profile_id: profile_id.to_string(),
            })?;
        lock.remove(idx);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock StoreProvider
// ---------------------------------------------------------------------------

struct MockStoreProvider {
    codex: MockProfileStore,
    claude: MockProfileStore,
}

impl StoreProvider for MockStoreProvider {
    type Store = MockProfileStore;

    fn store_for(&self, provider: Provider) -> MockProfileStore {
        match provider {
            Provider::Codex => self.codex.clone(),
            Provider::Claude => self.claude.clone(),
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn test_profile(provider: Provider, id: &str, label: &str) -> DomainProfile {
    let now = Utc::now();
    DomainProfile::new(
        ProfileId::new_unchecked(id),
        provider,
        label.to_string(),
        Some(format!("{id}@example.com")),
        Some("pro".to_string()),
        Some(String::new()),
        true,
        now,
        now,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_load_profiles_returns_all_profiles() {
    let stores = MockStoreProvider {
        codex: MockProfileStore::new(
            Provider::Codex,
            vec![test_profile(Provider::Codex, "codex-1", "Codex One")],
        ),
        claude: MockProfileStore::new(
            Provider::Claude,
            vec![test_profile(Provider::Claude, "claude-1", "Claude One")],
        ),
    };

    let result = dispatch_effect(&Config::default(), &stores, Effect::LoadProfiles).await;

    match result {
        EffectResult::ProfilesLoaded { profiles, entries } => {
            assert_eq!(profiles.len(), 2);
            assert_eq!(entries.len(), 2);

            let codex = profiles
                .iter()
                .find(|p| p.provider() == Provider::Codex)
                .unwrap();
            assert_eq!(codex.profile_id(), "codex-1");
            assert_eq!(codex.label(), "Codex One");

            let claude = profiles
                .iter()
                .find(|p| p.provider() == Provider::Claude)
                .unwrap();
            assert_eq!(claude.profile_id(), "claude-1");
            assert_eq!(claude.label(), "Claude One");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[tokio::test]
async fn mock_save_edits_updates_profile() {
    let stores = MockStoreProvider {
        codex: MockProfileStore::new(
            Provider::Codex,
            vec![test_profile(Provider::Codex, "codex-1", "Original")],
        ),
        claude: MockProfileStore::empty(Provider::Claude),
    };

    let result = dispatch_effect(
        &Config::default(),
        &stores,
        Effect::SaveProfileEdits {
            provider: Provider::Codex,
            profile_id: ProfileId::new_unchecked("codex-1"),
            draft: EditableProfile {
                label: "Renamed".to_string(),
                notes: "new notes".to_string(),
                enabled: false,
            },
        },
    )
    .await;

    match result {
        EffectResult::ProfileSaved { profile } => {
            assert_eq!(profile.label(), "Renamed");
            assert_eq!(profile.notes(), Some("new notes"));
            assert!(!profile.enabled());
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[tokio::test]
async fn mock_delete_profile_returns_deleted_entry() {
    let stores = MockStoreProvider {
        codex: MockProfileStore::empty(Provider::Codex),
        claude: MockProfileStore::new(
            Provider::Claude,
            vec![test_profile(Provider::Claude, "claude-1", "To Delete")],
        ),
    };

    let result = dispatch_effect(
        &Config::default(),
        &stores,
        Effect::DeleteProfile {
            provider: Provider::Claude,
            profile_id: ProfileId::new_unchecked("claude-1"),
        },
    )
    .await;

    match result {
        EffectResult::ProfileDeleted { entry } => {
            assert_eq!(entry.provider, Provider::Claude);
            assert_eq!(entry.profile_id, "claude-1");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[tokio::test]
async fn mock_load_empty_stores_returns_empty() {
    let stores = MockStoreProvider {
        codex: MockProfileStore::empty(Provider::Codex),
        claude: MockProfileStore::empty(Provider::Claude),
    };

    let result = dispatch_effect(&Config::default(), &stores, Effect::LoadProfiles).await;

    match result {
        EffectResult::ProfilesLoaded { profiles, entries } => {
            assert!(profiles.is_empty());
            assert!(entries.is_empty());
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
