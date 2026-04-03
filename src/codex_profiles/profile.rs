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

pub fn extract_auth_metadata(value: &Value) -> AuthMetadata {
    let email = value
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan = value
        .get("plan")
        .or_else(|| value.get("subscription"))
        .and_then(Value::as_str)
        .map(str::to_string);

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
