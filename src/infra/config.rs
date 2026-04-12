use std::path::{Path, PathBuf};

use crate::config::Config;

pub fn legacy_data_dir(codex_management_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let new_parent = codex_management_dir.parent()?;
    let share_dir = new_parent.parent()?;
    Some((share_dir.join("ai-overview"), new_parent.to_path_buf()))
}

pub fn legacy_log_dir(log_file: &Path) -> Option<(PathBuf, PathBuf)> {
    let new_log_dir = log_file.parent()?;
    let cache_dir = new_log_dir.parent()?;
    Some((cache_dir.join("ai-overview"), new_log_dir.to_path_buf()))
}

/// One-time migration: if the old `ai-overview` data directory exists and the
/// new `nexus` directory does not, rename it in place so existing profiles and
/// quota cache are preserved across the rename.
pub fn migrate_legacy_dirs(config: &Config) {
    if let Some((old_parent, new_parent)) = legacy_data_dir(&config.codex_management_dir) {
        if old_parent.exists() && !new_parent.exists() {
            match std::fs::rename(&old_parent, &new_parent) {
                Ok(()) => tracing::info!(
                    "migrated data directory from {} to {}",
                    old_parent.display(),
                    new_parent.display()
                ),
                Err(e) => tracing::warn!(
                    "could not migrate {} to {}: {e}",
                    old_parent.display(),
                    new_parent.display()
                ),
            }
        }
    }

    if let Some((old_log_dir, new_log_dir)) = legacy_log_dir(&config.log_file) {
        if old_log_dir.exists() && !new_log_dir.exists() {
            match std::fs::rename(&old_log_dir, &new_log_dir) {
                Ok(()) => tracing::info!(
                    "migrated log directory from {} to {}",
                    old_log_dir.display(),
                    new_log_dir.display()
                ),
                Err(e) => tracing::warn!(
                    "could not migrate log dir {} to {}: {e}",
                    old_log_dir.display(),
                    new_log_dir.display()
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{legacy_data_dir, legacy_log_dir};

    #[test]
    fn legacy_data_dir_rewrites_ai_overview_share_root() {
        let new_dir = Path::new("/home/alice/.local/share/nexus/codex");
        let (old_dir, target_dir) = legacy_data_dir(new_dir).expect("legacy mapping");

        assert_eq!(old_dir, Path::new("/home/alice/.local/share/ai-overview"));
        assert_eq!(target_dir, Path::new("/home/alice/.local/share/nexus"));
    }

    #[test]
    fn legacy_log_dir_rewrites_ai_overview_cache_root() {
        let new_file = Path::new("/home/alice/.cache/nexus/nexus.log");
        let (old_dir, target_dir) = legacy_log_dir(new_file).expect("legacy mapping");

        assert_eq!(old_dir, Path::new("/home/alice/.cache/ai-overview"));
        assert_eq!(target_dir, Path::new("/home/alice/.cache/nexus"));
    }
}
