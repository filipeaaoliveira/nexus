use std::path::{Path, PathBuf};

use crate::services::accounts::storage::{LoadOutcome, ProfileStoreError, StoreResult};
use crate::services::accounts::{codex as codex_service, claude as claude_service, storage as storage_service};
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

fn notes_option(notes: &str) -> Option<String> {
    if notes.is_empty() {
        None
    } else {
        Some(notes.to_string())
    }
}

#[derive(Clone)]
pub struct CodexStorageBackend {
    pub(crate) profiles_dir: PathBuf,
}

impl StorageBackend for CodexStorageBackend {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        storage_service::scan_codex_profile_ids(&self.profiles_dir)
    }

    fn load_raw_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let outcome = storage_service::load_codex_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_codex_profile", e))?;
        Ok(match outcome {
            LoadOutcome::Clean(v) => LoadOutcome::Clean(v.to_domain(Provider::Codex)),
            LoadOutcome::Degraded { value, warnings } => LoadOutcome::Degraded {
                value: value.to_domain(Provider::Codex),
                warnings,
            },
        })
    }

    fn load_raw_editable(&self, profile_id: &str) -> StoreResult<EditableProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let profile = storage_service::load_codex_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_codex_profile", e))?
            .into_value();
        Ok(EditableProfile {
            label: profile.label,
            notes: profile.notes.unwrap_or_default(),
            enabled: profile.enabled,
        })
    }

    fn save_raw_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::save_codex_profile_metadata(
            &profile_dir,
            profile_id,
            &draft.label,
            notes_option(&draft.notes),
            draft.enabled,
        )
        .map_err(|e| ProfileStoreError::other("save_codex_profile_metadata", e))?;
        let profile = storage_service::load_codex_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_codex_profile", e))?
            .into_value();
        Ok(profile.to_domain(Provider::Codex))
    }

    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile> {
        let profile = codex_service::import_profile_from_home(&self.profiles_dir, home)
            .map_err(|e| ProfileStoreError::other("import_profile_from_home", e))?;
        Ok(profile.to_domain(Provider::Codex))
    }

    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        codex_service::write_profile_to_home(&profile_dir, home)
            .map_err(|e| ProfileStoreError::other("write_profile_to_home", e))
    }

    fn delete(&self, profile_id: &str) -> StoreResult<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::delete_profile_dir(&profile_dir)
            .map_err(|e| ProfileStoreError::other("delete_profile_dir", e))
    }
}

#[derive(Clone)]
pub struct ClaudeStorageBackend {
    pub(crate) profiles_dir: PathBuf,
}

impl StorageBackend for ClaudeStorageBackend {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        storage_service::scan_claude_profile_ids(&self.profiles_dir)
    }

    fn load_raw_profile(&self, profile_id: &str) -> StoreResult<LoadOutcome<DomainProfile>> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let outcome = storage_service::load_claude_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_claude_profile", e))?;
        Ok(match outcome {
            LoadOutcome::Clean(v) => LoadOutcome::Clean(v.to_domain(Provider::Claude)),
            LoadOutcome::Degraded { value, warnings } => LoadOutcome::Degraded {
                value: value.to_domain(Provider::Claude),
                warnings,
            },
        })
    }

    fn load_raw_editable(&self, profile_id: &str) -> StoreResult<EditableProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let profile = storage_service::load_claude_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_claude_profile", e))?
            .into_value();
        Ok(EditableProfile {
            label: profile.label,
            notes: profile.notes.unwrap_or_default(),
            enabled: profile.enabled,
        })
    }

    fn save_raw_edits(&self, profile_id: &str, draft: &EditableProfile) -> StoreResult<DomainProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::save_claude_profile_edits(
            &profile_dir,
            profile_id,
            &draft.label,
            notes_option(&draft.notes),
            draft.enabled,
        )
        .map_err(|e| ProfileStoreError::other("save_claude_profile_edits", e))?;
        let profile = storage_service::load_claude_profile(&profile_dir)
            .map_err(|e| ProfileStoreError::other("load_claude_profile", e))?
            .into_value();
        Ok(profile.to_domain(Provider::Claude))
    }

    fn import_from_home(&self, home: &Path) -> StoreResult<DomainProfile> {
        let profile = claude_service::import_profile_from_home(&self.profiles_dir, home)
            .map_err(|e| ProfileStoreError::other("import_profile_from_home", e))?;
        Ok(profile.to_domain(Provider::Claude))
    }

    fn write_to_home(&self, profile_id: &str, home: &Path) -> StoreResult<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        claude_service::write_profile_to_home(&profile_dir, home)
            .map_err(|e| ProfileStoreError::other("write_profile_to_home", e))
    }

    fn delete(&self, profile_id: &str) -> StoreResult<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::delete_profile_dir(&profile_dir)
            .map_err(|e| ProfileStoreError::other("delete_profile_dir", e))
    }
}
