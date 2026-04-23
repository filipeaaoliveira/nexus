use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::codex_profiles::profile::{self as codex_profile};
use crate::profiles::{io as profile_io, ProfileMetadata, StoredProfile};

use super::storage;

pub fn import_profile_from_home(profiles_dir: &Path, home_dir: &Path) -> Result<StoredProfile> {
    let auth_source = home_dir.join(".codex").join("auth.json");
    if !auth_source.is_file() {
        bail!("{} does not exist", auth_source.display());
    }
    import_profile_from_auth(profiles_dir, &auth_source)
}

pub fn import_profile_from_auth(profiles_dir: &Path, auth_source: &Path) -> Result<StoredProfile> {
    fs::create_dir_all(profiles_dir)
        .with_context(|| format!("failed to create {}", profiles_dir.display()))?;

    let profile_id = storage::next_profile_id(profiles_dir);
    let profile_dir = profiles_dir.join(&profile_id);
    let profile =
        codex_profile::import_from_auth(&profile_id, &profile_id, &profile_dir, auth_source)?;

    let metadata = ProfileMetadata {
        id: profile.id.clone(),
        label: Some(profile.label.clone()),
        email: profile.email.clone(),
        plan: profile.plan.clone(),
        notes: profile.notes.clone(),
        enabled: profile.enabled,
        created_at: Some(profile.created_at),
        updated_at: Some(profile.updated_at),
    };
    profile_io::save_profile_metadata(&profile_dir, &metadata)?;

    Ok(profile)
}

pub fn write_profile_to_home(profile_dir: &Path, home_dir: &Path) -> Result<()> {
    let source_auth = profile_dir.join("auth.json");
    if !source_auth.is_file() {
        bail!("profile auth.json not found at {}", source_auth.display());
    }

    let target_dir = home_dir.join(".codex");
    let target_auth = target_dir.join("auth.json");
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;
    fs::copy(&source_auth, &target_auth).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source_auth.display(),
            target_auth.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::services::accounts::codex::{import_profile_from_auth, write_profile_to_home};
    use crate::services::accounts::storage::load_codex_profile;

    #[test]
    fn import_profile_from_auth_copies_auth_and_profile_metadata() {
        let temp = tempdir().unwrap();
        let profiles_dir = temp.path().join("profiles");
        let auth_source = temp.path().join("auth.json");
        fs::create_dir_all(&profiles_dir).unwrap();
        fs::write(
            &auth_source,
            r#"{"email":"imported@example.com","plan":"plus"}"#,
        )
        .unwrap();

        let profile = import_profile_from_auth(&profiles_dir, &auth_source).unwrap();
        let profile_dir = profiles_dir.join(&profile.id);
        let loaded = load_codex_profile(&profile_dir).unwrap().into_value();

        assert_eq!(profile.id, "profile-01");
        assert!(profile_dir.join("auth.json").is_file());
        assert_eq!(loaded.label, "profile-01");
        assert_eq!(loaded.email.as_deref(), Some("imported@example.com"));
        assert_eq!(loaded.plan.as_deref(), Some("plus"));
        assert!(loaded.enabled);
    }

    #[test]
    fn write_profile_to_home_copies_auth_json_into_codex_dir() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profiles").join("profile-09");
        let home_dir = temp.path().join("home");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"write@example.com"}"#,
        )
        .unwrap();

        write_profile_to_home(&profile_dir, &home_dir).unwrap();

        let target = home_dir.join(".codex").join("auth.json");
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            r#"{"email":"write@example.com"}"#
        );
    }
}
