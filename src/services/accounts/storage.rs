use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use crate::claude_profiles::auth::ClaudeAuth;
use crate::claude_profiles::profile::{
    load_claude_profile_metadata, save_claude_profile_metadata, ClaudeProfile,
    ClaudeProfileMetadata,
};
use crate::codex_profiles::profile::{
    extract_auth_metadata, load_profile_metadata, save_profile_metadata, CodexProfile,
    ProfileMetadata,
};

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

pub fn load_codex_profile(profile_dir: &Path) -> Result<CodexProfile> {
    let profile_id = profile_id_from_dir(profile_dir)?;
    let auth_path = profile_dir.join("auth.json");
    let auth_content = fs::read_to_string(&auth_path)
        .with_context(|| format!("failed to read {}", auth_path.display()))?;
    let auth_json: Value = serde_json::from_str(&auth_content)
        .with_context(|| format!("failed to parse {}", auth_path.display()))?;
    let auth_metadata = extract_auth_metadata(&auth_json);
    let metadata = load_profile_metadata(profile_dir)?;
    let now = Utc::now();

    Ok(CodexProfile {
        id: profile_id.clone(),
        label: metadata.label.unwrap_or(profile_id.clone()),
        email: metadata.email.or(auth_metadata.email),
        plan: metadata.plan.or(auth_metadata.plan),
        notes: metadata.notes,
        enabled: metadata.enabled,
        last_used: None,
        created_at: metadata.created_at.unwrap_or(now),
        updated_at: metadata.updated_at.unwrap_or(now),
        profile_dir: profile_dir.to_path_buf(),
        auth_path,
    })
}

pub fn load_claude_profile(profile_dir: &Path) -> Result<ClaudeProfile> {
    let profile_id = profile_id_from_dir(profile_dir)?;
    let auth_path = profile_dir.join(".credentials.json");
    let auth = ClaudeAuth::load(&auth_path).unwrap_or_default();
    let metadata = load_claude_profile_metadata(profile_dir)?;
    let now = Utc::now();

    Ok(ClaudeProfile {
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
        profile_dir: profile_dir.to_path_buf(),
        auth_path,
    })
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
    save_profile_metadata(profile_dir, &metadata)
}

pub fn save_claude_profile_edits(
    profile_dir: &Path,
    profile_id: &str,
    label: &str,
    notes: Option<String>,
    enabled: bool,
) -> Result<()> {
    let existing = load_claude_profile_metadata(profile_dir).unwrap_or_default();
    let metadata = ClaudeProfileMetadata {
        id: profile_id.to_string(),
        label: Some(label.to_string()),
        email: existing.email,
        plan: existing.plan,
        notes,
        enabled,
        created_at: existing.created_at,
        updated_at: Some(Utc::now()),
    };
    save_claude_profile_metadata(profile_dir, &metadata)
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
        delete_profile_dir, load_codex_profile, next_profile_id, save_codex_profile_metadata,
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

        let profile = load_codex_profile(&profile_dir).unwrap();

        assert_eq!(profile.id, "profile-01");
        assert_eq!(profile.label, "Main Profile");
        assert_eq!(profile.email.as_deref(), Some("person@example.com"));
        assert_eq!(profile.plan.as_deref(), Some("pro"));
        assert_eq!(profile.notes.as_deref(), Some("notes from metadata"));
        assert!(!profile.enabled);
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
}
