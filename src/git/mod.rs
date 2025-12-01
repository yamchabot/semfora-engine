//! Git operations for diff analysis
//!
//! This module provides git integration for analyzing diffs between branches
//! and commits. It uses subprocess calls to git for maximum compatibility.

mod branch;
mod diff;
mod commit;

pub use branch::{detect_base_branch, get_current_branch, get_merge_base, is_git_repo};
pub use diff::{get_changed_files, get_commit_changed_files, ChangedFile, ChangeType};
pub use commit::{get_commits_since, get_file_at_ref, get_parent_commit, get_repo_root, CommitInfo};

use std::path::Path;
use std::process::Command;

use crate::error::{McpDiffError, Result};

/// Run a git command and return stdout as string
pub fn git_command(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output().map_err(|e| McpDiffError::GitError {
        message: format!("Failed to execute git: {}", e),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(McpDiffError::GitError {
            message: format!("git {} failed: {}", args.join(" "), stderr.trim()),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a git command, returning None if it fails (for optional queries)
pub fn git_command_optional(args: &[&str], cwd: Option<&Path>) -> Option<String> {
    git_command(args, cwd).ok()
}
