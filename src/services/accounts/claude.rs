use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use tempfile::TempPath;

use crate::claude_profiles::profile::{self as claude_profile};
use crate::profiles::{io as profile_io, ProfileMetadata, StoredProfile};
use crate::runtime_support::run_blocking_compatible;

use super::storage;

pub fn import_profile_from_home(profiles_dir: &Path, home_dir: &Path) -> Result<StoredProfile> {
    let (credentials_source, _temp_path) = credentials_source_from_home(home_dir)?;
    import_profile_from_credentials(profiles_dir, &credentials_source)
}

pub fn import_profile_from_credentials(
    profiles_dir: &Path,
    credentials_source: &Path,
) -> Result<StoredProfile> {
    fs::create_dir_all(profiles_dir)
        .with_context(|| format!("failed to create {}", profiles_dir.display()))?;

    let profile_id = storage::next_profile_id(profiles_dir);
    let profile_dir = profiles_dir.join(&profile_id);
    let profile = claude_profile::import_from_credentials(
        &profile_id,
        &profile_id,
        &profile_dir,
        credentials_source,
    )?;

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
    let source = profile_dir.join(".credentials.json");
    if !source.is_file() {
        bail!(
            "profile .credentials.json not found at {}",
            source.display()
        );
    }

    let target = home_dir.join(".claude").join(".credentials.json");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&source, &target).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

fn credentials_source_from_home(home_dir: &Path) -> Result<(std::path::PathBuf, Option<TempPath>)> {
    let creds_file = home_dir.join(".claude").join(".credentials.json");
    if creds_file.is_file() {
        return Ok((creds_file, None));
    }

    let run_security = || {
        std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-l",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
    };
    let output = run_blocking_compatible(run_security).context("failed to run security CLI")?;
    if !output.status.success() {
        bail!("No Claude credentials found. Run `claude` once to log in first.");
    }

    let json = String::from_utf8(output.stdout)
        .context("keychain output is not valid UTF-8")?
        .trim()
        .to_string();
    let temp_file =
        tempfile::NamedTempFile::new().context("failed to create temp file for credentials")?;
    fs::write(temp_file.path(), json.as_bytes())
        .context("failed to write credentials to temp file")?;
    let temp_path = temp_file.into_temp_path();
    Ok((temp_path.to_path_buf(), Some(temp_path)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::profiles::io as profile_io;
    use crate::services::accounts::claude::{
        import_profile_from_credentials, write_profile_to_home,
    };

    #[test]
    fn import_profile_from_credentials_copies_credentials_and_profile_metadata() {
        let temp = tempdir().unwrap();
        let profiles_dir = temp.path().join("profiles");
        let creds_source = temp.path().join(".credentials.json");
        fs::create_dir_all(&profiles_dir).unwrap();
        fs::write(
            &creds_source,
            r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000,"subscriptionType":"claude_max"}}"#,
        )
        .unwrap();

        let profile = import_profile_from_credentials(&profiles_dir, &creds_source).unwrap();
        let profile_dir = profiles_dir.join(&profile.id);
        let metadata = profile_io::load_profile_metadata(&profile_dir).unwrap();

        assert_eq!(profile.id, "profile-01");
        assert!(profile_dir.join(".credentials.json").is_file());
        assert_eq!(metadata.label.as_deref(), Some("profile-01"));
        assert_eq!(metadata.plan.as_deref(), Some("claude_max"));
        assert!(metadata.enabled);
    }

    #[test]
    fn write_profile_to_home_copies_credentials_into_claude_dir() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profiles").join("profile-03");
        let home_dir = temp.path().join("home");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000}}"#,
        )
        .unwrap();

        write_profile_to_home(&profile_dir, &home_dir).unwrap();

        let target = home_dir.join(".claude").join(".credentials.json");
        assert!(target.is_file());
        assert!(fs::read_to_string(target)
            .unwrap()
            .contains("\"accessToken\":\"tok\""));
    }
}
