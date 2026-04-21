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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_metadata_has_enabled_true() {
        let m = ProfileMetadata::default();
        assert!(m.enabled);
    }

    #[test]
    fn absent_file_returns_default_with_enabled_true() {
        let tmp = TempDir::new().unwrap();
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(result.enabled);
    }

    #[test]
    fn missing_enabled_field_defaults_to_true() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("profile.json"), b"{\"id\":\"x\"}").unwrap();
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(result.enabled);
    }

    #[test]
    fn explicit_false_preserved() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("profile.json"),
            b"{\"id\":\"x\",\"enabled\":false}",
        )
        .unwrap();
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(!result.enabled);
    }
}
