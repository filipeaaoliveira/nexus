use std::path::{Path, PathBuf};

use crate::app::EditableProfile;
use crate::profiles::StoredProfile;
use crate::services::accounts::storage::{LoadOutcome, ProfileStoreError, StoreResult};
use crate::services::accounts::traits::ProfileStore;
use crate::services::accounts::{
    claude as claude_service, codex as codex_service, storage as storage_service,
};
use nexus_domain::accounts::{DomainProfile, Provider};

fn notes_option(notes: &str) -> Option<String> {
    if notes.is_empty() {
        None
    } else {
        Some(notes.to_string())
    }
}

#[derive(Clone)]
pub struct ProviderBackend {
    provider: Provider,
    profiles_dir: PathBuf,
}

impl ProviderBackend {
    pub fn new(provider: Provider, profiles_dir: PathBuf) -> Self {
        Self {
            provider,
            profiles_dir,
        }
    }

    fn profile_dir(&self, profile_id: &str) -> StoreResult<PathBuf> {
        let dir = self.profiles_dir.join(profile_id);
        if !dir.is_dir() {
            return Err(ProfileStoreError::NotFound {
                profile_id: profile_id.to_string(),
            });
        }
        Ok(dir)
    }

    fn load_stored_profile(&self, profile_dir: &Path) -> StoreResult<LoadOutcome<StoredProfile>> {
        let load_fn = match self.provider {
            Provider::Codex => storage_service::load_codex_profile,
            Provider::Claude => storage_service::load_claude_profile,
            _ => unreachable!(),
        };
        load_fn(profile_dir).map_err(|e| ProfileStoreError::other("load_profile", e))
    }
}

impl ProfileStore for ProviderBackend {
    fn provider(&self) -> Provider {
        self.provider
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        let auth_file = match self.provider {
            Provider::Codex => "auth.json",
            Provider::Claude => ".credentials.json",
            _ => unreachable!(),
        };
        storage_service::scan_profile_ids(&self.profiles_dir, auth_file)
    }

    fn load_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
        let profile_dir = self.profile_dir(profile_id)?;
        let outcome = self.load_stored_profile(&profile_dir)?;
        Ok(match outcome {
            LoadOutcome::Clean(v) => LoadOutcome::Clean(v.to_domain(self.provider)),
            LoadOutcome::Degraded { value, warnings } => LoadOutcome::Degraded {
                value: value.to_domain(self.provider),
                warnings,
            },
        })
    }

    fn load_editable(&self, profile_id: &str) -> StoreResult<EditableProfile> {
        let profile_dir = self.profile_dir(profile_id)?;
        let profile = self.load_stored_profile(&profile_dir)?.into_value();
        Ok(EditableProfile {
            label: profile.label,
            notes: profile.notes.unwrap_or_default(),
            enabled: profile.enabled,
        })
    }

    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile> {
        let profile_dir = self.profile_dir(profile_id)?;
        let preserve = match self.provider {
            Provider::Codex => false,
            Provider::Claude => true,
            _ => unreachable!(),
        };
        storage_service::save_profile_edits(
            &profile_dir,
            profile_id,
            &draft.label,
            notes_option(&draft.notes),
            draft.enabled,
            preserve,
        )
        .map_err(|e| ProfileStoreError::other("save_profile_edits", e))?;

        let profile = self.load_stored_profile(&profile_dir)?.into_value();
        Ok(profile.to_domain(self.provider))
    }

    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile> {
        let import_fn = match self.provider {
            Provider::Codex => {
                codex_service::import_profile_from_home as fn(&Path, &Path) -> anyhow::Result<_>
            }
            Provider::Claude => claude_service::import_profile_from_home,
            _ => unreachable!(),
        };
        let profile = import_fn(&self.profiles_dir, home)
            .map_err(|e| ProfileStoreError::other("import_profile_from_home", e))?;
        Ok(profile.to_domain(self.provider))
    }

    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()> {
        let profile_dir = self.profile_dir(profile_id)?;
        let write_fn = match self.provider {
            Provider::Codex => {
                codex_service::write_profile_to_home as fn(&Path, &Path) -> anyhow::Result<()>
            }
            Provider::Claude => claude_service::write_profile_to_home,
            _ => unreachable!(),
        };
        write_fn(&profile_dir, home)
            .map_err(|e| ProfileStoreError::other("write_profile_to_home", e))
    }

    fn delete(&self, profile_id: &str) -> StoreResult<()> {
        let profile_dir = self.profile_dir(profile_id)?;
        storage_service::delete_profile_dir(&profile_dir)
            .map_err(|e| ProfileStoreError::other("delete_profile_dir", e))
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileStoreError;
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn provider_backend_returns_correct_provider() {
        let tmp = TempDir::new().unwrap();
        let codex = ProviderBackend::new(Provider::Codex, tmp.path().to_path_buf());
        let claude = ProviderBackend::new(Provider::Claude, tmp.path().to_path_buf());
        assert_eq!(codex.provider(), Provider::Codex);
        assert_eq!(claude.provider(), Provider::Claude);
    }

    #[test]
    fn provider_backend_scans_codex_profiles_by_auth_json() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path().join("my-profile");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("auth.json"), "{}").unwrap();

        let backend = ProviderBackend::new(Provider::Codex, tmp.path().to_path_buf());
        let ids = backend.scan_profile_ids();
        assert_eq!(ids, vec!["my-profile"]);
    }

    #[test]
    fn provider_backend_scans_claude_profiles_by_credentials() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path().join("claude-prof");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join(".credentials.json"), "{}").unwrap();

        let backend = ProviderBackend::new(Provider::Claude, tmp.path().to_path_buf());
        let ids = backend.scan_profile_ids();
        assert_eq!(ids, vec!["claude-prof"]);
    }

    #[test]
    fn load_profile_returns_not_found_for_missing_profile() {
        let tmp = TempDir::new().unwrap();
        let backend = ProviderBackend::new(Provider::Codex, tmp.path().to_path_buf());
        let result = backend.load_profile("nonexistent");
        assert!(matches!(result, Err(ProfileStoreError::NotFound { .. })));
    }

    #[test]
    fn delete_missing_profile_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let backend = ProviderBackend::new(Provider::Codex, tmp.path().to_path_buf());
        let result = backend.delete("nonexistent");
        assert!(matches!(result, Err(ProfileStoreError::NotFound { .. })));
    }
}
