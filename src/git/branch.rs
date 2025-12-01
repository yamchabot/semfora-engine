//! Branch detection and management

use std::path::Path;

use crate::error::Result;
use super::{git_command, git_command_optional};

/// Check if the current directory is inside a git repository
pub fn is_git_repo(cwd: Option<&Path>) -> bool {
    git_command_optional(&["rev-parse", "--is-inside-work-tree"], cwd)
        .map(|s| s == "true")
        .unwrap_or(false)
}

/// Get the current branch name
pub fn get_current_branch(cwd: Option<&Path>) -> Result<String> {
    git_command(&["rev-parse", "--abbrev-ref", "HEAD"], cwd)
}

/// Detect the base branch (main or master)
///
/// Priority:
/// 1. Check if 'main' branch exists
/// 2. Check if 'master' branch exists
/// 3. Check for origin/main or origin/master
/// 4. Return error if neither found
pub fn detect_base_branch(cwd: Option<&Path>) -> Result<String> {
    // Check local branches first
    if branch_exists("main", cwd) {
        return Ok("main".to_string());
    }

    if branch_exists("master", cwd) {
        return Ok("master".to_string());
    }

    // Check remote tracking branches
    if remote_branch_exists("origin/main", cwd) {
        return Ok("origin/main".to_string());
    }

    if remote_branch_exists("origin/master", cwd) {
        return Ok("origin/master".to_string());
    }

    // Try to get the default branch from origin
    if let Some(default) = get_origin_default_branch(cwd) {
        return Ok(default);
    }

    Err(crate::error::McpDiffError::GitError {
        message: "Could not detect base branch. No main/master branch found. Use --base to specify.".to_string(),
    })
}

/// Check if a local branch exists
fn branch_exists(name: &str, cwd: Option<&Path>) -> bool {
    git_command_optional(&["show-ref", "--verify", "--quiet", &format!("refs/heads/{}", name)], cwd)
        .is_some()
        || git_command_optional(&["rev-parse", "--verify", name], cwd).is_some()
}

/// Check if a remote branch exists
fn remote_branch_exists(name: &str, cwd: Option<&Path>) -> bool {
    git_command_optional(&["show-ref", "--verify", "--quiet", &format!("refs/remotes/{}", name)], cwd)
        .is_some()
}

/// Try to get the default branch from origin
fn get_origin_default_branch(cwd: Option<&Path>) -> Option<String> {
    // Try to get from remote HEAD
    let result = git_command_optional(&["symbolic-ref", "refs/remotes/origin/HEAD"], cwd)?;

    // Result is like "refs/remotes/origin/main"
    result.strip_prefix("refs/remotes/").map(|s| s.to_string())
}

/// Find the merge base between two refs (common ancestor)
///
/// This is useful for finding where a branch diverged from the base branch.
pub fn get_merge_base(ref1: &str, ref2: &str, cwd: Option<&Path>) -> Result<String> {
    git_command(&["merge-base", ref1, ref2], cwd)
}

/// Get the upstream branch for the current branch (if any)
pub fn get_upstream_branch(cwd: Option<&Path>) -> Option<String> {
    git_command_optional(&["rev-parse", "--abbrev-ref", "@{upstream}"], cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_git_repo() {
        // The localCouncil directory should be a git repo (or not, but the function should work)
        let result = is_git_repo(None);
        // Just verify it returns a boolean without panicking
        let _ = result;
    }

    #[test]
    fn test_detect_base_branch_in_repo() {
        // This test depends on being run in a git repo
        if is_git_repo(None) {
            let result = detect_base_branch(None);
            // Should either find a branch or return an error
            match result {
                Ok(branch) => {
                    assert!(branch.contains("main") || branch.contains("master"));
                }
                Err(_) => {
                    // No main/master, which is fine for some repos
                }
            }
        }
    }
}
