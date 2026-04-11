use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::codex_profiles::quota::{parse_legacy_status_output, QuotaSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePlan {
    pub profile_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub runtime_home: PathBuf,
    pub auth_path: PathBuf,
    pub sessions_path: PathBuf,
    pub sqlite_home: PathBuf,
}

impl RuntimePlan {
    pub fn new(profile_dir: PathBuf, repo_dir: PathBuf, runtime_home: PathBuf) -> Self {
        let auth_path = profile_dir.join("auth.json");
        let sessions_path = repo_dir.join("sessions");
        let sqlite_home = repo_dir.join("sqlite");

        Self {
            profile_dir,
            repo_dir,
            runtime_home,
            auth_path,
            sessions_path,
            sqlite_home,
        }
    }

    pub fn materialize(&self) -> Result<()> {
        fs::create_dir_all(&self.runtime_home)
            .with_context(|| format!("failed to create {}", self.runtime_home.display()))?;
        fs::create_dir_all(&self.sessions_path)
            .with_context(|| format!("failed to create {}", self.sessions_path.display()))?;
        fs::create_dir_all(&self.sqlite_home)
            .with_context(|| format!("failed to create {}", self.sqlite_home.display()))?;

        let runtime_auth_path = self.runtime_home.join("auth.json");
        fs::copy(&self.auth_path, &runtime_auth_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                self.auth_path.display(),
                runtime_auth_path.display()
            )
        })?;

        let runtime_sessions_path = self.runtime_home.join("sessions");
        replace_path(&runtime_sessions_path)
            .with_context(|| format!("failed to prepare {}", runtime_sessions_path.display()))?;
        create_dir_symlink(&self.sessions_path, &runtime_sessions_path).with_context(|| {
            format!(
                "failed to link {} to {}",
                runtime_sessions_path.display(),
                self.sessions_path.display()
            )
        })?;

        Ok(())
    }
}

pub fn canonical_repo_root(cwd: &Path) -> Result<PathBuf> {
    let resolved = git_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    normalize_repo_path(&resolved)
}

pub fn repo_runtime_key(repo_root: &Path) -> String {
    let normalized = normalize_repo_path(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let repo_name = normalized
        .file_name()
        .and_then(|name| name.to_str())
        .map(filesystem_safe_segment)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "repo".to_string());
    let hash = stable_path_hash(&normalized);

    format!("{repo_name}-{hash:08x}")
}

pub fn require_codex_binary(path_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = path_override {
        if is_runnable_binary(path) {
            return Ok(path.to_path_buf());
        }

        return Err(anyhow!("codex binary override is not executable"));
    }

    let path_var = env::var_os("PATH").ok_or_else(|| anyhow!("PATH is not set"))?;
    let candidates = codex_binary_names();

    for directory in env::split_paths(&path_var) {
        for candidate_name in candidates {
            let candidate = directory.join(candidate_name);
            if is_runnable_binary(&candidate) {
                return Ok(candidate);
            }
        }
    }

    Err(anyhow!("codex binary not found in PATH"))
}

pub fn launch_codex(
    codex_bin: &Path,
    runtime: &RuntimePlan,
    args: &[String],
    working_dir: &Path,
) -> Result<ExitStatus> {
    runtime.materialize()?;

    Command::new(codex_bin)
        .arg("-c")
        .arg(format!("sqlite_home={}", runtime.sqlite_home.display()))
        .args(args)
        .env("CODEX_HOME", &runtime.runtime_home)
        .current_dir(working_dir)
        .status()
        .context("failed to spawn codex")
}

fn codex_binary_names() -> &'static [&'static str] {
    &["codex"]
}

fn replace_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("failed to remove {}", path.display()))?;
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("failed to remove {}", path.display()))?;
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err).with_context(|| format!("failed to inspect {}", path.display()))
        }
    }

    Ok(())
}

fn is_runnable_binary(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn create_dir_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("failed to create symlink {}", link.display()))
}

pub fn get_codex_quota(codex_bin: &Path, runtime: &RuntimePlan) -> Result<Option<QuotaSnapshot>> {
    let output = Command::new(codex_bin)
        .arg("-c")
        .arg(format!("sqlite_home={}", runtime.sqlite_home.display()))
        .arg("status")
        .env("CODEX_HOME", &runtime.runtime_home)
        .output()
        .context("failed to run codex status")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_legacy_status_output(&stdout))
}

fn git_repo_root(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let root = stdout.trim();
    if root.is_empty() {
        return None;
    }

    Some(PathBuf::from(root))
}

fn normalize_repo_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("failed to resolve current working directory")?
            .join(path)
    };

    match absolute.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) => Ok(absolute),
    }
}

fn filesystem_safe_segment(input: &str) -> String {
    let mut segment = String::new();
    let mut last_was_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            segment.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            segment.push('-');
            last_was_dash = true;
        }
    }

    segment.trim_matches('-').to_string()
}

fn stable_path_hash(path: &Path) -> u32 {
    const FNV_OFFSET: u32 = 0x811c9dc5;
    const FNV_PRIME: u32 = 0x01000193;

    let mut hash = FNV_OFFSET;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}
