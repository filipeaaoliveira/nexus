use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::ProfileMetadata;

pub fn load_profile_metadata(profile_dir: &Path) -> Result<ProfileMetadata> {
    let metadata_path = profile_dir.join("profile.json");
    if !metadata_path.is_file() {
        return Ok(ProfileMetadata::default());
    }
    let content = fs::read_to_string(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", metadata_path.display()))
}

pub fn save_profile_metadata(profile_dir: &Path, metadata: &ProfileMetadata) -> Result<()> {
    let metadata_path = profile_dir.join("profile.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata_path, content)
        .with_context(|| format!("failed to write {}", metadata_path.display()))
}
