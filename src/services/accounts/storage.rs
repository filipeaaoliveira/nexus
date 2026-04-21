use std::fs;
use std::path::Path;

use crate::claude_profiles::auth::ClaudeAuth;
use crate::claude_profiles::profile::StoredClaudeProfile;
use crate::codex_profiles::profile::{extract_auth_metadata, AuthMetadata, StoredCodexProfile};
use crate::profiles::{io as profile_io, ProfileMetadata};
use anyhow::{Context, Result};
use chrono::Utc;

#[derive(Debug, thiserror::Error)]
pub enum ProfileStoreError {
    #[error("profile not found: {profile_id}")]
    #[allow(dead_code)]
    NotFound { profile_id: String },
    #[error("corrupted profile data at {path}: {detail}")]
    #[allow(dead_code)]
    Corrupted { path: String, detail: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{context}: {source}")]
    Other { context: String, #[source] source: Box<dyn std::error::Error + Send + Sync> },
}

pub type StoreResult<T> = Result<T, ProfileStoreError>;

impl ProfileStoreError {
    #[allow(dead_code)]
    pub fn corrupted(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Corrupted { path: path.to_string(), detail: detail.to_string() }
    }
    pub fn other(context: impl std::fmt::Display, source: impl std::fmt::Display) -> Self {
        #[derive(Debug)]
        struct StringError(String);
        impl std::fmt::Display for StringError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl std::error::Error for StringError {}
        Self::Other { context: context.to_string(), source: Box::new(StringError(source.to_string())) }
    }
}

/// Result of loading a profile from disk.
///
/// `Clean` means all files parsed without error.
/// `Degraded` means the profile was loaded but some files were corrupted
/// and defaults were substituted. The `warnings` list describes what was wrong.
#[derive(Debug)]
pub enum LoadOutcome<T> {
    Clean(T),
    Degraded { value: T, warnings: Vec<String> },
}

impl<T> LoadOutcome<T> {
    #[allow(dead_code)]
    pub fn value(&self) -> &T {
        match self {
            Self::Clean(v) | Self::Degraded { value: v, .. } => v,
        }
    }

    pub fn into_value(self) -> T {
        match self {
            Self::Clean(v) | Self::Degraded { value: v, .. } => v,
        }
    }

    #[allow(dead_code)]
    pub fn warnings(&self) -> &[String] {
        match self {
            Self::Clean(_) => &[],
            Self::Degraded { warnings, .. } => warnings,
        }
    }
}

pub fn next_profile_id(profiles_dir: &Path) -> String {
    let used: std::collections::BTreeSet<u32> = fs::read_dir(profiles_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .strip_prefix("profile-")
                        .and_then(|value| value.parse::<u32>().ok())
                })
                .collect()
        })
        .unwrap_or_default();

    let next = (1u32..).find(|value| !used.contains(value)).unwrap_or(1);
    format!("profile-{next:02}")
}

pub fn scan_codex_profile_ids(profiles_dir: &Path) -> Vec<String> {
    scan_profile_ids(profiles_dir, "auth.json")
}

pub fn scan_claude_profile_ids(profiles_dir: &Path) -> Vec<String> {
    scan_profile_ids(profiles_dir, ".credentials.json")
}

pub fn load_codex_profile(profile_dir: &Path) -> Result<LoadOutcome<StoredCodexProfile>> {
    let profile_id = profile_id_from_dir(profile_dir)?;
    let auth_path = profile_dir.join("auth.json");
    let mut warnings = Vec::new();

    let auth_metadata = match load_codex_auth_metadata_strict(&auth_path) {
        Ok(m) => m,
        Err(error) => {
            warnings.push(format!("corrupted auth.json: {error}"));
            AuthMetadata::default()
        }
    };

    let metadata = match profile_io::load_profile_metadata(profile_dir) {
        Ok(m) => m,
        Err(error) => {
            warnings.push(format!("corrupted profile.json: {error}"));
            ProfileMetadata::default()
        }
    };

    let now = Utc::now();
    let profile = StoredCodexProfile {
        id: profile_id.clone(),
        label: metadata.label.unwrap_or(profile_id.clone()),
        email: auth_metadata.email.or(metadata.email),
        plan: auth_metadata.plan.or(metadata.plan),
        notes: metadata.notes,
        enabled: metadata.enabled,
        last_used: None,
        created_at: metadata.created_at.unwrap_or(now),
        updated_at: metadata.updated_at.unwrap_or(now),
    };

    if warnings.is_empty() {
        Ok(LoadOutcome::Clean(profile))
    } else {
        for w in &warnings {
            tracing::warn!(profile_dir = %profile_dir.display(), "{w}");
        }
        Ok(LoadOutcome::Degraded {
            value: profile,
            warnings,
        })
    }
}

fn load_codex_auth_metadata_strict(auth_path: &Path) -> Result<AuthMetadata> {
    let content = match fs::read_to_string(auth_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AuthMetadata::default());
        }
        Err(e) => return Err(e.into()),
    };
    let auth_json: serde_json::Value =
        serde_json::from_str(&content).context("invalid JSON in auth.json")?;
    Ok(extract_auth_metadata(&auth_json))
}

pub fn load_claude_profile(profile_dir: &Path) -> Result<LoadOutcome<StoredClaudeProfile>> {
    let profile_id = profile_id_from_dir(profile_dir)?;
    let auth_path = profile_dir.join(".credentials.json");
    let mut warnings = Vec::new();

    let auth = match ClaudeAuth::load(&auth_path) {
        Ok(a) => a,
        Err(error) => {
            warnings.push(format!("corrupted .credentials.json: {error}"));
            ClaudeAuth::default()
        }
    };

    let metadata = match profile_io::load_profile_metadata(profile_dir) {
        Ok(m) => m,
        Err(error) => {
            warnings.push(format!("corrupted profile.json: {error}"));
            ProfileMetadata::default()
        }
    };

    let now = Utc::now();
    let profile = StoredClaudeProfile {
        id: profile_id.clone(),
        label: metadata.label.unwrap_or(profile_id.clone()),
        email: metadata.email,
        plan: metadata
            .plan
            .or_else(|| auth.subscription_type().map(str::to_string)),
        notes: metadata.notes,
        enabled: metadata.enabled,
        last_used: None,
        created_at: metadata.created_at.unwrap_or(now),
        updated_at: metadata.updated_at.unwrap_or(now),
    };

    if warnings.is_empty() {
        Ok(LoadOutcome::Clean(profile))
    } else {
        for w in &warnings {
            tracing::warn!(profile_dir = %profile_dir.display(), "{w}");
        }
        Ok(LoadOutcome::Degraded {
            value: profile,
            warnings,
        })
    }
}

pub fn save_codex_profile_metadata(
    profile_dir: &Path,
    profile_id: &str,
    label: &str,
    notes: Option<String>,
    enabled: bool,
) -> Result<()> {
    let metadata = ProfileMetadata {
        id: profile_id.to_string(),
        label: Some(label.to_string()),
        email: None,
        plan: None,
        notes,
        enabled,
        created_at: None,
        updated_at: Some(Utc::now()),
    };
    profile_io::save_profile_metadata(profile_dir, &metadata)
}

pub fn save_claude_profile_edits(
    profile_dir: &Path,
    profile_id: &str,
    label: &str,
    notes: Option<String>,
    enabled: bool,
) -> Result<()> {
    let existing = match profile_io::load_profile_metadata(profile_dir) {
        Ok(m) => m,
        Err(error) => {
            tracing::warn!(
                profile_dir = %profile_dir.display(),
                error = %error,
                "corrupted profile.json during save, starting from defaults"
            );
            ProfileMetadata::default()
        }
    };
    let metadata = ProfileMetadata {
        id: profile_id.to_string(),
        label: Some(label.to_string()),
        email: existing.email,
        plan: existing.plan,
        notes,
        enabled,
        created_at: existing.created_at,
        updated_at: Some(Utc::now()),
    };
    profile_io::save_profile_metadata(profile_dir, &metadata)
}

pub fn delete_profile_dir(profile_dir: &Path) -> Result<()> {
    if profile_dir.exists() {
        fs::remove_dir_all(profile_dir)
            .with_context(|| format!("failed to delete {}", profile_dir.display()))?;
    }
    Ok(())
}

fn scan_profile_ids(profiles_dir: &Path, auth_file_name: &str) -> Vec<String> {
    let mut profile_ids = Vec::new();
    if let Ok(entries) = fs::read_dir(profiles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join(auth_file_name).is_file() {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    profile_ids.push(name.to_string());
                }
            }
        }
    }
    profile_ids.sort();
    profile_ids
}

fn profile_id_from_dir(profile_dir: &Path) -> Result<String> {
    profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .context("profile directory must have a valid UTF-8 name")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::services::accounts::storage::{
        delete_profile_dir, load_claude_profile, load_codex_profile, next_profile_id,
        save_claude_profile_edits, save_codex_profile_metadata, LoadOutcome,
    };

    #[test]
    fn load_codex_profile_reads_auth_and_profile_metadata() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"person@example.com","plan":"pro"}"#,
        )
        .unwrap();

        save_codex_profile_metadata(
            &profile_dir,
            "profile-01",
            "Main Profile",
            Some("notes from metadata".to_string()),
            false,
        )
        .unwrap();

        let profile = load_codex_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.id, "profile-01");
        assert_eq!(profile.label, "Main Profile");
        assert_eq!(profile.email.as_deref(), Some("person@example.com"));
        assert_eq!(profile.plan.as_deref(), Some("pro"));
        assert_eq!(profile.notes.as_deref(), Some("notes from metadata"));
        assert!(!profile.enabled);
    }

    #[test]
    fn load_codex_profile_uses_auth_metadata_when_profile_metadata_is_missing() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-07");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"fallback@example.com","plan":"plus"}"#,
        )
        .unwrap();

        let profile = load_codex_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.id, "profile-07");
        assert_eq!(profile.label, "profile-07");
        assert_eq!(profile.email.as_deref(), Some("fallback@example.com"));
        assert_eq!(profile.plan.as_deref(), Some("plus"));
        assert_eq!(profile.notes, None);
        assert!(profile.enabled);
    }

    #[test]
    fn load_codex_profile_keeps_saved_metadata_when_auth_json_is_invalid() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-08");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("auth.json"), "{not-json").unwrap();

        save_codex_profile_metadata(
            &profile_dir,
            "profile-08",
            "Saved Label",
            Some("persisted notes".to_string()),
            false,
        )
        .unwrap();

        let profile = load_codex_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.id, "profile-08");
        assert_eq!(profile.label, "Saved Label");
        assert_eq!(profile.notes.as_deref(), Some("persisted notes"));
        assert!(!profile.enabled);
    }

    #[test]
    fn load_codex_profile_prefers_current_auth_metadata_over_stale_profile_metadata() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-09");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"current@example.com","plan":"team"}"#,
        )
        .unwrap();
        fs::write(
            profile_dir.join("profile.json"),
            r#"{
                "id":"profile-09",
                "label":"Current Label",
                "email":"stale@example.com",
                "plan":"plus",
                "notes":"stale notes",
                "enabled":true
            }"#,
        )
        .unwrap();

        let profile = load_codex_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.label, "Current Label");
        assert_eq!(profile.email.as_deref(), Some("current@example.com"));
        assert_eq!(profile.plan.as_deref(), Some("team"));
        assert_eq!(profile.notes.as_deref(), Some("stale notes"));
    }

    #[test]
    fn save_claude_profile_edits_persists_label_notes_and_enabled() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-05");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000,"subscriptionType":"claude_max"}}"#,
        )
        .unwrap();

        save_claude_profile_edits(
            &profile_dir,
            "profile-05",
            "Edited Claude",
            Some("saved notes".to_string()),
            false,
        )
        .unwrap();

        let profile = load_claude_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.label, "Edited Claude");
        assert_eq!(profile.notes.as_deref(), Some("saved notes"));
        assert!(!profile.enabled);
        assert_eq!(profile.plan.as_deref(), Some("claude_max"));
    }

    #[test]
    fn delete_profile_dir_removes_profile_only_when_present() {
        let temp = tempdir().unwrap();
        let profiles_dir = temp.path().join("profiles");
        let profile_dir = profiles_dir.join("profile-02");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("auth.json"), "{}").unwrap();

        delete_profile_dir(&profile_dir).unwrap();

        assert!(!profile_dir.exists());
        assert!(profiles_dir.exists());

        delete_profile_dir(&profile_dir).unwrap();
        assert!(profiles_dir.exists());
    }

    #[test]
    fn next_profile_id_fills_first_gap() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("profile-01")).unwrap();
        fs::create_dir_all(temp.path().join("profile-03")).unwrap();

        assert_eq!(next_profile_id(temp.path()), "profile-02");
    }

    #[test]
    fn load_codex_profile_returns_clean_for_valid_data() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-10");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"clean@example.com","plan":"pro"}"#,
        )
        .unwrap();

        let outcome = load_codex_profile(&profile_dir).unwrap();
        assert!(matches!(outcome, LoadOutcome::Clean(_)));
        assert_eq!(outcome.value().email.as_deref(), Some("clean@example.com"));
    }

    #[test]
    fn load_codex_profile_returns_degraded_for_corrupted_auth() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-11");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("auth.json"), "{not-valid-json").unwrap();

        let outcome = load_codex_profile(&profile_dir).unwrap();
        match &outcome {
            LoadOutcome::Degraded { warnings, .. } => {
                assert!(warnings.iter().any(|w| w.contains("auth.json")));
            }
            _ => panic!("expected Degraded outcome"),
        }
    }

    #[test]
    fn load_codex_profile_returns_degraded_for_corrupted_profile_metadata() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-12");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"ok@example.com"}"#,
        )
        .unwrap();
        fs::write(profile_dir.join("profile.json"), "{bad json}").unwrap();

        let outcome = load_codex_profile(&profile_dir).unwrap();
        match &outcome {
            LoadOutcome::Degraded { warnings, .. } => {
                assert!(warnings.iter().any(|w| w.contains("profile.json")));
            }
            _ => panic!("expected Degraded outcome"),
        }
    }

    #[test]
    fn load_claude_profile_returns_degraded_for_corrupted_credentials() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profile-13");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join(".credentials.json"), "{bad}").unwrap();

        let outcome = load_claude_profile(&profile_dir).unwrap();
        match &outcome {
            LoadOutcome::Degraded { warnings, .. } => {
                assert!(warnings.iter().any(|w| w.contains(".credentials.json")));
            }
            _ => panic!("expected Degraded outcome"),
        }
    }
}
