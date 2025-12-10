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
use crate::git;
use crate::overlay::{LayerKind, LayeredIndex, Overlay};
use crate::schema::{fnv1a_hash, SCHEMA_VERSION};

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

    /// Git SHA this index was created at (for incremental indexing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_sha: Option<String>,
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
            indexed_sha: None,
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
#[derive(Clone)]
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

    /// Create a cache directory for a worktree (uses path-based hash, not git remote)
    /// This ensures each worktree gets its own separate cache even if they share the same git repo
    pub fn for_worktree(worktree_path: &Path) -> Result<Self> {
        let repo_root = worktree_path.canonicalize().unwrap_or_else(|_| worktree_path.to_path_buf());
        // Use path-based hash for worktrees (not git remote URL)
        let repo_hash = format!("{:016x}", fnv1a_hash(&repo_root.to_string_lossy()));
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
        fs::create_dir_all(self.layers_dir())?;

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

    // ========== Layer paths (SEM-45) ==========

    /// Path to layers directory
    pub fn layers_dir(&self) -> PathBuf {
        self.root.join("layers")
    }

    /// Path to a specific layer's directory
    ///
    /// AI layer is not persisted - returns None for LayerKind::AI
    pub fn layer_dir(&self, kind: LayerKind) -> Option<PathBuf> {
        match kind {
            LayerKind::AI => None, // AI layer is ephemeral
            _ => Some(self.layers_dir().join(kind.as_str())),
        }
    }

    /// Path to a layer's symbols.jsonl file
    pub fn layer_symbols_path(&self, kind: LayerKind) -> Option<PathBuf> {
        self.layer_dir(kind).map(|d| d.join("symbols.jsonl"))
    }

    /// Path to a layer's deleted.txt file
    pub fn layer_deleted_path(&self, kind: LayerKind) -> Option<PathBuf> {
        self.layer_dir(kind).map(|d| d.join("deleted.txt"))
    }

    /// Path to a layer's moves.jsonl file
    pub fn layer_moves_path(&self, kind: LayerKind) -> Option<PathBuf> {
        self.layer_dir(kind).map(|d| d.join("moves.jsonl"))
    }

    /// Path to layered index metadata file
    pub fn layer_meta_path(&self) -> PathBuf {
        self.layers_dir().join("meta.json")
    }

    /// Path to head_sha file (last indexed commit)
    pub fn head_sha_path(&self) -> PathBuf {
        self.root.join("head_sha")
    }

    /// Get the indexed SHA (last commit that was indexed)
    pub fn get_indexed_sha(&self) -> Option<String> {
        let path = self.head_sha_path();
        fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Set the indexed SHA (save after successful indexing)
    pub fn set_indexed_sha(&self, sha: &str) -> Result<()> {
        let path = self.head_sha_path();
        fs::write(&path, sha)?;
        Ok(())
    }

    /// Check if cached layers exist
    pub fn has_cached_layers(&self) -> bool {
        self.layers_dir().exists()
            && self.layer_dir(LayerKind::Base).map(|p| p.exists()).unwrap_or(false)
    }

    /// Initialize layer directories
    pub fn init_layer_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.layers_dir())?;
        for kind in [LayerKind::Base, LayerKind::Branch, LayerKind::Working] {
            if let Some(dir) = self.layer_dir(kind) {
                fs::create_dir_all(dir)?;
            }
        }
        Ok(())
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

    // ========== Static Analysis API ==========

    /// Load the call graph from cache
    ///
    /// Returns a HashMap where keys are symbol hashes and values are lists of
    /// called symbol names/hashes.
    pub fn load_call_graph(&self) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let path = self.call_graph_path();
        if !path.exists() {
            return Ok(std::collections::HashMap::new());
        }

        let content = fs::read_to_string(&path)?;
        let mut graph = std::collections::HashMap::new();

        // Parse TOON format call graph
        // Format: caller_hash: [callee1, callee2, ...]
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("_type:") || line.starts_with("schema_version:") || line.starts_with("edges:") {
                continue;
            }

            // Parse "hash: [call1, call2, ...]" format
            if let Some(colon_pos) = line.find(':') {
                let hash = line[..colon_pos].trim().to_string();
                let rest = line[colon_pos + 1..].trim();

                // Parse the array part
                if rest.starts_with('[') && rest.ends_with(']') {
                    let inner = &rest[1..rest.len() - 1];
                    let calls: Vec<String> = inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    if !calls.is_empty() {
                        graph.insert(hash, calls);
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Load all SemanticSummaries for a module
    ///
    /// Parses the module's TOON shard and reconstructs SemanticSummary objects.
    pub fn load_module_summaries(&self, module_name: &str) -> Result<Vec<crate::schema::SemanticSummary>> {
        let path = self.module_path(module_name);
        if !path.exists() {
            return Err(crate::McpDiffError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let content = fs::read_to_string(&path)?;
        let mut summaries = Vec::new();
        let mut current_summary: Option<crate::schema::SemanticSummary> = None;

        // Parse TOON format - each symbol starts with "--- hash ---" or contains symbol_id
        for line in content.lines() {
            let line = line.trim();

            // New symbol section
            if line.starts_with("--- ") && line.ends_with(" ---") {
                // Save previous summary if exists
                if let Some(summary) = current_summary.take() {
                    summaries.push(summary);
                }
                current_summary = Some(crate::schema::SemanticSummary::default());
                continue;
            }

            // Skip header lines
            if line.starts_with("_type:") || line.starts_with("schema_version:") || line.starts_with("module:") {
                continue;
            }

            // Parse key: value pairs into current summary
            if let Some(ref mut summary) = current_summary {
                if let Some(colon_pos) = line.find(':') {
                    let key = line[..colon_pos].trim();
                    let value = line[colon_pos + 1..].trim();

                    match key {
                        "file" => summary.file = value.trim_matches('"').to_string(),
                        "language" => summary.language = value.trim_matches('"').to_string(),
                        "symbol" => summary.symbol = Some(value.trim_matches('"').to_string()),
                        "symbol_id" => {
                            summary.symbol_id = Some(crate::schema::SymbolId {
                                hash: value.trim_matches('"').to_string(),
                                ..Default::default()
                            });
                        }
                        "symbol_kind" => {
                            summary.symbol_kind = Some(crate::schema::SymbolKind::from_str(value.trim_matches('"')));
                        }
                        "lines" => {
                            // Parse "start-end" format
                            let parts: Vec<&str> = value.trim_matches('"').split('-').collect();
                            if parts.len() == 2 {
                                summary.start_line = parts[0].parse().ok();
                                summary.end_line = parts[1].parse().ok();
                            }
                        }
                        "behavioral_risk" => {
                            summary.behavioral_risk = match value.trim_matches('"') {
                                "high" => crate::schema::RiskLevel::High,
                                "medium" => crate::schema::RiskLevel::Medium,
                                _ => crate::schema::RiskLevel::Low,
                            };
                        }
                        "control_flow" => {
                            // Parse control flow array
                            if value.starts_with('[') && value.ends_with(']') {
                                let inner = &value[1..value.len() - 1];
                                for kind_str in inner.split(',') {
                                    let kind = kind_str.trim().trim_matches('"');
                                    if !kind.is_empty() {
                                        summary.control_flow_changes.push(crate::schema::ControlFlowChange {
                                            kind: crate::schema::ControlFlowKind::from_str(kind),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }
                        "added_dependencies" => {
                            // Parse dependencies array
                            if value.starts_with('[') && value.ends_with(']') {
                                let inner = &value[1..value.len() - 1];
                                for dep in inner.split(',') {
                                    let dep = dep.trim().trim_matches('"');
                                    if !dep.is_empty() {
                                        summary.added_dependencies.push(dep.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Don't forget the last summary
        if let Some(summary) = current_summary {
            summaries.push(summary);
        }

        Ok(summaries)
    }

    // ========== Query-Driven API (v1) ==========

    /// Path to the symbol index file (JSONL format)
    pub fn symbol_index_path(&self) -> PathBuf {
        self.root.join("symbol_index.jsonl")
    }

    /// Check if symbol index exists
    pub fn has_symbol_index(&self) -> bool {
        self.symbol_index_path().exists()
    }

    /// Update symbol index for a single file (incremental update)
    ///
    /// This removes all existing entries for the file and adds new ones.
    /// Used by the file watcher to keep the index up-to-date in real-time.
    pub fn update_symbol_index_for_file(
        &self,
        file_path: &str,
        new_entries: Vec<SymbolIndexEntry>,
    ) -> Result<()> {
        use std::io::{BufRead, Write};

        let index_path = self.symbol_index_path();
        tracing::debug!("[CACHE] update_symbol_index_for_file: file_path={}, cache={}", file_path, index_path.display());

        // Read existing entries, filtering out the ones for this file
        let mut entries: Vec<SymbolIndexEntry> = if index_path.exists() {
            let file = fs::File::open(&index_path)?;
            let reader = std::io::BufReader::new(file);
            let all_entries: Vec<_> = reader
                .lines()
                .filter_map(|line| line.ok())
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str::<SymbolIndexEntry>(&line).ok())
                .collect();

            let before_count = all_entries.len();
            let filtered: Vec<_> = all_entries
                .into_iter()
                .filter(|entry| {
                    let keep = entry.file != file_path;
                    if !keep {
                        tracing::debug!("[CACHE] Filtering out entry for file: {}", entry.file);
                    }
                    keep
                })
                .collect();
            tracing::debug!("[CACHE] Filtered {} -> {} entries (removed {} for {})",
                before_count, filtered.len(), before_count - filtered.len(), file_path);
            filtered
        } else {
            Vec::new()
        };

        // Add new entries
        entries.extend(new_entries);

        // Write back atomically (temp file + rename)
        let temp_path = index_path.with_extension("jsonl.tmp");
        {
            let mut file = fs::File::create(&temp_path)?;
            for entry in &entries {
                let json = serde_json::to_string(&entry).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to serialize symbol index entry: {}", e),
                    }
                })?;
                writeln!(file, "{}", json)?;
            }
        }

        // Atomic rename
        fs::rename(&temp_path, &index_path)?;

        tracing::debug!(
            "[CACHE] Updated symbol_index.jsonl for {}: {} total entries",
            file_path,
            entries.len()
        );

        Ok(())
    }

    /// Search symbol index with filters
    /// Returns lightweight entries matching the query
    pub fn search_symbols(
        &self,
        query: &str,
        module_filter: Option<&str>,
        kind_filter: Option<&str>,
        risk_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SymbolIndexEntry>> {
        use std::io::BufRead;

        let index_path = self.symbol_index_path();
        if !index_path.exists() {
            return Err(crate::McpDiffError::FileNotFound {
                path: index_path.display().to_string(),
            });
        }

        let file = fs::File::open(&index_path)?;
        let reader = std::io::BufReader::new(file);
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue, // Skip malformed lines
            };

            // Match query against symbol name (case-insensitive, partial)
            if !entry.symbol.to_lowercase().contains(&query_lower) {
                continue;
            }

            // Apply optional filters
            if let Some(m) = module_filter {
                if entry.module != m {
                    continue;
                }
            }
            if let Some(k) = kind_filter {
                if entry.kind != k {
                    continue;
                }
            }
            if let Some(r) = risk_filter {
                if entry.risk != r {
                    continue;
                }
            }

            results.push(entry);

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// List symbols in a module (lightweight index only)
    pub fn list_module_symbols(
        &self,
        module: &str,
        kind_filter: Option<&str>,
        risk_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SymbolIndexEntry>> {
        use std::io::BufRead;

        let index_path = self.symbol_index_path();
        if !index_path.exists() {
            return Err(crate::McpDiffError::FileNotFound {
                path: index_path.display().to_string(),
            });
        }

        let file = fs::File::open(&index_path)?;
        let reader = std::io::BufReader::new(file);
        let mut results = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Must match module
            if entry.module != module {
                continue;
            }

            // Apply optional filters
            if let Some(k) = kind_filter {
                if entry.kind != k {
                    continue;
                }
            }
            if let Some(r) = risk_filter {
                if entry.risk != r {
                    continue;
                }
            }

            results.push(entry);

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Load all symbol index entries (for static analysis)
    ///
    /// Returns all entries from the symbol index without filtering.
    /// Use this for batch analysis operations.
    pub fn load_all_symbol_entries(&self) -> Result<Vec<SymbolIndexEntry>> {
        use std::io::BufRead;

        let index_path = self.symbol_index_path();
        if !index_path.exists() {
            return Err(crate::McpDiffError::FileNotFound {
                path: index_path.display().to_string(),
            });
        }

        let file = fs::File::open(&index_path)?;
        let reader = std::io::BufReader::new(file);
        let mut results = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            results.push(entry);
        }

        Ok(results)
    }

    // ========== Ripgrep Fallback Search (SEM-55) ==========

    /// Search using ripgrep as fallback when no semantic index exists.
    ///
    /// This enables search functionality without waiting for indexing.
    /// Results include file path, line number, and matching content.
    ///
    /// # Arguments
    /// * `query` - Search pattern (regex supported)
    /// * `file_types` - Optional file type filters (e.g., ["rs", "ts"])
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// Vector of `RipgrepSearchResult` entries, or error if search fails
    pub fn search_with_ripgrep(
        &self,
        query: &str,
        file_types: Option<Vec<String>>,
        limit: usize,
    ) -> Result<Vec<RipgrepSearchResult>> {
        use crate::ripgrep::{RipgrepSearcher, SearchOptions};

        let searcher = RipgrepSearcher::new();
        let mut options = SearchOptions::new(query)
            .with_limit(limit)
            .case_insensitive();

        if let Some(types) = file_types {
            options = options.with_file_types(types);
        }

        let matches = searcher.search(&self.repo_root, &options)?;

        Ok(matches
            .into_iter()
            .map(|m| RipgrepSearchResult {
                file: m.file.strip_prefix(&self.repo_root)
                    .unwrap_or(&m.file)
                    .to_string_lossy()
                    .to_string(),
                line: m.line,
                column: m.column,
                content: m.content,
            })
            .collect())
    }

    /// Search symbols with automatic ripgrep fallback when no index exists.
    ///
    /// This is the primary entry point for search that provides graceful degradation:
    /// - If semantic index exists: use fast indexed search
    /// - If no index: fall back to ripgrep text search
    ///
    /// The `fallback_used` field in the result indicates which method was used.
    pub fn search_symbols_with_fallback(
        &self,
        query: &str,
        module_filter: Option<&str>,
        kind_filter: Option<&str>,
        risk_filter: Option<&str>,
        limit: usize,
    ) -> Result<SearchWithFallbackResult> {
        // Try indexed search first
        if self.has_symbol_index() {
            match self.search_symbols(query, module_filter, kind_filter, risk_filter, limit) {
                Ok(results) => {
                    return Ok(SearchWithFallbackResult {
                        indexed_results: Some(results),
                        ripgrep_results: None,
                        fallback_used: false,
                    });
                }
                Err(e) => {
                    // Log error and fall through to ripgrep
                    tracing::warn!("Index search failed, falling back to ripgrep: {}", e);
                }
            }
        }

        // Fall back to ripgrep
        // Note: ripgrep doesn't support module/kind/risk filters, so we search broadly
        let file_types = Self::infer_file_types_from_kind(kind_filter);
        let ripgrep_results = self.search_with_ripgrep(query, file_types, limit)?;

        Ok(SearchWithFallbackResult {
            indexed_results: None,
            ripgrep_results: Some(ripgrep_results),
            fallback_used: true,
        })
    }

    /// Infer file type filters from symbol kind
    fn infer_file_types_from_kind(kind: Option<&str>) -> Option<Vec<String>> {
        match kind {
            Some("component") => Some(vec!["tsx".to_string(), "jsx".to_string(), "vue".to_string(), "svelte".to_string()]),
            Some("fn") | Some("function") => None, // Functions exist in all languages
            Some("struct") | Some("trait") | Some("enum") => Some(vec!["rs".to_string()]),
            Some("class") => Some(vec!["py".to_string(), "ts".to_string(), "tsx".to_string(), "java".to_string()]),
            Some("interface") => Some(vec!["ts".to_string(), "tsx".to_string(), "java".to_string()]),
            _ => None,
        }
    }

    /// Search only uncommitted files (working overlay mode).
    ///
    /// This searches uncommitted changes (staged + unstaged + untracked) in real-time using ripgrep,
    /// bypassing the semantic index entirely. Useful for finding recent changes before
    /// the index is regenerated.
    ///
    /// # Arguments
    /// * `query` - Search pattern (regex supported)
    /// * `file_types` - Optional file type filters
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// `RipgrepSearchResult` entries from uncommitted files only
    pub fn search_working_overlay(
        &self,
        query: &str,
        file_types: Option<Vec<String>>,
        limit: usize,
    ) -> Result<Vec<RipgrepSearchResult>> {
        use crate::git::get_uncommitted_changes;
        use crate::ripgrep::{RipgrepSearcher, SearchOptions};

        let mut search_files: Vec<String> = Vec::new();

        // Get modified/staged files (tracked files with changes)
        if let Ok(changed_files) = get_uncommitted_changes("HEAD", Some(&self.repo_root)) {
            for f in changed_files {
                if f.change_type != crate::git::ChangeType::Deleted {
                    search_files.push(f.path);
                }
            }
        }

        // Also get untracked files using git status --porcelain
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 3 {
                    continue;
                }
                let status = &line[0..2];
                let file_path = line[3..].to_string();
                // ?? = untracked, A = staged new file, M = modified, etc.
                // Skip deleted files (D in first or second position)
                if !status.contains('D') {
                    // Avoid duplicates
                    if !search_files.contains(&file_path) {
                        search_files.push(file_path);
                    }
                }
            }
        }

        if search_files.is_empty() {
            return Ok(vec![]);
        }

        // Search only in these specific files using ripgrep
        let searcher = RipgrepSearcher::new();
        let mut options = SearchOptions::new(query)
            .with_limit(limit * 2)  // Get more results since we'll filter
            .case_insensitive();

        if let Some(types) = file_types {
            options = options.with_file_types(types);
        }

        // Ripgrep can search specific files via --file-list or by specifying paths
        // We'll search the repo but filter results to only include our changed files
        let matches = searcher.search(&self.repo_root, &options)?;

        // Filter to only results from uncommitted files
        let uncommitted_set: std::collections::HashSet<&str> =
            search_files.iter().map(|s| s.as_str()).collect();

        Ok(matches
            .into_iter()
            .filter_map(|m| {
                let rel_path = m.file.strip_prefix(&self.repo_root)
                    .unwrap_or(&m.file)
                    .to_string_lossy()
                    .to_string();

                // Only include if this file is in our uncommitted set
                if uncommitted_set.contains(rel_path.as_str()) {
                    Some(RipgrepSearchResult {
                        file: rel_path,
                        line: m.line,
                        column: m.column,
                        content: m.content,
                    })
                } else {
                    None
                }
            })
            .take(limit)
            .collect())
    }

    // ========== Layer persistence (SEM-45) ==========

    /// Save a single layer overlay to cache using separate files:
    /// - symbols.jsonl: Symbol states (one JSON object per line)
    /// - deleted.txt: Deleted symbol hashes (one per line)
    /// - moves.jsonl: File moves (one JSON object per line)
    ///
    /// AI layer is not persisted and returns Ok(()) immediately.
    ///
    /// Uses atomic two-phase commit: writes to .tmp files first, then renames.
    /// This ensures cache is never left in an inconsistent state.
    pub fn save_layer(&self, overlay: &Overlay) -> Result<()> {
        use std::io::Write;

        let kind = match overlay.meta.kind {
            Some(k) => k,
            None => return Err(crate::McpDiffError::ExtractionFailure {
                message: "Cannot save overlay with unknown layer kind".to_string(),
            }),
        };

        let layer_dir = match self.layer_dir(kind) {
            Some(d) => d,
            None => return Ok(()), // AI layer - skip
        };

        // Ensure layer directory exists
        fs::create_dir_all(&layer_dir)?;

        // Define final and temp paths
        let symbols_path = layer_dir.join("symbols.jsonl");
        let symbols_temp = layer_dir.join("symbols.jsonl.tmp");
        let deleted_path = layer_dir.join("deleted.txt");
        let deleted_temp = layer_dir.join("deleted.txt.tmp");
        let moves_path = layer_dir.join("moves.jsonl");
        let moves_temp = layer_dir.join("moves.jsonl.tmp");
        let meta_path = layer_dir.join("meta.json");
        let meta_temp = layer_dir.join("meta.json.tmp");

        // Phase 1: Write all files to temp locations
        // If any write fails, temp files are left behind but final files are intact

        // Write symbols.jsonl.tmp
        {
            let mut file = fs::File::create(&symbols_temp)?;
            for (hash, state) in &overlay.symbols {
                let entry = SymbolEntry { hash: hash.clone(), state: state.clone() };
                let json = serde_json::to_string(&entry).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to serialize symbol {}: {}", hash, e),
                    }
                })?;
                writeln!(file, "{}", json)?;
            }
        }

        // Write deleted.txt.tmp
        {
            let mut file = fs::File::create(&deleted_temp)?;
            for hash in &overlay.deleted {
                writeln!(file, "{}", hash)?;
            }
        }

        // Write moves.jsonl.tmp
        {
            let mut file = fs::File::create(&moves_temp)?;
            for file_move in &overlay.moves {
                let json = serde_json::to_string(&file_move).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to serialize file move: {}", e),
                    }
                })?;
                writeln!(file, "{}", json)?;
            }
        }

        // Write meta.json.tmp
        let meta_json = serde_json::to_string_pretty(&overlay.meta).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to serialize {} layer meta: {}", kind, e),
            }
        })?;
        fs::write(&meta_temp, &meta_json)?;

        // Phase 2: Atomically rename all temp files to final locations
        // On POSIX systems, rename is atomic within the same filesystem
        fs::rename(&symbols_temp, &symbols_path)?;
        fs::rename(&deleted_temp, &deleted_path)?;
        fs::rename(&moves_temp, &moves_path)?;
        fs::rename(&meta_temp, &meta_path)?;

        Ok(())
    }

    /// Load a single layer overlay from cache
    ///
    /// Returns None if layer directory doesn't exist or AI layer is requested.
    pub fn load_layer(&self, kind: LayerKind) -> Result<Option<Overlay>> {
        use std::io::BufRead;

        let layer_dir = match self.layer_dir(kind) {
            Some(d) => d,
            None => return Ok(None), // AI layer - not persisted
        };

        if !layer_dir.exists() {
            return Ok(None);
        }

        // Load layer metadata
        let meta_path = layer_dir.join("meta.json");
        let meta: crate::overlay::LayerMeta = if meta_path.exists() {
            let json = fs::read_to_string(&meta_path)?;
            serde_json::from_str(&json).map_err(|e| {
                crate::McpDiffError::ExtractionFailure {
                    message: format!("Failed to deserialize {} layer meta: {}", kind, e),
                }
            })?
        } else {
            crate::overlay::LayerMeta::new(kind)
        };

        // Load symbols from symbols.jsonl
        let mut symbols = std::collections::HashMap::new();
        let symbols_path = layer_dir.join("symbols.jsonl");
        if symbols_path.exists() {
            let file = fs::File::open(&symbols_path)?;
            let reader = std::io::BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry: SymbolEntry = serde_json::from_str(&line).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to deserialize symbol entry: {}", e),
                    }
                })?;
                symbols.insert(entry.hash, entry.state);
            }
        }

        // Load deleted hashes from deleted.txt
        let mut deleted = std::collections::HashSet::new();
        let deleted_path = layer_dir.join("deleted.txt");
        if deleted_path.exists() {
            let content = fs::read_to_string(&deleted_path)?;
            for line in content.lines() {
                let hash = line.trim();
                if !hash.is_empty() {
                    deleted.insert(hash.to_string());
                }
            }
        }

        // Load moves from moves.jsonl
        let mut moves = Vec::new();
        let moves_path = layer_dir.join("moves.jsonl");
        if moves_path.exists() {
            let file = fs::File::open(&moves_path)?;
            let reader = std::io::BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let file_move: crate::overlay::FileMove = serde_json::from_str(&line).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to deserialize file move: {}", e),
                    }
                })?;
                moves.push(file_move);
            }
        }

        // Construct the overlay and rebuild file index
        let mut overlay = Overlay {
            meta,
            symbols,
            deleted,
            moves,
            symbols_by_file: std::collections::HashMap::new(),
        };
        overlay.rebuild_file_index();

        Ok(Some(overlay))
    }

    /// Save a full LayeredIndex to cache
    ///
    /// Saves base, branch, and working layers. AI layer is ephemeral.
    pub fn save_layered_index(&self, index: &LayeredIndex) -> Result<()> {
        // Save each persistent layer
        self.save_layer(&index.base)?;
        self.save_layer(&index.branch)?;
        self.save_layer(&index.working)?;
        // AI layer is not saved (ephemeral)

        // Save metadata
        let meta = LayeredIndexMeta {
            schema_version: SCHEMA_VERSION.to_string(),
            saved_at: chrono::Utc::now().to_rfc3339(),
            base_indexed_sha: index.base.meta.indexed_sha.clone(),
            branch_indexed_sha: index.branch.meta.indexed_sha.clone(),
            // merge_base_sha is stored on the branch layer (where branch diverged from base)
            merge_base: index.branch.meta.merge_base_sha.clone(),
        };

        let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to serialize layer meta: {}", e),
            }
        })?;

        fs::write(self.layer_meta_path(), &meta_json)?;

        Ok(())
    }

    /// Load a full LayeredIndex from cache
    ///
    /// Returns None if layers haven't been cached yet.
    /// AI layer is always initialized empty.
    pub fn load_layered_index(&self) -> Result<Option<LayeredIndex>> {
        if !self.has_cached_layers() {
            return Ok(None);
        }

        // Load each layer
        let base = match self.load_layer(LayerKind::Base)? {
            Some(o) => o,
            None => return Ok(None),
        };

        let branch = self.load_layer(LayerKind::Branch)?.unwrap_or_else(|| Overlay::new(LayerKind::Branch));
        let working = self.load_layer(LayerKind::Working)?.unwrap_or_else(|| Overlay::new(LayerKind::Working));
        let ai = Overlay::new(LayerKind::AI); // AI is always fresh

        Ok(Some(LayeredIndex {
            base,
            branch,
            working,
            ai,
        }))
    }

    /// Clear all cached layers
    pub fn clear_layers(&self) -> Result<()> {
        let layers_dir = self.layers_dir();
        if layers_dir.exists() {
            fs::remove_dir_all(&layers_dir)?;
        }
        Ok(())
    }

    // ========== Layer staleness detection (SEM-45) ==========

    /// Check if a cached layer is stale
    ///
    /// Staleness rules:
    /// - Base: indexed_sha != current HEAD of main/master
    /// - Branch: indexed_sha != current branch HEAD
    /// - Working: any tracked file changed (mtime/size)
    /// - AI: always fresh (not persisted)
    pub fn is_layer_stale(&self, kind: LayerKind) -> Result<bool> {
        let overlay = match self.load_layer(kind)? {
            Some(o) => o,
            None => return Ok(true), // No cached layer = stale
        };

        match kind {
            LayerKind::Base => self.is_base_layer_stale(&overlay),
            LayerKind::Branch => self.is_branch_layer_stale(&overlay),
            LayerKind::Working => self.is_working_layer_stale(&overlay),
            LayerKind::AI => Ok(false), // AI layer is never persisted, always fresh in memory
        }
    }

    /// Check if base layer is stale (indexed SHA != main/master HEAD)
    fn is_base_layer_stale(&self, overlay: &Overlay) -> Result<bool> {
        let indexed_sha = match &overlay.meta.indexed_sha {
            Some(sha) => sha,
            None => return Ok(true), // No indexed SHA = stale
        };

        // Get the current base branch HEAD
        let base_branch = git::detect_base_branch(Some(&self.repo_root))?;
        let current_sha = get_ref_sha(&base_branch, Some(&self.repo_root))?;

        Ok(indexed_sha != &current_sha)
    }

    /// Check if branch layer is stale (indexed SHA != branch HEAD, or merge-base changed)
    fn is_branch_layer_stale(&self, overlay: &Overlay) -> Result<bool> {
        let indexed_sha = match &overlay.meta.indexed_sha {
            Some(sha) => sha,
            None => return Ok(true), // No indexed SHA = stale
        };

        // Check if branch HEAD has moved
        let current_sha = get_ref_sha("HEAD", Some(&self.repo_root))?;
        if indexed_sha != &current_sha {
            return Ok(true);
        }

        // Check if merge-base has changed (rebase scenario)
        if let Some(stored_merge_base) = &overlay.meta.merge_base_sha {
            let base_branch = git::detect_base_branch(Some(&self.repo_root))?;
            let current_merge_base = git::get_merge_base("HEAD", &base_branch, Some(&self.repo_root))?;
            if stored_merge_base != &current_merge_base {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if working layer is stale (any tracked file has changed)
    ///
    /// Uses mtime comparison as a quick heuristic. This may have edge cases:
    /// - Clock skew could produce false positives/negatives
    /// - Files touched without content changes trigger false positives
    ///
    /// For more robust detection, consider content hashing (adds overhead).
    /// The current approach favors speed over perfect accuracy since
    /// working layer staleness is checked frequently.
    fn is_working_layer_stale(&self, overlay: &Overlay) -> Result<bool> {
        // Working layer tracks files via symbols_by_file
        // Check if any tracked file's mtime has changed since layer was updated
        for file_path in overlay.symbols_by_file.keys() {
            let full_path = self.repo_root.join(file_path);
            match fs::metadata(&full_path) {
                Ok(meta) => {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    // If the layer was updated before the file was modified, it's stale
                    if mtime > overlay.meta.updated_at {
                        return Ok(true);
                    }
                }
                Err(_) => {
                    // File deleted or inaccessible = stale
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Check if entire LayeredIndex cache is stale
    pub fn is_layered_index_stale(&self) -> Result<bool> {
        // If no cache exists, it's "stale" (needs to be created)
        if !self.has_cached_layers() {
            return Ok(true);
        }

        // Base layer staleness is most critical - rebuild if base moved
        if self.is_layer_stale(LayerKind::Base)? {
            return Ok(true);
        }

        // Branch and working layers can be rebuilt incrementally,
        // but for now we consider the whole index stale if any layer is stale
        if self.is_layer_stale(LayerKind::Branch)? {
            return Ok(true);
        }

        if self.is_layer_stale(LayerKind::Working)? {
            return Ok(true);
        }

        Ok(false)
    }
}

/// Get the SHA for a git reference
fn get_ref_sha(ref_name: &str, cwd: Option<&Path>) -> Result<String> {
    git::git_command(&["rev-parse", ref_name], cwd)
}

/// Metadata for cached LayeredIndex
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayeredIndexMeta {
    /// Schema version for compatibility
    pub schema_version: String,

    /// When the layers were saved
    pub saved_at: String,

    /// Git SHA that base layer was indexed at
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_indexed_sha: Option<String>,

    /// Git SHA that branch layer was indexed at
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_indexed_sha: Option<String>,

    /// Merge base SHA (where branch diverged from base)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<String>,
}

/// Entry in symbols.jsonl for layer persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SymbolEntry {
    /// Symbol hash
    hash: String,
    /// Symbol state
    state: crate::overlay::SymbolState,
}

/// Lightweight symbol index entry for query-driven access
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolIndexEntry {
    /// Symbol name
    #[serde(rename = "s")]
    pub symbol: String,

    /// Symbol hash (for get_symbol lookup)
    #[serde(rename = "h")]
    pub hash: String,

    /// Symbol kind (fn, struct, component, enum, trait, etc.)
    #[serde(rename = "k")]
    pub kind: String,

    /// Module name
    #[serde(rename = "m")]
    pub module: String,

    /// File path (relative to repo root)
    #[serde(rename = "f")]
    pub file: String,

    /// Line range (e.g., "45-89")
    #[serde(rename = "l")]
    pub lines: String,

    /// Risk level (high, medium, low)
    #[serde(rename = "r")]
    pub risk: String,

    /// Cognitive complexity (SonarSource metric)
    #[serde(rename = "cc", default, skip_serializing_if = "is_zero_usize")]
    pub cognitive_complexity: usize,

    /// Maximum nesting depth
    #[serde(rename = "nest", default, skip_serializing_if = "is_zero_usize")]
    pub max_nesting: usize,
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

/// Result from a ripgrep fallback search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RipgrepSearchResult {
    /// File path (relative to repo root)
    pub file: String,

    /// Line number (1-indexed)
    pub line: u64,

    /// Column number (1-indexed)
    pub column: u64,

    /// Content of the matching line
    pub content: String,
}

/// Result from search_symbols_with_fallback
///
/// Contains either indexed results or ripgrep results, along with
/// a flag indicating which method was used.
#[derive(Debug, Clone)]
pub struct SearchWithFallbackResult {
    /// Results from semantic index (if available)
    pub indexed_results: Option<Vec<SymbolIndexEntry>>,

    /// Results from ripgrep fallback (if index unavailable)
    pub ripgrep_results: Option<Vec<RipgrepSearchResult>>,

    /// Whether ripgrep fallback was used
    pub fallback_used: bool,
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

    // ========================================================================
    // Layer Cache Tests (SEM-45)
    // ========================================================================

    #[test]
    fn test_layer_paths() {
        let cache = CacheDir {
            root: PathBuf::from("/tmp/semfora/abc123"),
            repo_root: PathBuf::from("/home/user/project"),
            repo_hash: "abc123".to_string(),
        };

        // Test layers directory path
        assert_eq!(
            cache.layers_dir(),
            PathBuf::from("/tmp/semfora/abc123/layers")
        );

        // Test layer directory paths for each kind
        assert_eq!(
            cache.layer_dir(LayerKind::Base),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/base"))
        );
        assert_eq!(
            cache.layer_dir(LayerKind::Branch),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/branch"))
        );
        assert_eq!(
            cache.layer_dir(LayerKind::Working),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/working"))
        );
        // AI layer should return None (ephemeral)
        assert_eq!(cache.layer_dir(LayerKind::AI), None);

        // Test layer file paths
        assert_eq!(
            cache.layer_symbols_path(LayerKind::Base),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/base/symbols.jsonl"))
        );
        assert_eq!(
            cache.layer_deleted_path(LayerKind::Base),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/base/deleted.txt"))
        );
        assert_eq!(
            cache.layer_moves_path(LayerKind::Base),
            Some(PathBuf::from("/tmp/semfora/abc123/layers/base/moves.jsonl"))
        );

        // Test layer meta path
        assert_eq!(
            cache.layer_meta_path(),
            PathBuf::from("/tmp/semfora/abc123/layers/meta.json")
        );

        // Test head_sha path
        assert_eq!(
            cache.head_sha_path(),
            PathBuf::from("/tmp/semfora/abc123/head_sha")
        );
    }

    #[test]
    fn test_save_and_load_layer() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a test overlay
        let mut overlay = Overlay::new(LayerKind::Base);
        overlay.meta.indexed_sha = Some("abc123".to_string());

        // Save the layer
        cache.save_layer(&overlay).expect("Failed to save layer");

        // Verify the directory and files exist
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        assert!(layer_dir.exists(), "Layer directory should exist after save");
        assert!(layer_dir.join("symbols.jsonl").exists(), "symbols.jsonl should exist");
        assert!(layer_dir.join("deleted.txt").exists(), "deleted.txt should exist");
        assert!(layer_dir.join("moves.jsonl").exists(), "moves.jsonl should exist");
        assert!(layer_dir.join("meta.json").exists(), "meta.json should exist");

        // Load the layer back
        let loaded = cache.load_layer(LayerKind::Base).expect("Failed to load layer");
        assert!(loaded.is_some(), "Should load the saved layer");

        let loaded_overlay = loaded.unwrap();
        assert_eq!(loaded_overlay.meta.indexed_sha, Some("abc123".to_string()));
    }

    #[test]
    fn test_ai_layer_not_saved() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create an AI overlay
        let overlay = Overlay::new(LayerKind::AI);

        // Attempting to save AI layer should succeed but not create a file
        cache.save_layer(&overlay).expect("Save should succeed for AI layer");

        // The layers directory shouldn't have any ai.json file
        let ai_path = cache.layers_dir().join("ai.json");
        assert!(!ai_path.exists(), "AI layer should not be persisted");

        // Loading AI layer should return None
        let loaded = cache.load_layer(LayerKind::AI).expect("Load should succeed");
        assert!(loaded.is_none(), "Loading AI layer should return None");
    }

    #[test]
    fn test_save_and_load_layered_index() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a layered index with some data
        let mut index = LayeredIndex::new();
        index.base.meta.indexed_sha = Some("base_sha".to_string());
        index.branch.meta.indexed_sha = Some("branch_sha".to_string());

        // Save the index
        cache.save_layered_index(&index).expect("Failed to save layered index");

        // Verify meta file exists
        assert!(cache.layer_meta_path().exists(), "Meta file should exist");

        // Verify has_cached_layers returns true
        assert!(cache.has_cached_layers(), "Should detect cached layers");

        // Load the index back
        let loaded = cache.load_layered_index().expect("Failed to load layered index");
        assert!(loaded.is_some(), "Should load the saved index");

        let loaded_index = loaded.unwrap();
        assert_eq!(loaded_index.base.meta.indexed_sha, Some("base_sha".to_string()));
        assert_eq!(loaded_index.branch.meta.indexed_sha, Some("branch_sha".to_string()));
        // AI layer should be fresh (empty)
        assert!(loaded_index.ai.meta.indexed_sha.is_none());
    }

    #[test]
    fn test_clear_layers() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Save some layers
        let index = LayeredIndex::new();
        cache.save_layered_index(&index).expect("Failed to save");
        assert!(cache.has_cached_layers());

        // Clear the layers
        cache.clear_layers().expect("Failed to clear layers");

        // Verify layers are gone
        assert!(!cache.has_cached_layers(), "Layers should be cleared");
        assert!(!cache.layers_dir().exists(), "Layers directory should be removed");
    }

    #[test]
    fn test_load_missing_layer() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Loading a layer that doesn't exist should return None
        let result = cache.load_layer(LayerKind::Base).expect("Load should not error");
        assert!(result.is_none(), "Should return None for missing layer");
    }

    #[test]
    fn test_layered_index_meta_serialization() {
        let meta = LayeredIndexMeta {
            schema_version: "1.0.0".to_string(),
            saved_at: "2024-01-01T00:00:00Z".to_string(),
            base_indexed_sha: Some("abc123".to_string()),
            branch_indexed_sha: None,
            merge_base: Some("def456".to_string()),
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&meta).expect("Serialize failed");
        let restored: LayeredIndexMeta = serde_json::from_str(&json).expect("Deserialize failed");

        assert_eq!(restored.schema_version, "1.0.0");
        assert_eq!(restored.base_indexed_sha, Some("abc123".to_string()));
        assert_eq!(restored.branch_indexed_sha, None);
        assert_eq!(restored.merge_base, Some("def456".to_string()));
    }

    // ========================================================================
    // TDD Tests from SEM-45 Ticket Requirements
    // ========================================================================

    /// TDD: test_layer_paths_created
    /// Verifies that layer directories are created on save
    #[test]
    fn test_layer_paths_created() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Initially no layer directories
        assert!(!cache.layers_dir().exists());

        // Save a layer
        let overlay = Overlay::new(LayerKind::Base);
        cache.save_layer(&overlay).expect("Failed to save layer");

        // Layer paths should now exist
        assert!(cache.layers_dir().exists());
        assert!(cache.layer_dir(LayerKind::Base).unwrap().exists());
        assert!(cache.layer_symbols_path(LayerKind::Base).unwrap().exists());
        assert!(cache.layer_deleted_path(LayerKind::Base).unwrap().exists());
        assert!(cache.layer_moves_path(LayerKind::Base).unwrap().exists());
    }

    /// TDD: test_layer_persist_reload
    /// Verifies layers persist correctly and reload with same data
    #[test]
    fn test_layer_persist_reload() {
        use tempfile::TempDir;
        use crate::overlay::SymbolState;
        use crate::schema::{SymbolInfo, SymbolKind, RiskLevel};

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create an overlay with symbols, deletions, and moves
        let mut overlay = Overlay::new(LayerKind::Base);
        overlay.meta.indexed_sha = Some("abc123".to_string());

        // Add a symbol
        let symbol = SymbolInfo {
            name: "test_func".to_string(),
            kind: SymbolKind::Function,
            start_line: 10,
            end_line: 20,
            behavioral_risk: RiskLevel::Low,
            ..Default::default()
        };
        overlay.upsert(
            "hash_001".to_string(),
            SymbolState::active_at(symbol, PathBuf::from("src/lib.rs")),
        );

        // Add a deletion
        overlay.delete("deleted_hash");

        // Add a file move
        overlay.record_move(PathBuf::from("old.rs"), PathBuf::from("new.rs"));

        // Save
        cache.save_layer(&overlay).expect("Failed to save");

        // Reload
        let loaded = cache.load_layer(LayerKind::Base)
            .expect("Failed to load")
            .expect("Should have layer");

        // Verify data matches
        assert_eq!(loaded.meta.indexed_sha, Some("abc123".to_string()));

        // Symbols includes both active and deleted (delete() adds to symbols map too)
        // We have 1 active symbol + 1 deleted symbol = 2 total entries
        assert_eq!(loaded.symbols.len(), 2);
        assert!(loaded.symbols.contains_key("hash_001"));
        // The deleted symbol is tracked in both deleted set and symbols map
        assert!(loaded.symbols.contains_key("deleted_hash"));
        assert!(loaded.deleted.contains("deleted_hash"));
        assert_eq!(loaded.moves.len(), 1);
        assert_eq!(loaded.moves[0].from_path, PathBuf::from("old.rs"));
        assert_eq!(loaded.moves[0].to_path, PathBuf::from("new.rs"));
    }

    /// TDD: test_backward_compat_v1_cache
    /// Verifies existing v1 sharded caches still work alongside new layer system
    #[test]
    fn test_backward_compat_v1_cache() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Initialize creates all directories including layers
        cache.init().expect("Failed to init");

        // V1 directories should exist
        assert!(cache.modules_dir().exists());
        assert!(cache.symbols_dir().exists());
        assert!(cache.graphs_dir().exists());
        assert!(cache.diffs_dir().exists());

        // V2 layers directory should also exist
        assert!(cache.layers_dir().exists());

        // Both systems can coexist
        assert!(cache.root.exists());
    }

    /// TDD: test_schema_version_bump
    /// Verifies schema version is 2.0 for layered index support
    #[test]
    fn test_schema_version_bump() {
        assert_eq!(SCHEMA_VERSION, "2.0", "Schema version should be 2.0 for SEM-45");
    }

    /// TDD: test_meta_json_structure
    /// Verifies meta.json has correct structure
    #[test]
    fn test_meta_json_structure() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create and save a layered index
        let mut index = LayeredIndex::new();
        index.base.meta.indexed_sha = Some("base_sha".to_string());
        index.branch.meta.indexed_sha = Some("branch_sha".to_string());
        // merge_base_sha belongs on the branch layer (where branch diverged from base)
        index.branch.meta.merge_base_sha = Some("merge_base".to_string());

        cache.save_layered_index(&index).expect("Failed to save");

        // Read and verify meta.json structure
        let meta_content = std::fs::read_to_string(cache.layer_meta_path()).expect("Read meta");
        let meta: LayeredIndexMeta = serde_json::from_str(&meta_content).expect("Parse meta");

        assert_eq!(meta.schema_version, SCHEMA_VERSION);
        assert!(meta.saved_at.len() > 0);
        assert_eq!(meta.base_indexed_sha, Some("base_sha".to_string()));
        assert_eq!(meta.branch_indexed_sha, Some("branch_sha".to_string()));
        assert_eq!(meta.merge_base, Some("merge_base".to_string()));
    }

    // ========================================================================
    // Layer Cache Error Scenario Tests
    // ========================================================================

    #[test]
    fn test_load_layer_corrupted_symbols_jsonl() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with corrupted symbols.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write corrupted symbols.jsonl (malformed JSON - truncated object simulating interrupted write)
        fs::write(layer_dir.join("symbols.jsonl"), "{\"hash\":\"abc123\",\"state\":\"active\",\"symbol\":{\"name\":\"test\"\n{\"hash\":\"incomplete\"").expect("Write corrupted symbols");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");
        fs::write(layer_dir.join("moves.jsonl"), "").expect("Write moves");

        // Should return error when loading
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_err(), "Should fail to load layer with corrupted symbols.jsonl");
        
        // Verify it's a deserialization error
        match result {
            Err(crate::McpDiffError::ExtractionFailure { message }) => {
                assert!(message.contains("deserialize") || message.contains("symbol entry"), 
                    "Error should indicate symbol deserialization issue: {}", message);
            }
            Err(e) => panic!("Expected ExtractionFailure error, got: {:?}", e),
            Ok(_) => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_load_layer_corrupted_moves_jsonl() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with corrupted moves.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write valid files except moves.jsonl
        fs::write(layer_dir.join("symbols.jsonl"), "").expect("Write symbols");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");
        // Write corrupted moves.jsonl (truncated JSON array simulating interrupted write)
        fs::write(layer_dir.join("moves.jsonl"), "{\"from_path\":\"old.rs\",\"to_path\":\"new.rs\",\"moved_at\":\n").expect("Write corrupted moves");

        // Should return error when loading
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_err(), "Should fail to load layer with corrupted moves.jsonl");
        
        // Verify it's a deserialization error
        match result {
            Err(crate::McpDiffError::ExtractionFailure { message }) => {
                assert!(message.contains("deserialize") || message.contains("file move"), 
                    "Error should indicate file move deserialization issue: {}", message);
            }
            Err(e) => panic!("Expected ExtractionFailure error, got: {:?}", e),
            Ok(_) => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_load_layer_missing_symbols_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with meta.json but missing symbols.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write other files but NOT symbols.jsonl
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");
        fs::write(layer_dir.join("moves.jsonl"), "").expect("Write moves");

        // Should succeed - missing files are treated as empty
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle missing symbols.jsonl gracefully");
        
        let overlay = result.unwrap();
        assert!(overlay.is_some(), "Should return an overlay");
        let overlay = overlay.unwrap();
        assert!(overlay.symbols.is_empty(), "Symbols should be empty when file is missing");
    }

    #[test]
    fn test_load_layer_missing_moves_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with meta.json but missing moves.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write other files but NOT moves.jsonl
        fs::write(layer_dir.join("symbols.jsonl"), "").expect("Write symbols");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");

        // Should succeed - missing files are treated as empty
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle missing moves.jsonl gracefully");
        
        let overlay = result.unwrap();
        assert!(overlay.is_some(), "Should return an overlay");
        let overlay = overlay.unwrap();
        assert!(overlay.moves.is_empty(), "Moves should be empty when file is missing");
    }

    #[test]
    fn test_load_layer_empty_symbols_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with zero-byte symbols.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write zero-byte files
        fs::write(layer_dir.join("symbols.jsonl"), "").expect("Write empty symbols");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write empty deleted");
        fs::write(layer_dir.join("moves.jsonl"), "").expect("Write empty moves");

        // Should succeed with empty collections
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle empty symbols.jsonl gracefully");
        
        let overlay = result.unwrap();
        assert!(overlay.is_some(), "Should return an overlay");
        let overlay = overlay.unwrap();
        assert!(overlay.symbols.is_empty(), "Symbols should be empty");
        assert!(overlay.deleted.is_empty(), "Deleted should be empty");
        assert!(overlay.moves.is_empty(), "Moves should be empty");
    }

    #[test]
    fn test_load_layer_empty_moves_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with zero-byte moves.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write files with zero-byte moves.jsonl
        fs::write(layer_dir.join("symbols.jsonl"), "").expect("Write symbols");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");
        fs::write(layer_dir.join("moves.jsonl"), "").expect("Write empty moves");

        // Should succeed with empty moves
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle empty moves.jsonl gracefully");
        
        let overlay = result.unwrap();
        assert!(overlay.is_some(), "Should return an overlay");
        let overlay = overlay.unwrap();
        assert!(overlay.moves.is_empty(), "Moves should be empty");
    }

    #[test]
    fn test_load_layer_invalid_utf8_content() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create layer directory with invalid UTF-8 in symbols.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        fs::create_dir_all(&layer_dir).expect("Create layer dir");

        // Write valid meta.json
        let meta = crate::overlay::LayerMeta::new(LayerKind::Base);
        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(layer_dir.join("meta.json"), meta_json).expect("Write meta");

        // Write invalid UTF-8 bytes to symbols.jsonl
        // These bytes (0xFF 0xFE 0xFD) are invalid in UTF-8 encoding
        // and simulate file corruption or encoding issues
        let invalid_utf8: Vec<u8> = vec![0xFF, 0xFE, 0xFD, 0x00];
        std::fs::write(layer_dir.join("symbols.jsonl"), invalid_utf8).expect("Write invalid UTF-8");
        fs::write(layer_dir.join("deleted.txt"), "").expect("Write deleted");
        fs::write(layer_dir.join("moves.jsonl"), "").expect("Write moves");

        // Should return error when reading invalid UTF-8
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_err(), "Should fail to load layer with invalid UTF-8");
    }

    #[test]
    fn test_load_layer_symbols_with_blank_lines() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a test overlay and save it
        let mut overlay = Overlay::new(LayerKind::Base);
        let symbol = crate::schema::SymbolInfo {
            name: "test_symbol".to_string(),
            kind: crate::schema::SymbolKind::Function,
            start_line: 1,
            end_line: 10,
            is_exported: true,
            ..Default::default()
        };
        overlay.symbols.insert(
            "test_hash".to_string(),
            crate::overlay::SymbolState::active(symbol),
        );

        cache.save_layer(&overlay).expect("Failed to save layer");

        // Now manually add blank lines to symbols.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        let symbols_path = layer_dir.join("symbols.jsonl");
        let mut content = fs::read_to_string(&symbols_path).expect("Read symbols");
        content.push_str("\n\n   \n\t\n"); // Add various blank lines
        fs::write(&symbols_path, content).expect("Write modified symbols");

        // Should handle blank lines gracefully
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle blank lines in symbols.jsonl");
        
        let loaded = result.unwrap();
        assert!(loaded.is_some(), "Should load the overlay");
        let loaded_overlay = loaded.unwrap();
        assert_eq!(loaded_overlay.symbols.len(), 1, "Should have one symbol despite blank lines");
    }

    #[test]
    fn test_load_layer_moves_with_blank_lines() {
        use tempfile::TempDir;
        use std::path::PathBuf;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a test overlay with a file move
        let mut overlay = Overlay::new(LayerKind::Base);
        overlay.moves.push(crate::overlay::FileMove::new(
            PathBuf::from("old.rs"),
            PathBuf::from("new.rs"),
        ));

        cache.save_layer(&overlay).expect("Failed to save layer");

        // Now manually add blank lines to moves.jsonl
        let layer_dir = cache.layer_dir(LayerKind::Base).unwrap();
        let moves_path = layer_dir.join("moves.jsonl");
        let mut content = fs::read_to_string(&moves_path).expect("Read moves");
        content.push_str("\n\n   \n\t\n"); // Add various blank lines
        fs::write(&moves_path, content).expect("Write modified moves");

        // Should handle blank lines gracefully
        let result = cache.load_layer(LayerKind::Base);
        assert!(result.is_ok(), "Should handle blank lines in moves.jsonl");
        
        let loaded = result.unwrap();
        assert!(loaded.is_some(), "Should load the overlay");
        let loaded_overlay = loaded.unwrap();
        assert_eq!(loaded_overlay.moves.len(), 1, "Should have one move despite blank lines");
    }

    /// TDD: test_test_file_exclusion_default
    /// Verifies test files are detected correctly
    #[test]
    fn test_test_file_exclusion_default() {
        use crate::search::is_test_file;

        // Should be detected as test files
        assert!(is_test_file("tests/test_api.rs"));
        assert!(is_test_file("src/lib_test.rs"));
        assert!(is_test_file("src/button.test.ts"));
        assert!(is_test_file("__tests__/component.tsx"));
        assert!(is_test_file("test_utils.py"));

        // Should NOT be detected as test files
        assert!(!is_test_file("src/lib.rs"));
        assert!(!is_test_file("src/main.py"));
        assert!(!is_test_file("src/index.ts"));
    }

    /// TDD: test_test_file_inclusion_flag
    /// Verifies --allow-tests flag exists in CLI
    #[test]
    fn test_test_file_inclusion_flag() {
        use crate::cli::Cli;
        use clap::Parser;

        // Test that --allow-tests flag is recognized
        let args = vec!["semfora-mcp", "--allow-tests", "test.rs"];
        let cli = Cli::try_parse_from(args).expect("Should parse with --allow-tests");
        assert!(cli.allow_tests, "--allow-tests should be true");

        // Without the flag, default is false
        let args = vec!["semfora-mcp", "test.rs"];
        let cli = Cli::try_parse_from(args).expect("Should parse without --allow-tests");
        assert!(!cli.allow_tests, "Default should be false");
    }

    // ========================================================================
    // Staleness Detection Tests
    // ========================================================================

    /// Test is_layer_stale returns true when no cached layer exists
    #[test]
    fn test_is_layer_stale_missing_layer() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // No layer exists, should be stale
        let result = cache.is_layer_stale(LayerKind::Base);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Missing layer should be stale");
    }

    /// Test is_base_layer_stale when indexed SHA is missing
    #[test]
    fn test_is_base_layer_stale_no_indexed_sha() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay without indexed_sha
        let overlay = Overlay::new(LayerKind::Base);
        assert!(overlay.meta.indexed_sha.is_none());

        // Should be stale when no indexed SHA
        let result = cache.is_base_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer without indexed SHA should be stale");
    }

    /// Test is_branch_layer_stale when indexed SHA is missing
    #[test]
    fn test_is_branch_layer_stale_no_indexed_sha() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay without indexed_sha
        let overlay = Overlay::new(LayerKind::Branch);
        assert!(overlay.meta.indexed_sha.is_none());

        // Should be stale when no indexed SHA
        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer without indexed SHA should be stale");
    }

    /// Test is_working_layer_stale when no files are tracked
    #[test]
    fn test_is_working_layer_stale_no_files() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with no tracked files
        let overlay = Overlay::new(LayerKind::Working);
        assert!(overlay.symbols_by_file.is_empty());

        // Should not be stale if no files are tracked
        let result = cache.is_working_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Layer with no tracked files should not be stale");
    }

    /// Test is_working_layer_stale when tracked file is deleted
    #[test]
    fn test_is_working_layer_stale_deleted_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay tracking a non-existent file
        let mut overlay = Overlay::new(LayerKind::Working);
        overlay.symbols_by_file.insert(
            PathBuf::from("nonexistent.rs"),
            Vec::new(),
        );

        // Should be stale when file is missing
        let result = cache.is_working_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer with deleted file should be stale");
    }

    /// Test is_working_layer_stale when tracked file is modified
    #[test]
    fn test_is_working_layer_stale_modified_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        // Create overlay with a very old update time (before file was modified)
        let mut overlay = Overlay::new(LayerKind::Working);
        overlay.meta.updated_at = 1; // Very old timestamp
        overlay.symbols_by_file.insert(
            PathBuf::from("test.rs"),
            Vec::new(),
        );

        // Should be stale because file mtime > overlay update time
        let result = cache.is_working_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer should be stale when file modified after cache");
    }

    /// Test is_working_layer_stale when tracked file is NOT modified
    #[test]
    fn test_is_working_layer_stale_unmodified_file() {
        use tempfile::TempDir;
        use std::time::{SystemTime, UNIX_EPOCH};

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").expect("Failed to write test file");

        // Create overlay with a very recent update time (after file was modified)
        let mut overlay = Overlay::new(LayerKind::Working);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        // Set to future time to ensure overlay is newer than the file
        const FUTURE_OFFSET_SECONDS: u64 = 1000;
        overlay.meta.updated_at = now + FUTURE_OFFSET_SECONDS;
        overlay.symbols_by_file.insert(
            PathBuf::from("test.rs"),
            Vec::new(),
        );

        // Should NOT be stale because overlay was updated after file modification
        let result = cache.is_working_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Layer should not be stale when updated after file");
    }

    /// Test is_layer_stale dispatching for different layer kinds
    #[test]
    fn test_is_layer_stale_dispatching() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // AI layer should always be stale (returns true for missing layer)
        let ai_result = cache.is_layer_stale(LayerKind::AI);
        assert!(ai_result.is_ok());
        // AI layer is never persisted, so loading returns None, making it "stale"
        assert!(ai_result.unwrap(), "AI layer should be stale (no cache)");
    }

    /// Test is_layered_index_stale when no cache exists
    #[test]
    fn test_is_layered_index_stale_no_cache() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // No cache exists
        assert!(!cache.has_cached_layers());

        // Should be stale
        let result = cache.is_layered_index_stale();
        assert!(result.is_ok());
        assert!(result.unwrap(), "Index should be stale when no cache exists");
    }

    /// Test is_layered_index_stale when base layer is stale
    #[test]
    fn test_is_layered_index_stale_base_stale() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create a layered index with base layer (no indexed_sha = stale)
        let index = LayeredIndex::new();
        cache.save_layered_index(&index).expect("Failed to save");

        // Base layer has no indexed_sha, so it's stale
        let result = cache.is_layered_index_stale();
        assert!(result.is_ok());
        assert!(result.unwrap(), "Index should be stale when base layer is stale");
    }

    // ========================================================================
    // Git Integration Tests for Staleness Detection
    // ========================================================================

    /// Test is_base_layer_stale with actual git repo - HEAD changes
    #[test]
    fn test_is_base_layer_stale_head_changed() {
        use tempfile::TempDir;
        use std::process::Command;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        
        // Initialize a git repository with main as default branch
        Command::new("git")
            .args(&["init", "-b", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git user for commits
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git email");

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git name");

        // Create initial commit on main
        std::fs::write(temp_dir.path().join("test.txt"), "initial").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "initial commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Get the current SHA
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let initial_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with the initial SHA
        let mut overlay = Overlay::new(LayerKind::Base);
        overlay.meta.indexed_sha = Some(initial_sha.clone());

        // Should NOT be stale - SHA matches
        let result = cache.is_base_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Layer should not be stale when SHA matches");

        // Create a new commit
        std::fs::write(temp_dir.path().join("test.txt"), "modified").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "second commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Now HEAD has moved, layer should be stale
        let result = cache.is_base_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer should be stale when HEAD has moved");
    }

    /// Test is_branch_layer_stale with actual git repo - branch HEAD moves
    #[test]
    fn test_is_branch_layer_stale_head_moved() {
        use tempfile::TempDir;
        use std::process::Command;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        
        // Initialize a git repository with main as default branch
        Command::new("git")
            .args(&["init", "-b", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git email");

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git name");

        // Create initial commit
        std::fs::write(temp_dir.path().join("test.txt"), "initial").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "initial commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Create a branch
        Command::new("git")
            .args(&["checkout", "-b", "feature"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to create branch");

        // Get the current SHA
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let branch_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with the branch SHA
        let mut overlay = Overlay::new(LayerKind::Branch);
        overlay.meta.indexed_sha = Some(branch_sha.clone());

        // Should NOT be stale - SHA matches
        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Layer should not be stale when SHA matches");

        // Create a new commit on branch
        std::fs::write(temp_dir.path().join("feature.txt"), "feature").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "feature.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "feature commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Now HEAD has moved, layer should be stale
        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer should be stale when branch HEAD has moved");
    }

    /// Test is_branch_layer_stale with actual git repo - merge base changes (rebase)
    #[test]
    fn test_is_branch_layer_stale_merge_base_changed() {
        use tempfile::TempDir;
        use std::process::Command;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        
        // Initialize a git repository with main as default branch
        Command::new("git")
            .args(&["init", "-b", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git email");

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git name");

        // Create initial commit on main
        std::fs::write(temp_dir.path().join("base.txt"), "base").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "base.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "base commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Get initial merge base (same as HEAD at this point)
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let initial_merge_base = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Create a branch
        Command::new("git")
            .args(&["checkout", "-b", "feature"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to create branch");

        // Make a commit on feature
        std::fs::write(temp_dir.path().join("feature.txt"), "feature").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "feature.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "feature commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let feature_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Get the current merge-base (should be initial commit)
        let output = Command::new("git")
            .args(&["merge-base", "HEAD", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get merge-base");
        let current_merge_base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(initial_merge_base, current_merge_base, "Merge base should still be initial commit");

        // Switch to main and add another commit
        Command::new("git")
            .args(&["checkout", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to checkout main");

        std::fs::write(temp_dir.path().join("main.txt"), "main progress").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "main.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "main progress"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let new_main_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Switch back to feature
        Command::new("git")
            .args(&["checkout", "feature"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to checkout feature");

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with current HEAD and old merge base
        let mut overlay = Overlay::new(LayerKind::Branch);
        overlay.meta.indexed_sha = Some(feature_sha.clone());
        overlay.meta.merge_base_sha = Some(initial_merge_base.clone());

        // Merge base hasn't changed yet (still at initial commit even after main moved forward)
        // The merge-base of feature and main is still the initial commit
        let output = Command::new("git")
            .args(&["merge-base", "HEAD", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get merge-base");
        let actual_merge_base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(initial_merge_base, actual_merge_base, "Merge base should still be initial commit after main moves");

        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        // Merge-base is still the same, so should NOT be stale
        assert!(!result.unwrap(), "Layer should not be stale when merge-base hasn't changed");

        // Now rebase onto main - this will change the merge-base
        let _rebase_result = Command::new("git")
            .args(&["rebase", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to rebase");

        // After rebase, HEAD SHA has changed (feature commit was rewritten)
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let rebased_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_ne!(feature_sha, rebased_sha, "SHA should change after rebase");

        // After rebase, merge-base is now the new main SHA (where we rebased onto)
        let output = Command::new("git")
            .args(&["merge-base", "HEAD", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get merge-base");
        let new_merge_base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(new_main_sha, new_merge_base, "After rebase, merge-base should be the new main HEAD");

        // The layer should be stale because HEAD changed (from original feature_sha to rebased_sha)
        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Layer should be stale after rebase (HEAD changed)");
    }

    /// Test edge case: missing git reference
    #[test]
    fn test_is_base_layer_stale_missing_git_ref() {
        use tempfile::TempDir;
        use std::process::Command;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        
        // Initialize a git repository with main as default branch
        Command::new("git")
            .args(&["init", "-b", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to init git");

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with indexed SHA but no commits in repo (no main/master)
        let mut overlay = Overlay::new(LayerKind::Base);
        overlay.meta.indexed_sha = Some("abc123".to_string());

        // Should return an error or report stale when git ref is missing
        let result = cache.is_base_layer_stale(&overlay);
        // The function will fail when trying to detect base branch or get ref SHA
        match result {
            Err(_) => {
                // Error is acceptable - no commits yet
            }
            Ok(is_stale) => {
                // If it returns Ok, it should report stale
                assert!(is_stale, "Should report stale when git ref is missing");
            }
        }
    }

    /// Test edge case: detached HEAD
    #[test]
    fn test_is_branch_layer_stale_detached_head() {
        use tempfile::TempDir;
        use std::process::Command;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        
        // Initialize a git repository with main as default branch
        Command::new("git")
            .args(&["init", "-b", "main"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git email");

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to config git name");

        // Create initial commit
        std::fs::write(temp_dir.path().join("test.txt"), "initial").expect("Failed to write file");
        Command::new("git")
            .args(&["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(&["commit", "-m", "initial commit"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to git commit");

        // Get the commit SHA
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to get HEAD");
        let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Detach HEAD
        Command::new("git")
            .args(&["checkout", &commit_sha])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to detach HEAD");

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Create overlay with the current SHA
        let mut overlay = Overlay::new(LayerKind::Branch);
        overlay.meta.indexed_sha = Some(commit_sha.clone());

        // Even in detached HEAD state, if SHA matches, it shouldn't be stale
        let result = cache.is_branch_layer_stale(&overlay);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Layer should not be stale in detached HEAD if SHA matches");
    }

    // ========================================================================
    // Ripgrep Fallback Tests (SEM-55)
    // ========================================================================

    #[test]
    fn test_fallback_when_no_index() {
        use tempfile::TempDir;

        // Create a temp directory with some Rust files but NO .semfora index
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a simple Rust file
        fs::write(
            temp_dir.path().join("main.rs"),
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        ).unwrap();

        fs::write(
            temp_dir.path().join("lib.rs"),
            "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n",
        ).unwrap();

        // Create a CacheDir pointing to this repo (no index exists)
        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Verify no symbol index exists
        assert!(!cache.has_symbol_index(), "Should not have symbol index");

        // Test search_symbols_with_fallback - should use ripgrep
        let result = cache.search_symbols_with_fallback("fn main", None, None, None, 20);
        assert!(result.is_ok(), "Fallback search should succeed");

        let result = result.unwrap();
        assert!(result.fallback_used, "Should use fallback (ripgrep)");
        assert!(result.indexed_results.is_none(), "Should not have indexed results");
        assert!(result.ripgrep_results.is_some(), "Should have ripgrep results");

        let ripgrep_results = result.ripgrep_results.unwrap();
        assert!(!ripgrep_results.is_empty(), "Should find matches with ripgrep");

        // Verify we found the main function
        let found_main = ripgrep_results.iter().any(|r| r.content.contains("fn main"));
        assert!(found_main, "Should find 'fn main' in ripgrep results");
    }

    #[test]
    fn test_fallback_with_query_syntax() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create files with specific patterns
        fs::write(
            temp_dir.path().join("test.rs"),
            r#"
            fn validate_email(email: &str) -> bool {
                email.contains('@')
            }

            fn validate_password(password: &str) -> bool {
                password.len() >= 8
            }

            struct UserValidator {
                strict: bool,
            }
            "#,
        ).unwrap();

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Test regex pattern search
        let result = cache.search_symbols_with_fallback(
            r"fn validate_\w+",  // Regex pattern
            None,
            None,
            None,
            20,
        );
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.fallback_used);

        let results = result.ripgrep_results.unwrap();
        // Should find both validate_email and validate_password
        let found_email = results.iter().any(|r| r.content.contains("validate_email"));
        let found_password = results.iter().any(|r| r.content.contains("validate_password"));
        assert!(found_email, "Should find validate_email with regex");
        assert!(found_password, "Should find validate_password with regex");
    }

    #[test]
    fn test_search_with_ripgrep_direct() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create test files
        fs::write(
            temp_dir.path().join("api.rs"),
            "// API module\nfn handle_request() {}\n",
        ).unwrap();

        fs::write(
            temp_dir.path().join("utils.rs"),
            "// Utility functions\nfn helper() {}\n",
        ).unwrap();

        fs::write(
            temp_dir.path().join("config.yaml"),
            "# Configuration\napi_key: secret\n",
        ).unwrap();

        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: temp_dir.path().to_path_buf(),
            repo_hash: "test_hash".to_string(),
        };

        // Test direct ripgrep search
        let result = cache.search_with_ripgrep("API", None, 20);
        assert!(result.is_ok());

        let results = result.unwrap();
        // Should find API in both api.rs (comment) and potentially config.yaml
        assert!(!results.is_empty(), "Should find API references");

        // Test with file type filter - only .rs files
        let result_rs_only = cache.search_with_ripgrep(
            "API",
            Some(vec!["rs".to_string()]),
            20,
        );
        assert!(result_rs_only.is_ok());

        let rs_results = result_rs_only.unwrap();
        for r in &rs_results {
            assert!(r.file.ends_with(".rs"), "Should only return .rs files");
        }
    }

    #[test]
    fn test_kind_to_file_type_inference() {
        // Test that we infer correct file types from symbol kinds

        // Component kind should search React/Vue/Svelte files
        let component_types = CacheDir::infer_file_types_from_kind(Some("component"));
        assert!(component_types.is_some());
        let types = component_types.unwrap();
        assert!(types.contains(&"tsx".to_string()));
        assert!(types.contains(&"jsx".to_string()));

        // Rust-specific kinds
        let struct_types = CacheDir::infer_file_types_from_kind(Some("struct"));
        assert!(struct_types.is_some());
        assert!(struct_types.unwrap().contains(&"rs".to_string()));

        // Generic function kind should search all files
        let fn_types = CacheDir::infer_file_types_from_kind(Some("fn"));
        assert!(fn_types.is_none(), "Function kind should search all files");

        // Unknown kind should search all files
        let unknown_types = CacheDir::infer_file_types_from_kind(None);
        assert!(unknown_types.is_none());
    }

    #[test]
    fn test_working_overlay_searches_modified_only() {
        // Test that working overlay mode only searches uncommitted files.
        // This test creates a mock scenario where we verify the method signature
        // and behavior pattern. Full integration requires a git repo with uncommitted changes.

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path().to_path_buf();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_root)
            .output()
            .expect("git init");

        // Create a committed file
        let committed_file = repo_root.join("committed.rs");
        std::fs::write(&committed_file, "fn committed_function() {}\n").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_root)
            .output()
            .expect("git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&repo_root)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git commit");

        // Create an uncommitted file with a searchable term
        let uncommitted_file = repo_root.join("uncommitted.rs");
        std::fs::write(&uncommitted_file, "fn SEARCHABLE_uncommitted() {}\n").unwrap();

        // Create cache dir
        let cache = CacheDir {
            root: temp_dir.path().join(".semfora"),
            repo_root: repo_root.clone(),
            repo_hash: "test_hash".to_string(),
        };

        // Search working overlay for a term that only exists in uncommitted file
        let results = cache.search_working_overlay("SEARCHABLE", None, 20).unwrap();

        // Should find results only in uncommitted file
        assert!(!results.is_empty(), "Should find results in uncommitted file");
        for r in &results {
            assert!(
                r.file == "uncommitted.rs" || r.file.ends_with("uncommitted.rs"),
                "Results should only be from uncommitted files, got: {}",
                r.file
            );
        }

        // Search for term only in committed file - should return empty
        let committed_results = cache.search_working_overlay("committed_function", None, 20).unwrap();
        assert!(
            committed_results.is_empty(),
            "Should NOT find results in committed files when using working overlay"
        );
    }
}
