use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AuthMetadata {
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexProfile {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub profile_dir: PathBuf,
    pub auth_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileMetadata {
    pub id: String,
    pub label: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl CodexProfile {
    pub fn import_from_auth(
        id: &str,
        label: &str,
        profile_dir: PathBuf,
        auth_source: &Path,
    ) -> Result<Self> {
        if id.trim().is_empty() {
            bail!("id cannot be empty or whitespace");
        }
        if label.trim().is_empty() {
            bail!("label cannot be empty or whitespace");
        }

        let content = fs::read_to_string(auth_source)
            .with_context(|| format!("failed to read {}", auth_source.display()))?;
        let json: Value = serde_json::from_str(&content).context("invalid auth.json")?;
        let metadata = extract_auth_metadata(&json);

        fs::create_dir_all(&profile_dir)
            .with_context(|| format!("failed to create {}", profile_dir.display()))?;

        let auth_path = profile_dir.join("auth.json");
        fs::copy(auth_source, &auth_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                auth_source.display(),
                auth_path.display()
            )
        })?;

        let now = Utc::now();

        Ok(Self {
            id: id.to_string(),
            label: label.to_string(),
            email: metadata.email,
            plan: metadata.plan,
            notes: None,
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir,
            auth_path,
        })
    }
}

fn decode_jwt_payload(token: &str) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let payload = parts[1];
    let padding = (4 - payload.len() % 4) % 4;
    let padded = format!("{}{}", payload, "=".repeat(padding));

    use base64::Engine;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&padded)
        .ok()?;

    serde_json::from_slice(&decoded).ok()
}

pub fn extract_auth_metadata(value: &Value) -> AuthMetadata {
    let email = value
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let tokens = value.get("tokens")?;
            let id_token = tokens.get("id_token")?.as_str()?;
            let payload = decode_jwt_payload(id_token)?;
            payload.get("email")?.as_str().map(str::to_string)
        });

    let plan = value
        .get("plan")
        .or_else(|| value.get("subscription"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let tokens = value.get("tokens")?;
            let id_token = tokens.get("id_token")?.as_str()?;
            let payload = decode_jwt_payload(id_token)?;
            let auth = payload.get("https://api.openai.com/auth")?;
            auth.get("chatgpt_plan_type")?.as_str().map(str::to_string)
        });

    AuthMetadata { email, plan }
}

pub fn load_profile_metadata(profile_dir: &Path) -> Result<ProfileMetadata> {
    let metadata_path = profile_dir.join("profile.json");
    if !metadata_path.is_file() {
        return Ok(ProfileMetadata::default());
    }
    let content = fs::read_to_string(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    let metadata: ProfileMetadata = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
    Ok(metadata)
}

pub fn save_profile_metadata(profile_dir: &Path, metadata: &ProfileMetadata) -> Result<()> {
    let metadata_path = profile_dir.join("profile.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata_path, content)
        .with_context(|| format!("failed to write {}", metadata_path.display()))?;
    Ok(())
}

/// Detects which stored profile is currently "live" by comparing
/// `~/.codex/auth.json` bytes against each profile's `auth.json`.
/// Returns the profile ID of the first match, or `None` if no match
/// or if `codex_home_auth` cannot be read.
pub fn detect_live_profile_id(profiles_dir: &Path, codex_home_auth: &Path) -> Option<String> {
    let home_bytes = fs::read(codex_home_auth).ok()?;

    let entries = fs::read_dir(profiles_dir).ok()?;
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();

    for id in ids {
        let profile_auth = profiles_dir.join(&id).join("auth.json");
        if let Ok(profile_bytes) = fs::read(&profile_auth) {
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

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_detect_live_profile_id_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"a@b.com\"}").unwrap();

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, Some("profile-01".to_string()));
    }

    #[test]
    fn test_detect_live_profile_id_no_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"other@b.com\"}").unwrap();

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_live_profile_id_missing_home_auth() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("nonexistent-auth.json");

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_live_profile_id_empty_profiles_dir() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let home_auth = tmp.path().join("auth.json");
        write_file(tmp.path(), "auth.json", b"{\"email\":\"a@b.com\"}");

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }
}
