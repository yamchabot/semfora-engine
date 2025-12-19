//! Worktree auto-discovery via git
//!
//! Uses `git worktree list --porcelain` to discover all worktrees for a repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::socket_server::protocol::WorktreeInfo;

/// Discover all worktrees for a repository
pub fn discover_worktrees(repo_path: &Path) -> anyhow::Result<Vec<WorktreeInfo>> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["worktree", "list", "--porcelain"])
        .output()?;

    if !output.status.success() {
        // Not a git repo or git error - return empty list
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_output(&stdout)
}

/// Get the default branch (main/master) for a repository
pub fn get_default_branch(repo_path: &Path) -> anyhow::Result<String> {
    // Try to get the default branch from remote
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout);
        if let Some(name) = branch.trim().strip_prefix("refs/remotes/origin/") {
            return Ok(name.to_string());
        }
    }

    // Fall back to checking for main/master
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["branch", "-l", "main", "master"])
        .output()?;

    let branches = String::from_utf8_lossy(&output.stdout);
    if branches.contains("main") {
        Ok("main".to_string())
    } else if branches.contains("master") {
        Ok("master".to_string())
    } else {
        // Default to main
        Ok("main".to_string())
    }
}

/// Get the current branch for a repository or worktree
pub fn get_current_branch(repo_path: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["branch", "--show-current"])
        .output()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Ok(branch);
        }
    }

    // Detached HEAD - get the commit SHA
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--short", "HEAD"])
        .output()?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get HEAD commit SHA
pub fn get_head_sha(repo_path: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "HEAD"])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(anyhow::anyhow!("Failed to get HEAD SHA"))
    }
}

/// Parse the porcelain output of `git worktree list`
fn parse_worktree_output(output: &str) -> anyhow::Result<Vec<WorktreeInfo>> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut counter = 0;

    for line in output.lines() {
        if line.starts_with("worktree ") {
            // Save previous worktree if complete
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                let branch = current_branch
                    .take()
                    .unwrap_or_else(|| "detached".to_string());
                let is_semfora = branch.starts_with("semfora/");
                counter += 1;
                worktrees.push(WorktreeInfo {
                    id: format!("wt_{:03}", counter),
                    path,
                    branch,
                    is_semfora,
                    head_sha: head,
                });
            }

            current_path = Some(PathBuf::from(line.strip_prefix("worktree ").unwrap()));
        } else if line.starts_with("HEAD ") {
            current_head = Some(line.strip_prefix("HEAD ").unwrap().to_string());
        } else if line.starts_with("branch ") {
            let branch = line.strip_prefix("branch ").unwrap();
            // Strip refs/heads/ prefix
            current_branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line.starts_with("detached") {
            current_branch = Some("detached".to_string());
        }
    }

    // Don't forget the last worktree
    if let (Some(path), Some(head)) = (current_path, current_head) {
        let branch = current_branch.unwrap_or_else(|| "detached".to_string());
        let is_semfora = branch.starts_with("semfora/");
        counter += 1;
        worktrees.push(WorktreeInfo {
            id: format!("wt_{:03}", counter),
            path,
            branch,
            is_semfora,
            head_sha: head,
        });
    }

    Ok(worktrees)
}

/// Check if a path is a git repository
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the repository root path
pub fn get_repo_root(path: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()?;

    if output.status.success() {
        Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim(),
        ))
    } else {
        Err(anyhow::anyhow!("Not a git repository"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_output() {
        let output = r#"worktree /home/user/project
HEAD abc123def456
branch refs/heads/main

worktree /home/user/project-wt
HEAD def789abc123
branch refs/heads/semfora/feature-auth

"#;

        let worktrees = parse_worktree_output(output).unwrap();
        assert_eq!(worktrees.len(), 2);

        assert_eq!(worktrees[0].path, PathBuf::from("/home/user/project"));
        assert_eq!(worktrees[0].branch, "main");
        assert!(!worktrees[0].is_semfora);

        assert_eq!(worktrees[1].path, PathBuf::from("/home/user/project-wt"));
        assert_eq!(worktrees[1].branch, "semfora/feature-auth");
        assert!(worktrees[1].is_semfora);
    }
}
