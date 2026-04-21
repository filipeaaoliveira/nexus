use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs;
use std::path::Path;

use crate::claude_profiles::auth::ClaudeAuth;

pub use crate::profiles::StoredProfile as StoredClaudeProfile;

pub use crate::profiles::ProfileMetadata as ClaudeProfileMetadata;

pub fn import_from_credentials(
    id: &str,
    label: &str,
    profile_dir: &Path,
    credentials_source: &Path,
) -> Result<StoredClaudeProfile> {
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

    Ok(StoredClaudeProfile {
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

pub fn load_claude_profile_metadata(profile_dir: &Path) -> Result<ClaudeProfileMetadata> {
    crate::profiles::io::load_profile_metadata(profile_dir)
}

pub fn save_claude_profile_metadata(
    profile_dir: &Path,
    metadata: &ClaudeProfileMetadata,
) -> Result<()> {
    crate::profiles::io::save_profile_metadata(profile_dir, metadata)
}

/// Detects which stored Claude profile is currently "live" by comparing
/// `~/.claude/.credentials.json` bytes against each profile's `.credentials.json`.
pub fn detect_live_claude_profile_id(
    profiles_dir: &Path,
    claude_home_creds: &Path,
) -> Option<String> {
    crate::profiles::detection::detect_live_profile_id(
        profiles_dir,
        claude_home_creds,
        ".credentials.json",
    )
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
        let profile = import_from_credentials(
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
            import_from_credentials("   ", "label", tmp.path(), &source);
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
