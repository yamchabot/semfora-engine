//! Cross-platform filesystem utilities for Windows compatibility
//!
//! This module provides helpers that work correctly on both Unix and Windows:
//! - `normalize_path`: Strips Windows `\\?\` prefix from canonicalized paths
//! - `atomic_rename`: Handles atomic file replacement (Windows requires explicit delete)
//! - `get_cache_base_dir`: Returns platform-appropriate cache directory

use std::io;
use std::path::{Path, PathBuf};

/// Normalize Windows paths by removing the `\\?\` prefix if present.
///
/// On Windows, `Path::canonicalize()` returns paths with the extended-length path prefix
/// (`\\?\C:\...`), which can cause issues with:
/// - String comparisons (different prefixes)
/// - Hash computations (affects cache identification)
/// - User-facing display (confusing prefix)
///
/// This function strips the prefix on Windows while being a no-op on Unix.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use semfora_engine::fs_utils::normalize_path;
///
/// // On Unix, path is returned unchanged
/// let path = PathBuf::from("/home/user/repo");
/// assert_eq!(normalize_path(&path), path);
/// ```
pub fn normalize_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        // Handle UNC paths: \\?\UNC\server\share -> \\server\share
        if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{}", stripped));
        }
        // Handle local paths: \\?\C:\path -> C:\path
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path.to_path_buf()
}

/// Cross-platform atomic rename that handles Windows file replacement.
///
/// On Unix, `fs::rename` atomically replaces the target if it exists.
/// On Windows, `fs::rename` fails if the target exists (needs `MOVEFILE_REPLACE_EXISTING`).
///
/// This function provides consistent behavior by deleting the target on Windows first.
///
/// # Errors
///
/// Returns an error if:
/// - The source file doesn't exist
/// - The target file exists and cannot be deleted (Windows only)
/// - The rename operation fails
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use semfora_engine::fs_utils::atomic_rename;
///
/// // Write to temp file, then atomically replace target
/// std::fs::write("config.tmp", "new content")?;
/// atomic_rename(Path::new("config.tmp"), Path::new("config.toml"))?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn atomic_rename(src: &Path, dst: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Windows requires explicit deletion before rename if target exists
        if dst.exists() {
            std::fs::remove_file(dst)?;
        }
    }
    std::fs::rename(src, dst)
}

/// Get platform-appropriate cache base directory.
///
/// Returns the cache directory following platform conventions:
/// - **Windows**: `%LOCALAPPDATA%\semfora\cache` (e.g., `C:\Users\<user>\AppData\Local\semfora\cache`)
/// - **Unix**: `$XDG_CACHE_HOME/semfora` or `~/.cache/semfora`
/// - **Fallback**: System temp directory + `semfora`
///
/// # Examples
///
/// ```
/// use semfora_engine::fs_utils::get_cache_base_dir;
///
/// let cache_dir = get_cache_base_dir();
/// assert!(cache_dir.to_string_lossy().contains("semfora"));
/// ```
pub fn get_cache_base_dir() -> PathBuf {
    #[cfg(windows)]
    {
        // Windows: Use %LOCALAPPDATA%\semfora\cache
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_appdata).join("semfora").join("cache");
        }
        // Fallback if LOCALAPPDATA not set (rare)
        if let Some(home) = dirs::home_dir() {
            return home
                .join("AppData")
                .join("Local")
                .join("semfora")
                .join("cache");
        }
    }

    #[cfg(not(windows))]
    {
        // Unix: Check XDG_CACHE_HOME first
        if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            return PathBuf::from(xdg_cache).join("semfora");
        }
        // Fall back to ~/.cache/semfora
        if let Some(home) = dirs::home_dir() {
            return home.join(".cache").join("semfora");
        }
    }

    // Last resort: temp directory
    std::env::temp_dir().join("semfora")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_preserves_regular_paths() {
        // Regular paths should be unchanged
        let unix_path = PathBuf::from("/home/user/repo");
        assert_eq!(normalize_path(&unix_path), unix_path);

        let windows_path = PathBuf::from(r"C:\Users\Test\repo");
        assert_eq!(normalize_path(&windows_path), windows_path);
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_path_strips_windows_prefix() {
        // Test \\?\ prefix stripping on Windows
        let prefixed = PathBuf::from(r"\\?\C:\Users\Test\repo");
        let expected = PathBuf::from(r"C:\Users\Test\repo");
        assert_eq!(normalize_path(&prefixed), expected);

        // Test UNC path conversion
        let unc_prefixed = PathBuf::from(r"\\?\UNC\server\share\path");
        let unc_expected = PathBuf::from(r"\\server\share\path");
        assert_eq!(normalize_path(&unc_prefixed), unc_expected);
    }

    #[test]
    fn test_get_cache_base_dir_contains_semfora() {
        let dir = get_cache_base_dir();
        assert!(
            dir.to_string_lossy().contains("semfora"),
            "Cache dir should contain 'semfora': {:?}",
            dir
        );
    }

    #[test]
    fn test_get_cache_base_dir_is_absolute() {
        let dir = get_cache_base_dir();
        // The path should be absolute or at least contain a recognizable base
        let path_str = dir.to_string_lossy();
        assert!(
            path_str.starts_with('/') || path_str.contains(':') || path_str.starts_with("tmp"),
            "Cache dir should be absolute: {:?}",
            dir
        );
    }

    #[test]
    fn test_atomic_rename_creates_file() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join("semfora_test_atomic");
        let _ = fs::create_dir_all(&temp_dir);

        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        // Clean up any existing files
        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);

        // Write source file
        fs::write(&src, "test content").expect("Failed to write source");

        // Atomic rename
        atomic_rename(&src, &dst).expect("Failed to rename");

        // Verify
        assert!(!src.exists(), "Source should not exist after rename");
        assert!(dst.exists(), "Dest should exist after rename");
        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "test content",
            "Content should match"
        );

        // Cleanup
        let _ = fs::remove_file(&dst);
        let _ = fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_atomic_rename_replaces_existing() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join("semfora_test_atomic_replace");
        let _ = fs::create_dir_all(&temp_dir);

        let src = temp_dir.join("new.txt");
        let dst = temp_dir.join("existing.txt");

        // Clean up
        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);

        // Create existing destination
        fs::write(&dst, "old content").expect("Failed to write dest");

        // Create source
        fs::write(&src, "new content").expect("Failed to write source");

        // Atomic rename should replace existing
        atomic_rename(&src, &dst).expect("Failed to rename over existing");

        // Verify replacement
        assert!(!src.exists(), "Source should not exist after rename");
        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "new content",
            "Content should be replaced"
        );

        // Cleanup
        let _ = fs::remove_file(&dst);
        let _ = fs::remove_dir(&temp_dir);
    }
}
