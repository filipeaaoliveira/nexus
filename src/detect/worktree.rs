use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_worktree: bool,
}

pub fn is_git_worktree(dir: &Path) -> bool {
    let git_path = dir.join(".git");
    git_path.is_file()
}

pub async fn get_repo_root(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(PathBuf::from(path))
}

pub fn parse_worktree_list(output: &str) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head = String::new();
    let mut current_branch: Option<String> = None;
    let mut is_first = true;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(prev_path) = current_path.take() {
                worktrees.push(WorktreeInfo {
                    path: prev_path,
                    head: std::mem::take(&mut current_head),
                    branch: current_branch.take(),
                    is_worktree: !is_first,
                });
                is_first = false;
            } else {
                is_first = true;
            }
            current_path = Some(PathBuf::from(path));
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            current_head = head.to_string();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        }
    }

    if let Some(path) = current_path {
        worktrees.push(WorktreeInfo {
            path,
            head: current_head,
            branch: current_branch,
            is_worktree: !is_first,
        });
    }

    worktrees
}

pub async fn scan_worktrees(repo_dir: &Path) -> Vec<WorktreeInfo> {
    let output = match Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_dir)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to list worktrees for {}: {e}", repo_dir.display());
            return Vec::new();
        }
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_list(&stdout)
}

pub async fn lookup_branch_info(cwd: &Path) -> (Option<String>, bool) {
    let is_wt = is_git_worktree(cwd);
    let branch = match Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if b == "HEAD" {
                None
            } else {
                Some(b)
            }
        }
        _ => None,
    };
    (branch, is_wt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list() {
        let output = "\
worktree /Users/dev/myproject
HEAD abc123def456
branch refs/heads/main

worktree /Users/dev/myproject-feat
HEAD def456abc789
branch refs/heads/feat/new-feature

worktree /Users/dev/myproject-fix
HEAD 111222333444
branch refs/heads/fix/bug-123
";
        let wts = parse_worktree_list(output);
        assert_eq!(wts.len(), 3);
        assert_eq!(wts[0].path, PathBuf::from("/Users/dev/myproject"));
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(!wts[0].is_worktree);
        assert_eq!(wts[1].path, PathBuf::from("/Users/dev/myproject-feat"));
        assert_eq!(wts[1].branch.as_deref(), Some("feat/new-feature"));
        assert!(wts[1].is_worktree);
        assert_eq!(wts[2].path, PathBuf::from("/Users/dev/myproject-fix"));
        assert_eq!(wts[2].branch.as_deref(), Some("fix/bug-123"));
        assert!(wts[2].is_worktree);
    }

    #[test]
    fn test_parse_worktree_list_empty() {
        let wts = parse_worktree_list("");
        assert!(wts.is_empty());
    }

    #[test]
    fn test_parse_worktree_list_single() {
        let output = "worktree /Users/dev/project\nHEAD abc123\nbranch refs/heads/main\n";
        let wts = parse_worktree_list(output);
        assert_eq!(wts.len(), 1);
        assert!(!wts[0].is_worktree);
    }
}
