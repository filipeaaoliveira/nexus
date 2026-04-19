use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::claude_profiles::auth::ClaudeAuth;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredClaudeProfile {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProfileMetadata {
    pub id: String,
    pub label: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for ClaudeProfileMetadata {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: None,
            email: None,
            plan: None,
            notes: None,
            enabled: true,
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<StoredClaudeProfile> for nexus_domain::accounts::DomainProfile {
    fn from(p: StoredClaudeProfile) -> Self {
        Self {
            id: nexus_domain::accounts::ProfileId::new(p.id),
            provider: nexus_domain::accounts::Provider::Claude,
            label: p.label,
            email: p.email,
            plan: p.plan,
            notes: p.notes,
            enabled: p.enabled,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

impl StoredClaudeProfile {
    pub fn import_from_credentials(
        id: &str,
        label: &str,
        profile_dir: &Path,
        credentials_source: &Path,
    ) -> Result<Self> {
        if id.trim().is_empty() {
            bail!("id cannot be empty or whitespace");
        }
        if label.trim().is_empty() {
            bail!("label cannot be empty or whitespace");
        }

        let auth = ClaudeAuth::load(credentials_source)?;

        fs::create_dir_all(profile_dir)
            .with_context(|| format!("failed to create {}", profile_dir.display()))?;

        let auth_path = profile_dir.join(".credentials.json");
        fs::copy(credentials_source, &auth_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                credentials_source.display(),
                auth_path.display()
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", auth_path.display()))?;
        }

        let now = Utc::now();
        let plan = auth.subscription_type().map(str::to_string);

        Ok(Self {
            id: id.to_string(),
            label: label.to_string(),
            email: None,
            plan,
            notes: None,
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        })
    }
}

pub fn load_claude_profile_metadata(profile_dir: &Path) -> Result<ClaudeProfileMetadata> {
    let metadata_path = profile_dir.join("profile.json");
    if !metadata_path.is_file() {
        return Ok(ClaudeProfileMetadata::default());
    }
    let content = fs::read_to_string(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", metadata_path.display()))
}

pub fn save_claude_profile_metadata(
    profile_dir: &Path,
    metadata: &ClaudeProfileMetadata,
) -> Result<()> {
    let metadata_path = profile_dir.join("profile.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata_path, content)
        .with_context(|| format!("failed to write {}", metadata_path.display()))
}

/// Detects which stored Claude profile is currently "live" by comparing
/// `~/.claude/.credentials.json` bytes against each profile's `.credentials.json`.
pub fn detect_live_claude_profile_id(
    profiles_dir: &Path,
    claude_home_creds: &Path,
) -> Option<String> {
    let home_bytes = fs::read(claude_home_creds).ok()?;

    let entries = fs::read_dir(profiles_dir).ok()?;
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();

    for id in ids {
        let profile_creds = profiles_dir.join(&id).join(".credentials.json");
        if let Ok(profile_bytes) = fs::read(&profile_creds) {
            if profile_bytes == home_bytes {
                return Some(id);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_import_from_credentials_creates_profile_dir() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("source.json");
        fs::write(
            &source,
            r#"{"claudeAiOauth":{"accessToken":"t","refreshToken":"r","expiresAt":9999999999000,"subscriptionType":"claude_max"}}"#,
        )
        .unwrap();

        let profile_dir = tmp.path().join("profiles").join("profile-01");
        let profile = StoredClaudeProfile::import_from_credentials(
            "profile-01",
            "My Claude",
            &profile_dir,
            &source,
        )
        .unwrap();

        assert!(profile_dir.join(".credentials.json").is_file());
        assert_eq!(profile.plan.as_deref(), Some("claude_max"));
        assert!(profile.enabled);
    }

    #[test]
    fn test_import_rejects_empty_id() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("c.json");
        fs::write(&source, r#"{}"#).unwrap();
        let result =
            StoredClaudeProfile::import_from_credentials("   ", "label", tmp.path(), &source);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_live_profile_id_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join(".credentials.json"), b"creds").unwrap();

        let home_creds = tmp.path().join(".credentials.json");
        fs::write(&home_creds, b"creds").unwrap();

        assert_eq!(
            detect_live_claude_profile_id(&profiles_dir, &home_creds),
            Some("profile-01".to_string())
        );
    }

    #[test]
    fn test_detect_live_profile_id_no_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join(".credentials.json"), b"creds-a").unwrap();

        let home_creds = tmp.path().join(".credentials.json");
        fs::write(&home_creds, b"creds-b").unwrap();

        assert_eq!(
            detect_live_claude_profile_id(&profiles_dir, &home_creds),
            None
        );
    }

    #[test]
    fn test_metadata_default_enabled_true() {
        let m = ClaudeProfileMetadata::default();
        assert!(m.enabled);
    }

    #[test]
    fn test_load_metadata_absent_file_enabled_true() {
        let tmp = TempDir::new().unwrap();
        let result = load_claude_profile_metadata(tmp.path()).unwrap();
        assert!(result.enabled);
    }
}
