//! Git diff operations

use std::path::Path;

use crate::error::{McpDiffError, Result};
use super::git_command;

/// Type of change to a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// File was added
    Added,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed
    Renamed,
    /// File was copied
    Copied,
    /// Type changed (e.g., file to symlink)
    TypeChanged,
}

impl ChangeType {
    /// Parse from git status letter
    fn from_status_char(c: char) -> Option<Self> {
        match c {
            'A' => Some(Self::Added),
            'M' => Some(Self::Modified),
            'D' => Some(Self::Deleted),
            'R' => Some(Self::Renamed),
            'C' => Some(Self::Copied),
            'T' => Some(Self::TypeChanged),
            _ => None,
        }
    }

    /// Get a human-readable description
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::TypeChanged => "type_changed",
        }
    }
}

/// Information about a changed file
#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// Path to the file (relative to repo root)
    pub path: String,
    /// Original path if renamed/copied
    pub old_path: Option<String>,
    /// Type of change
    pub change_type: ChangeType,
}

/// Get list of files changed between two refs
///
/// # Arguments
/// * `from_ref` - Starting ref (e.g., "main", commit SHA)
/// * `to_ref` - Ending ref (e.g., "HEAD", branch name)
/// * `cwd` - Working directory (None for current)
///
/// # Returns
/// List of changed files with their change types
pub fn get_changed_files(from_ref: &str, to_ref: &str, cwd: Option<&Path>) -> Result<Vec<ChangedFile>> {
    // Use --name-status to get change type and filename
    // Use -M for rename detection
    let output = git_command(
        &["diff", "--name-status", "-M", from_ref, to_ref],
        cwd,
    )?;

    parse_name_status_output(&output)
}

/// Get files changed in a specific commit
pub fn get_commit_changed_files(commit: &str, cwd: Option<&Path>) -> Result<Vec<ChangedFile>> {
    let output = git_command(
        &["diff-tree", "--no-commit-id", "--name-status", "-r", "-M", commit],
        cwd,
    )?;

    parse_name_status_output(&output)
}

/// Parse the output of git diff --name-status
fn parse_name_status_output(output: &str) -> Result<Vec<ChangedFile>> {
    let mut files = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }

        let status = parts[0];
        let change_type = status.chars().next()
            .and_then(ChangeType::from_status_char)
            .ok_or_else(|| McpDiffError::GitError {
                message: format!("Unknown git status: {}", status),
            })?;

        let (path, old_path) = match change_type {
            ChangeType::Renamed | ChangeType::Copied if parts.len() >= 3 => {
                (parts[2].to_string(), Some(parts[1].to_string()))
            }
            _ if parts.len() >= 2 => {
                (parts[1].to_string(), None)
            }
            _ => {
                return Err(McpDiffError::GitError {
                    message: format!("Invalid diff output line: {}", line),
                });
            }
        };

        files.push(ChangedFile {
            path,
            old_path,
            change_type,
        });
    }

    Ok(files)
}

/// Get the raw diff content between two refs
pub fn get_diff_content(from_ref: &str, to_ref: &str, file_path: Option<&str>, cwd: Option<&Path>) -> Result<String> {
    let mut args = vec!["diff", from_ref, to_ref];

    if let Some(path) = file_path {
        args.push("--");
        args.push(path);
    }

    git_command(&args, cwd)
}

/// Get stats summary of changes
pub fn get_diff_stats(from_ref: &str, to_ref: &str, cwd: Option<&Path>) -> Result<String> {
    git_command(&["diff", "--stat", from_ref, to_ref], cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_status_modified() {
        let output = "M\tsrc/main.rs";
        let files = parse_name_status_output(output).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn test_parse_name_status_added() {
        let output = "A\tnew_file.rs";
        let files = parse_name_status_output(output).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new_file.rs");
        assert_eq!(files[0].change_type, ChangeType::Added);
    }

    #[test]
    fn test_parse_name_status_renamed() {
        let output = "R100\told_name.rs\tnew_name.rs";
        let files = parse_name_status_output(output).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new_name.rs");
        assert_eq!(files[0].old_path, Some("old_name.rs".to_string()));
        assert_eq!(files[0].change_type, ChangeType::Renamed);
    }

    #[test]
    fn test_parse_name_status_multiple() {
        let output = "M\tsrc/lib.rs\nA\tsrc/new.rs\nD\tsrc/old.rs";
        let files = parse_name_status_output(output).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].change_type, ChangeType::Modified);
        assert_eq!(files[1].change_type, ChangeType::Added);
        assert_eq!(files[2].change_type, ChangeType::Deleted);
    }

    #[test]
    fn test_change_type_as_str() {
        assert_eq!(ChangeType::Added.as_str(), "added");
        assert_eq!(ChangeType::Modified.as_str(), "modified");
        assert_eq!(ChangeType::Deleted.as_str(), "deleted");
    }
}
