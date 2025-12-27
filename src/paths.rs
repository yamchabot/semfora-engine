//! Unified path resolution for CLI and MCP contexts
//!
//! This module provides consistent path resolution used by both CLI commands
//! and MCP server handlers. It eliminates duplication and ensures consistent
//! behavior across all code paths.

use std::path::{Path, PathBuf};

use crate::{McpDiffError, Result};

/// Resolve path, defaulting to current working directory if None.
///
/// This is the primary entry point for path resolution. Use this when
/// accepting an optional path from CLI arguments or MCP requests.
///
/// # Examples
///
/// ```ignore
/// use semfora_engine::paths::resolve_path;
///
/// // Default to CWD
/// let path = resolve_path(None)?;
///
/// // Resolve relative path
/// let path = resolve_path(Some("./src"))?;
///
/// // Pass through absolute path
/// let path = resolve_path(Some("/home/user/project"))?;
/// ```
pub fn resolve_path(path: Option<&str>) -> Result<PathBuf> {
    match path {
        Some(p) => resolve_path_or_cwd(p),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        }),
    }
}

/// Resolve a path string, treating relative paths as relative to CWD.
///
/// - Absolute paths are returned as-is
/// - Relative paths are joined with the current working directory
///
/// # Errors
///
/// Returns an error if the current directory cannot be determined
/// (only relevant for relative paths).
pub fn resolve_path_or_cwd(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        let cwd = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?;
        Ok(cwd.join(p))
    }
}

/// Resolve path from `Option<PathBuf>`, defaulting to CWD if None.
///
/// This variant is useful when working with clap parsed arguments
/// that are already PathBuf.
pub fn resolve_pathbuf(path: Option<&PathBuf>) -> Result<PathBuf> {
    match path {
        Some(p) => {
            if p.is_absolute() {
                Ok(p.clone())
            } else {
                let cwd = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
                    path: format!("current directory: {}", e),
                })?;
                Ok(cwd.join(p))
            }
        }
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        }),
    }
}

/// Canonicalize path for consistent comparison.
///
/// Attempts to resolve symlinks and get the absolute path. If canonicalization
/// fails (e.g., path doesn't exist), returns the original path unchanged.
///
/// Note: For Windows path normalization (UNC paths, \\?\ prefix), use
/// `fs_utils::normalize_path` instead.
pub fn canonicalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Check if a path exists and is a directory.
///
/// Returns Ok(path) if valid directory, Err otherwise.
pub fn ensure_directory(path: &Path) -> Result<&Path> {
    if !path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: path.display().to_string(),
        });
    }
    if !path.is_dir() {
        return Err(McpDiffError::FileNotFound {
            path: format!("{} is not a directory", path.display()),
        });
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_path_none_returns_cwd() {
        let result = resolve_path(None).unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(result, cwd);
    }

    #[test]
    fn test_resolve_path_absolute() {
        let result = resolve_path(Some("/tmp")).unwrap();
        assert_eq!(result, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let result = resolve_path(Some("src")).unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(result, cwd.join("src"));
    }

    #[test]
    fn test_canonicalize_path_existing() {
        let cwd = std::env::current_dir().unwrap();
        let canonicalized = canonicalize_path(&cwd);
        // Canonicalized path should be absolute
        assert!(canonicalized.is_absolute());
    }

    #[test]
    fn test_canonicalize_path_nonexistent() {
        let fake_path = PathBuf::from("/this/path/does/not/exist/xyz");
        let canonicalized = canonicalize_path(&fake_path);
        // Should return original since canonicalization fails
        assert_eq!(canonicalized, fake_path);
    }
}
