use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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
        replace_path(&runtime_sessions_path).with_context(|| {
            format!("failed to prepare {}", runtime_sessions_path.display())
        })?;
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
    #[cfg(windows)]
    {
        &["codex.exe", "codex.cmd", "codex.bat", "codex"]
    }

    #[cfg(not(windows))]
    {
        &["codex"]
    }
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
        Err(err) => return Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }

    Ok(())
}

fn is_runnable_binary(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        return fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }

    #[cfg(windows)]
    {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
        {
            Some(ext) if matches!(ext.as_str(), "exe" | "cmd" | "bat") => true,
            _ => false,
        }
    }
}

#[cfg(unix)]
fn create_dir_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("failed to create symlink {}", link.display()))
}

#[cfg(windows)]
fn create_dir_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
        .with_context(|| format!("failed to create directory symlink {}", link.display()))
}
