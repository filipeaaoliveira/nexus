use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::services::accounts::storage::LoadOutcome;
use crate::services::accounts::{codex as codex_service, storage as storage_service};
use nexus_domain::accounts::{DomainProfile, EditableProfile, Provider};

use super::traits::ProfileStore;

#[derive(Clone)]
pub struct CodexProfileStore {
    profiles_dir: PathBuf,
}

impl CodexProfileStore {
    pub fn new(profiles_dir: PathBuf) -> Self {
        Self { profiles_dir }
    }
}

fn notes_option(notes: &str) -> Option<String> {
    if notes.is_empty() {
        None
    } else {
        Some(notes.to_string())
    }
}

impl ProfileStore for CodexProfileStore {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn scan_profile_ids(&self) -> Vec<String> {
        storage_service::scan_codex_profile_ids(&self.profiles_dir)
    }

    fn load_profile(&self, profile_id: &str) -> Result<LoadOutcome<DomainProfile>> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let outcome = storage_service::load_codex_profile(&profile_dir)?;
        Ok(match outcome {
            LoadOutcome::Clean(v) => LoadOutcome::Clean(DomainProfile::from(v)),
            LoadOutcome::Degraded { value, warnings } => LoadOutcome::Degraded {
                value: DomainProfile::from(value),
                warnings,
            },
        })
    }

    fn load_editable(&self, profile_id: &str) -> Result<EditableProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        let profile = storage_service::load_codex_profile(&profile_dir)?.into_value();
        Ok(EditableProfile {
            label: profile.label,
            notes: profile.notes.unwrap_or_default(),
            enabled: profile.enabled,
        })
    }

    fn save_edits(&self, profile_id: &str, draft: &EditableProfile) -> Result<DomainProfile> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::save_codex_profile_metadata(
            &profile_dir,
            profile_id,
            &draft.label,
            notes_option(&draft.notes),
            draft.enabled,
        )?;
        let profile = storage_service::load_codex_profile(&profile_dir)?.into_value();
        Ok(DomainProfile::from(profile))
    }

    fn import_from_home(&self, home: &Path) -> Result<DomainProfile> {
        let profile = codex_service::import_profile_from_home(&self.profiles_dir, home)?;
        Ok(DomainProfile::from(profile))
    }

    fn write_to_home(&self, profile_id: &str, home: &Path) -> Result<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        codex_service::write_profile_to_home(&profile_dir, home)
    }

    fn delete(&self, profile_id: &str) -> Result<()> {
        let profile_dir = self.profiles_dir.join(profile_id);
        storage_service::delete_profile_dir(&profile_dir)
    }
}
