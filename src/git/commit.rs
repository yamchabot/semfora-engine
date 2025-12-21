//! Commit traversal and file retrieval

#![allow(dead_code)]

use std::path::Path;

use super::git_command;
use crate::error::Result;

/// Information about a commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Commit SHA (full)
    pub sha: String,
    /// Short SHA (7 chars)
    pub short_sha: String,
    /// Commit message (first line)
    pub subject: String,
    /// Author name
    pub author: String,
    /// Author date (ISO format)
    pub date: String,
}

/// Get list of commits since a base ref
///
/// Returns commits in reverse chronological order (newest first)
pub fn get_commits_since(base_ref: &str, cwd: Option<&Path>) -> Result<Vec<CommitInfo>> {
    // Format: SHA|short|subject|author|date
    let format = "%H|%h|%s|%an|%aI";
    let output = git_command(
        &[
            "log",
            &format!("--format={}", format),
            &format!("{}..HEAD", base_ref),
        ],
        cwd,
    )?;

    let mut commits = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 {
            continue;
        }

        commits.push(CommitInfo {
            sha: parts[0].to_string(),
            short_sha: parts[1].to_string(),
            subject: parts[2].to_string(),
            author: parts[3].to_string(),
            date: parts[4].to_string(),
        });
    }

    Ok(commits)
}

/// Get the file content at a specific ref
///
/// Returns None if the file doesn't exist at that ref
pub fn get_file_at_ref(
    file_path: &str,
    ref_name: &str,
    cwd: Option<&Path>,
) -> Result<Option<String>> {
    let result =
        super::git_command_optional(&["show", &format!("{}:{}", ref_name, file_path)], cwd);

    Ok(result)
}

/// Get commit count since base ref
pub fn get_commit_count(base_ref: &str, cwd: Option<&Path>) -> Result<usize> {
    let output = git_command(
        &["rev-list", "--count", &format!("{}..HEAD", base_ref)],
        cwd,
    )?;

    output
        .parse()
        .map_err(|_| crate::error::McpDiffError::GitError {
            message: format!("Failed to parse commit count: {}", output),
        })
}

/// Get the parent commit of a given commit
pub fn get_parent_commit(commit: &str, cwd: Option<&Path>) -> Result<String> {
    git_command(&["rev-parse", &format!("{}^", commit)], cwd)
}

/// Get repo root directory
pub fn get_repo_root(cwd: Option<&Path>) -> Result<String> {
    git_command(&["rev-parse", "--show-toplevel"], cwd)
}

/// Get short description of HEAD
pub fn get_head_description(cwd: Option<&Path>) -> Result<String> {
    git_command(&["log", "-1", "--format=%h %s"], cwd)
}

/// Get the last commit information (HEAD)
///
/// Returns CommitInfo for the current HEAD, or None if there are no commits.
/// This is the unified function for CLI/MCP (DEDUP-104).
pub fn get_last_commit(cwd: Option<&Path>) -> Option<CommitInfo> {
    // Format: SHA|short|subject|author|date
    let format = "%H|%h|%s|%an|%aI";
    let output = super::git_command_optional(&["log", "-1", &format!("--format={}", format)], cwd)?;

    let parts: Vec<&str> = output.splitn(5, '|').collect();
    if parts.len() < 5 {
        return None;
    }

    Some(CommitInfo {
        sha: parts[0].to_string(),
        short_sha: parts[1].to_string(),
        subject: parts[2].to_string(),
        author: parts[3].to_string(),
        date: parts[4].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_info_fields() {
        let info = CommitInfo {
            sha: "abc123def456".to_string(),
            short_sha: "abc123d".to_string(),
            subject: "Test commit".to_string(),
            author: "Test Author".to_string(),
            date: "2024-01-01T12:00:00Z".to_string(),
        };

        assert_eq!(info.short_sha, "abc123d");
        assert_eq!(info.subject, "Test commit");
    }
}
