//! Cache storage module for sharded semantic index
//!
//! Provides XDG-compliant cache directory management and repo hashing
//! for storing sharded semantic IR that can be queried by AI agents.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::schema::SCHEMA_VERSION;

// FNV-1a constants for 64-bit hash (same as schema.rs for consistency)
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Compute a stable FNV-1a hash
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Metadata for cached files to detect staleness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    /// Schema version for compatibility
    pub schema_version: String,

    /// When this cache was generated
    pub generated_at: String,

    /// Source files that contributed to this cache entry
    pub source_files: Vec<SourceFileInfo>,

    /// Indexing status (for progressive indexing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexing_status: Option<IndexingStatus>,
}

/// Information about a source file for staleness detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFileInfo {
    /// Relative path from repo root
    pub path: String,

    /// File modification time (Unix timestamp)
    pub mtime: u64,

    /// File size in bytes (for quick change detection)
    pub size: u64,
}

impl SourceFileInfo {
    /// Create from a file path
    pub fn from_path(path: &Path, repo_root: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let mtime = metadata
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();

        let relative_path = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        Some(Self {
            path: relative_path,
            mtime,
            size: metadata.len(),
        })
    }

    /// Check if the source file has changed
    pub fn is_stale(&self, repo_root: &Path) -> bool {
        let full_path = repo_root.join(&self.path);
        match fs::metadata(&full_path) {
            Ok(metadata) => {
                let current_mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // Stale if mtime changed or size changed
                current_mtime != self.mtime || metadata.len() != self.size
            }
            Err(_) => true, // File deleted or inaccessible = stale
        }
    }
}

/// Progress status for ongoing indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStatus {
    /// Whether indexing is in progress
    pub in_progress: bool,

    /// Number of files indexed so far
    pub files_indexed: usize,

    /// Total number of files to index
    pub files_total: usize,

    /// Percentage complete (0-100)
    pub percent: u8,

    /// Estimated seconds remaining
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u32>,

    /// Modules that are ready to query
    pub modules_ready: Vec<String>,

    /// Modules still being indexed
    pub modules_pending: Vec<String>,
}

impl Default for IndexingStatus {
    fn default() -> Self {
        Self {
            in_progress: false,
            files_indexed: 0,
            files_total: 0,
            percent: 0,
            eta_seconds: None,
            modules_ready: Vec::new(),
            modules_pending: Vec::new(),
        }
    }
}

impl CacheMeta {
    /// Create a new cache metadata entry
    pub fn new(source_files: Vec<SourceFileInfo>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            source_files,
            indexing_status: None,
        }
    }

    /// Create metadata for a single file
    pub fn for_file(path: &Path, repo_root: &Path) -> Self {
        let source_files = SourceFileInfo::from_path(path, repo_root)
            .map(|f| vec![f])
            .unwrap_or_default();
        Self::new(source_files)
    }

    /// Check if any source file is stale
    pub fn is_stale(&self, repo_root: &Path) -> bool {
        self.source_files.iter().any(|f| f.is_stale(repo_root))
    }

    /// Check if schema version is compatible
    pub fn is_compatible(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
    }
}

/// Cache directory structure manager
pub struct CacheDir {
    /// Root of the cache for this repo
    pub root: PathBuf,

    /// Path to the repository being indexed
    pub repo_root: PathBuf,

    /// Repo hash (for identification)
    pub repo_hash: String,
}

impl CacheDir {
    /// Create a cache directory for a repository
    pub fn for_repo(repo_path: &Path) -> Result<Self> {
        let repo_root = repo_path.canonicalize().unwrap_or_else(|_| repo_path.to_path_buf());
        let repo_hash = compute_repo_hash(&repo_root);
        let cache_base = get_cache_base_dir();
        let root = cache_base.join(&repo_hash);

        Ok(Self {
            root,
            repo_root,
            repo_hash,
        })
    }

    /// Initialize the cache directory structure
    pub fn init(&self) -> Result<()> {
        // Create main directories
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(self.modules_dir())?;
        fs::create_dir_all(self.symbols_dir())?;
        fs::create_dir_all(self.graphs_dir())?;
        fs::create_dir_all(self.diffs_dir())?;

        Ok(())
    }

    /// Check if the cache exists and is initialized
    pub fn exists(&self) -> bool {
        self.root.exists() && self.repo_overview_path().exists()
    }

    // ========== Path accessors ==========

    /// Path to repo_overview.toon
    pub fn repo_overview_path(&self) -> PathBuf {
        self.root.join("repo_overview.toon")
    }

    /// Path to modules directory
    pub fn modules_dir(&self) -> PathBuf {
        self.root.join("modules")
    }

    /// Path to a specific module file
    pub fn module_path(&self, module_name: &str) -> PathBuf {
        self.modules_dir().join(format!("{}.toon", sanitize_filename(module_name)))
    }

    /// Path to symbols directory
    pub fn symbols_dir(&self) -> PathBuf {
        self.root.join("symbols")
    }

    /// Path to a specific symbol file
    pub fn symbol_path(&self, symbol_hash: &str) -> PathBuf {
        self.symbols_dir().join(format!("{}.toon", symbol_hash))
    }

    /// Path to graphs directory
    pub fn graphs_dir(&self) -> PathBuf {
        self.root.join("graphs")
    }

    /// Path to call graph
    pub fn call_graph_path(&self) -> PathBuf {
        self.graphs_dir().join("call_graph.toon")
    }

    /// Path to import graph
    pub fn import_graph_path(&self) -> PathBuf {
        self.graphs_dir().join("import_graph.toon")
    }

    /// Path to module graph
    pub fn module_graph_path(&self) -> PathBuf {
        self.graphs_dir().join("module_graph.toon")
    }

    /// Path to diffs directory
    pub fn diffs_dir(&self) -> PathBuf {
        self.root.join("diffs")
    }

    /// Path to a specific diff file
    pub fn diff_path(&self, commit_sha: &str) -> PathBuf {
        self.diffs_dir().join(format!("commit_{}.toon", commit_sha))
    }

    // ========== Utility methods ==========

    /// List all module names in the cache
    pub fn list_modules(&self) -> Vec<String> {
        fs::read_dir(self.modules_dir())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().map(|e| e == "toon").unwrap_or(false) {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// List all symbol hashes in the cache
    pub fn list_symbols(&self) -> Vec<String> {
        fs::read_dir(self.symbols_dir())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().map(|e| e == "toon").unwrap_or(false) {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get cache size in bytes
    pub fn size(&self) -> u64 {
        dir_size(&self.root)
    }

    /// Clear the cache
    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

/// Get the base cache directory (XDG-compliant)
pub fn get_cache_base_dir() -> PathBuf {
    // Check XDG_CACHE_HOME first
    if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg_cache).join("semfora");
    }

    // Fall back to ~/.cache/semfora
    if let Some(home) = dirs::home_dir() {
        return home.join(".cache").join("semfora");
    }

    // Last resort: temp directory
    std::env::temp_dir().join("semfora")
}

/// Compute a stable hash for a repository
///
/// Prefers git remote URL for consistency across clones,
/// falls back to absolute path.
pub fn compute_repo_hash(repo_path: &Path) -> String {
    // Try to get git remote URL first
    if let Some(remote_url) = get_git_remote_url(repo_path) {
        return format!("{:016x}", fnv1a_hash(&remote_url));
    }

    // Fall back to absolute path
    let canonical = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());
    format!("{:016x}", fnv1a_hash(&canonical.to_string_lossy()))
}

/// Get the git remote URL for a repository
fn get_git_remote_url(repo_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !url.is_empty() {
            return Some(url);
        }
    }

    None
}

/// Sanitize a string for use as a filename
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Calculate total size of a directory
fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

/// List all cached repositories
pub fn list_cached_repos() -> Vec<(String, PathBuf, u64)> {
    let cache_base = get_cache_base_dir();

    fs::read_dir(&cache_base)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let path = entry.path();
            let hash = entry.file_name().to_string_lossy().to_string();
            let size = dir_size(&path);
            (hash, path, size)
        })
        .collect()
}

/// Prune caches older than the specified number of days
pub fn prune_old_caches(days: u32) -> Result<usize> {
    let cache_base = get_cache_base_dir();
    let cutoff = SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(days as u64 * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut count = 0;

    for entry in fs::read_dir(&cache_base)?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check the modification time of repo_overview.toon as proxy for last use
        let overview_path = path.join("repo_overview.toon");
        if let Ok(metadata) = fs::metadata(&overview_path) {
            if let Ok(modified) = metadata.modified() {
                if modified < cutoff {
                    fs::remove_dir_all(&path)?;
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_compute_repo_hash_deterministic() {
        let path = Path::new("/tmp/test-repo");
        let hash1 = compute_repo_hash(path);
        let hash2 = compute_repo_hash(path);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16); // 64-bit hash as hex
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("api"), "api");
        assert_eq!(sanitize_filename("components/ui"), "components_ui");
        assert_eq!(sanitize_filename("src:main"), "src_main");
    }

    #[test]
    fn test_cache_base_dir() {
        let base = get_cache_base_dir();
        assert!(base.to_string_lossy().contains("semfora"));
    }

    #[test]
    fn test_cache_dir_paths() {
        let cache = CacheDir {
            root: PathBuf::from("/tmp/semfora/abc123"),
            repo_root: PathBuf::from("/home/user/project"),
            repo_hash: "abc123".to_string(),
        };

        assert_eq!(
            cache.repo_overview_path(),
            PathBuf::from("/tmp/semfora/abc123/repo_overview.toon")
        );
        assert_eq!(
            cache.module_path("api"),
            PathBuf::from("/tmp/semfora/abc123/modules/api.toon")
        );
        assert_eq!(
            cache.symbol_path("def456"),
            PathBuf::from("/tmp/semfora/abc123/symbols/def456.toon")
        );
    }

    #[test]
    fn test_source_file_info() {
        // Test with current file
        let current_file = Path::new(file!());
        let repo_root = env::current_dir().unwrap();

        if let Some(info) = SourceFileInfo::from_path(current_file, &repo_root) {
            assert!(!info.path.is_empty());
            assert!(info.mtime > 0);
            assert!(info.size > 0);
            assert!(!info.is_stale(&repo_root));
        }
    }
}
