use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::ProfileMetadata;

#[derive(Debug, Error)]
pub enum ProfileMetadataError {
    #[error("failed to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("profile metadata corruption in {}: {source}", path.display())]
    Corrupt {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to serialize profile metadata for {}: {source}", path.display())]
    Serialize {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, ProfileMetadataError>;

pub fn load_profile_metadata(profile_dir: &Path) -> Result<ProfileMetadata> {
    let metadata_path = profile_dir.join("profile.json");
    if !metadata_path.is_file() {
        return Ok(ProfileMetadata::default());
    }
    let content =
        fs::read_to_string(&metadata_path).map_err(|source| ProfileMetadataError::Read {
            path: metadata_path.clone(),
            source,
        })?;
    serde_json::from_str(&content).map_err(|source| ProfileMetadataError::Corrupt {
        path: metadata_path,
        source,
    })
}

pub fn save_profile_metadata(profile_dir: &Path, metadata: &ProfileMetadata) -> Result<()> {
    let metadata_path = profile_dir.join("profile.json");
    let content = serde_json::to_string_pretty(metadata).map_err(|source| {
        ProfileMetadataError::Serialize {
            path: metadata_path.clone(),
            source,
        }
    })?;
    fs::write(&metadata_path, content).map_err(|source| ProfileMetadataError::Write {
        path: metadata_path,
        source,
    })
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
