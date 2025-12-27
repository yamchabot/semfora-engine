//! Sharded IR writer for massive repository support
//!
//! Splits semantic analysis output into queryable shards:
//! - repo_overview.toon - High-level architecture
//! - modules/{name}.toon - Per-module semantic slices
//! - symbols/{hash}.toon - Individual symbol details
//! - graphs/*.toon - Dependency and call graphs

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{calculate_cognitive_complexity, max_nesting_depth};
use crate::bm25::{extract_terms_from_file_path, Bm25Document};
use crate::cache::{CacheDir, IndexingStatus, SourceFileInfo};
use crate::duplicate::FunctionSignature;
use crate::error::Result;
use crate::module_registry::ModuleRegistrySqlite;
use crate::schema::{
    CallGraphEdge, RefKind, RepoOverview, RiskLevel, SemanticSummary, SymbolId, SymbolInfo,
    SymbolKind, SCHEMA_VERSION,
};
use crate::toon::{encode_toon, generate_repo_overview_with_modules, is_meaningful_call};
use rusqlite::Connection;

/// Package version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Generate TOON header with type and version for shard files
#[inline]
fn toon_header(type_name: &str) -> String {
    format!("_type: {}\nversion: {}", type_name, VERSION)
}

fn emit_progress(
    progress: &Option<ShardProgressCallback>,
    step: &str,
    current: usize,
    total: usize,
) {
    if let Some(cb) = progress {
        cb(step, current, total);
    }
}

// ============================================================================
// Module Registry - Conflict-Aware Module Name Stripping
// ============================================================================

/// Registry that maps full module paths to optimally shortened names.
///
/// The algorithm iteratively strips the first path component from ALL module
/// names until doing so would create a duplicate (conflict).
///
/// Example:
/// ```text
/// Input:  src.game.player, src.game.enemy, src.map.player
/// Strip 1: game.player, game.enemy, map.player (no conflict, accept)
/// Strip 2: player, enemy, player (conflict! stop)
/// Result: game.player, game.enemy, map.player
/// ```
#[derive(Debug, Clone)]
pub struct ModuleRegistry {
    /// Full module path → shortened name
    full_to_short: HashMap<String, String>,

    /// Shortened name → full module path (for conflict detection)
    short_to_full: HashMap<String, String>,

    /// Current global strip depth applied to all modules
    strip_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct IndexProgress {
    version: u32,
    stages: HashMap<String, StageProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StageProgress {
    status: String,
}

impl ModuleRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            full_to_short: HashMap::new(),
            short_to_full: HashMap::new(),
            strip_depth: 0,
        }
    }

    /// Build a registry from a list of full module paths
    pub fn from_full_paths(full_paths: &[String]) -> Self {
        let (short_names, strip_depth) = compute_optimal_names(full_paths);

        let mut registry = Self {
            full_to_short: HashMap::new(),
            short_to_full: HashMap::new(),
            strip_depth,
        };

        for (full, short) in full_paths.iter().zip(short_names.iter()) {
            registry.full_to_short.insert(full.clone(), short.clone());
            registry.short_to_full.insert(short.clone(), full.clone());
        }

        registry
    }

    /// Get the shortened name for a full module path
    pub fn get_short(&self, full_path: &str) -> Option<&String> {
        self.full_to_short.get(full_path)
    }

    /// Get the full path for a shortened name
    #[allow(dead_code)]
    pub fn get_full(&self, short_name: &str) -> Option<&String> {
        self.short_to_full.get(short_name)
    }

    /// Get the current strip depth
    #[allow(dead_code)]
    pub fn strip_depth(&self) -> usize {
        self.strip_depth
    }

    /// Check if a shortened name already exists (would cause conflict)
    #[allow(dead_code)]
    pub fn has_conflict(&self, short_name: &str) -> bool {
        self.short_to_full.contains_key(short_name)
    }

    /// Get all shortened module names
    pub fn short_names(&self) -> impl Iterator<Item = &String> {
        self.short_to_full.keys()
    }

    /// Number of modules in the registry
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.full_to_short.len()
    }

    /// Check if registry is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.full_to_short.is_empty()
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute optimal shortened names for a list of full module paths.
///
/// Returns the shortened names (same order as input) and the strip depth used.
///
/// Algorithm: Iteratively strip the first component from ALL multi-component names
/// until doing so would create a duplicate. Single-component modules are preserved
/// as-is since they're already minimal and can't be stripped further.
///
/// This handles the case where a codebase has files in the root (creating "root"
/// modules) alongside deeply nested modules - the single-component modules won't
/// block stripping for the multi-component ones.
fn compute_optimal_names(full_paths: &[String]) -> (Vec<String>, usize) {
    if full_paths.is_empty() {
        return (Vec::new(), 0);
    }

    // Separate single-component modules (can't be stripped) from multi-component
    // Single-component = no dots in the path
    let mut result = vec![String::new(); full_paths.len()];
    let mut multi_indices: Vec<usize> = Vec::new();
    let mut multi_paths: Vec<String> = Vec::new();
    let mut single_names: HashSet<String> = HashSet::new();

    for (i, path) in full_paths.iter().enumerate() {
        if path.contains('.') {
            // Multi-component - will be processed by stripping algorithm
            multi_indices.push(i);
            multi_paths.push(path.clone());
        } else {
            // Single-component - preserve as-is (already minimal)
            result[i] = path.clone();
            single_names.insert(path.clone());
        }
    }

    // If no multi-component modules, nothing to strip
    if multi_paths.is_empty() {
        return (result, 0);
    }

    // Run the stripping algorithm on multi-component modules only
    let mut current_names = multi_paths;
    let mut strip_depth = 0;

    loop {
        // Try stripping one more level from each name
        let stripped: Vec<Option<String>> = current_names
            .iter()
            .map(|name| strip_first_component(name))
            .collect();

        // If any name can't be stripped (became single component), stop
        if stripped.iter().any(|s| s.is_none()) {
            break;
        }

        let stripped: Vec<String> = stripped.into_iter().flatten().collect();

        // Check for conflicts among multi-component names
        let unique: HashSet<&String> = stripped.iter().collect();
        if unique.len() < stripped.len() {
            // Conflict detected among multi-component names
            break;
        }

        // Check for conflicts with single-component names
        let conflicts_with_single = stripped.iter().any(|s| single_names.contains(s));
        if conflicts_with_single {
            // Stripping would conflict with an existing single-component name
            break;
        }

        // No conflict - accept this stripping level
        current_names = stripped;
        strip_depth += 1;
    }

    // Put the stripped multi-component names back in their original positions
    for (idx, stripped_name) in multi_indices.iter().zip(current_names.iter()) {
        result[*idx] = stripped_name.clone();
    }

    (result, strip_depth)
}

/// Public wrapper for `compute_optimal_names` - exposed for integration testing.
///
/// This function is used by integration tests to verify the module naming algorithm
/// works correctly without needing to go through the full index generation pipeline.
#[doc(hidden)]
pub fn compute_optimal_names_public(full_paths: &[String]) -> (Vec<String>, usize) {
    compute_optimal_names(full_paths)
}

/// Strip the first component from a dotted module path.
///
/// Returns None if the path has only one component.
///
/// Examples:
/// - "src.game.player" -> Some("game.player")
/// - "player" -> None
fn strip_first_component(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() <= 1 {
        return None;
    }
    Some(parts[1..].join("."))
}

/// Strip the first n components from a dotted module path.
///
/// Returns the original name if n is 0 or greater than component count.
#[allow(dead_code)]
fn strip_n_components(name: &str, n: usize) -> String {
    if n == 0 {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split('.').collect();
    if n >= parts.len() {
        return name.to_string();
    }
    parts[n..].join(".")
}

/// Compute the full module path from a file path.
///
/// This returns the raw dotted path based on directory structure,
/// WITHOUT any hardcoded marker stripping. The conflict-aware algorithm
/// will determine optimal stripping.
///
/// Example: "/home/user/project/src/game/player.rs" -> "src.game.player"
pub fn compute_full_module_path(file_path: &str) -> String {
    let path = std::path::Path::new(file_path);

    // Get the parent directory path
    let parent = match path.parent() {
        Some(p) => p,
        None => return "root".to_string(),
    };

    // Convert path to components, filtering out empty and common root paths
    let components: Vec<&str> = parent
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    if components.is_empty() {
        // File is in root directory - use filename without extension
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("root");

        // Skip generic names
        if matches!(stem, "index" | "mod" | "lib" | "main" | "__init__") {
            return "root".to_string();
        }
        return stem.to_string();
    }

    // Join components with dots
    components.join(".")
}

/// Write sharded IR output for a repository
pub struct ShardWriter {
    /// Cache directory manager
    cache: CacheDir,

    /// Repository root path (for computing relative module paths)
    repo_root: String,

    /// Summaries organized by FULL module path (before optimal stripping)
    /// Keys are raw dotted paths like "src.game.player"
    modules: HashMap<String, Vec<SemanticSummary>>,

    /// All summaries for graph building
    all_summaries: Vec<SemanticSummary>,

    /// Repository overview
    overview: Option<RepoOverview>,

    /// Indexing progress
    progress: IndexingStatus,

    /// Module name registry (computed at write time)
    module_registry: Option<ModuleRegistry>,
}

pub type ShardProgressCallback = Arc<dyn Fn(&str, usize, usize) + Send + Sync>;

impl ShardWriter {
    /// Create a new shard writer for a repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let cache = CacheDir::for_repo(repo_path)?;
        cache.init()?;

        let repo_root = repo_path
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            cache,
            repo_root,
            modules: HashMap::new(),
            all_summaries: Vec::new(),
            overview: None,
            progress: IndexingStatus::default(),
            module_registry: None,
        })
    }

    /// Create a shard writer with a custom cache directory
    /// Useful for worktrees where we need to use CacheDir::for_worktree
    pub fn with_cache(cache: CacheDir) -> Result<Self> {
        cache.init()?;

        Ok(Self {
            cache,
            repo_root: String::new(), // Will use extract_module_name fallback
            modules: HashMap::new(),
            all_summaries: Vec::new(),
            overview: None,
            progress: IndexingStatus::default(),
            module_registry: None,
        })
    }

    /// Add summaries to be sharded
    pub fn add_summaries(&mut self, summaries: Vec<SemanticSummary>) {
        // Organize by full module path (relative to repo root)
        for summary in &summaries {
            let module_name = self.compute_module_path(&summary.file);
            self.modules
                .entry(module_name)
                .or_insert_with(Vec::new)
                .push(summary.clone());
        }

        self.all_summaries.extend(summaries);
    }

    /// Compute the full module path for a file (relative to repo root).
    ///
    /// This returns the raw dotted path WITHOUT hardcoded marker stripping.
    /// The conflict-aware algorithm will determine optimal stripping at write time.
    fn compute_module_path(&self, file_path: &str) -> String {
        // Strip repo root prefix if present
        let relative = if !self.repo_root.is_empty() && file_path.starts_with(&self.repo_root) {
            file_path[self.repo_root.len()..].trim_start_matches('/')
        } else {
            file_path
        };

        let path = std::path::Path::new(relative);

        // Get parent directory (module path is based on directory structure)
        let parent = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => {
                // File in root - use filename without extension
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("root");

                // Skip generic names
                if matches!(stem, "index" | "mod" | "lib" | "main" | "__init__") {
                    return "root".to_string();
                }
                return stem.to_string();
            }
        };

        // Convert path components to dotted notation
        let components: Vec<&str> = parent
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();

        if components.is_empty() {
            return "root".to_string();
        }

        components.join(".")
    }

    /// Compute the module registry with optimal names.
    ///
    /// This builds a registry that maps full module paths to optimally
    /// shortened names using conflict-aware stripping.
    fn compute_module_registry(&mut self) {
        let full_paths: Vec<String> = self.modules.keys().cloned().collect();
        self.module_registry = Some(ModuleRegistry::from_full_paths(&full_paths));
    }

    /// Persist the module registry to SQLite for incremental indexing support.
    ///
    /// The SQLite file is stored in the cache directory alongside other index files.
    /// This enables O(1) lookups during future incremental indexing operations.
    fn persist_module_registry(&self) -> Result<()> {
        let Some(ref registry) = self.module_registry else {
            return Ok(()); // Nothing to persist
        };

        let mut sqlite_reg = ModuleRegistrySqlite::open(&self.cache)?;

        // Build entries: (full_path, short_name, file_path)
        let entries: Vec<(String, String, String)> = self
            .modules
            .iter()
            .map(|(full_path, summaries)| {
                let short_name = registry
                    .get_short(full_path)
                    .cloned()
                    .unwrap_or_else(|| full_path.clone());
                let file_path = summaries
                    .first()
                    .map(|s| s.file.clone())
                    .unwrap_or_default();
                (full_path.clone(), short_name, file_path)
            })
            .collect();

        sqlite_reg.bulk_insert(&entries, registry.strip_depth())?;

        Ok(())
    }

    /// Get the optimal (shortened) name for a full module path.
    ///
    /// Falls back to the full path if no registry is available.
    fn get_optimal_module_name(&self, full_path: &str) -> String {
        if let Some(ref registry) = self.module_registry {
            registry
                .get_short(full_path)
                .cloned()
                .unwrap_or_else(|| full_path.to_string())
        } else {
            full_path.to_string()
        }
    }

    /// Create a mapping from file paths to optimal module names.
    ///
    /// This is used for consistent module naming across overview and shards.
    fn build_file_to_module_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();

        for (full_module_path, summaries) in &self.modules {
            let optimal_name = self.get_optimal_module_name(full_module_path);
            for summary in summaries {
                map.insert(summary.file.clone(), optimal_name.clone());
            }
        }

        map
    }

    /// Generate and write all shards
    pub fn write_all(&mut self, dir_path: &str) -> Result<ShardStats> {
        self.write_all_with_progress(dir_path, None)
    }

    /// Generate and write all shards with optional progress updates.
    pub fn write_all_with_progress(
        &mut self,
        dir_path: &str,
        progress: Option<ShardProgressCallback>,
    ) -> Result<ShardStats> {
        let mut stats = ShardStats::default();
        let mut progress_state = self.load_progress_state();

        // Compute optimal module names using conflict-aware stripping
        emit_progress(&progress, "Module registry", 0, 1);
        self.compute_module_registry();

        // Persist registry to SQLite (Phase 2 - enables incremental indexing)
        emit_progress(&progress, "Module registry", 1, 1);
        emit_progress(&progress, "Persist registry", 0, 1);
        self.persist_module_registry()?;
        emit_progress(&progress, "Persist registry", 1, 1);

        // Generate overview first (fast, gives agents something to work with)
        if !self.stage_completed(
            "repo_overview",
            &[self.cache.repo_overview_path()],
            &progress_state,
        ) {
            emit_progress(&progress, "Repo overview", 0, 1);
            self.write_repo_overview(dir_path, &mut stats)?;
            emit_progress(&progress, "Repo overview", 1, 1);
            self.mark_stage_completed("repo_overview", &mut progress_state)?;
        }

        // Write module shards (using optimal names from registry)
        if !self.stage_completed("module_shards", &[], &progress_state) {
            emit_progress(&progress, "Module shards", 0, 1);
            self.write_module_shards(&mut stats)?;
            emit_progress(&progress, "Module shards", 1, 1);
            self.mark_stage_completed("module_shards", &mut progress_state)?;
        }

        // Write symbol shards
        if !self.stage_completed("symbol_shards", &[], &progress_state) {
            emit_progress(&progress, "Symbol shards", 0, 1);
            self.write_symbol_shards(&mut stats, &progress)?;
            emit_progress(&progress, "Symbol shards", 1, 1);
            self.mark_stage_completed("symbol_shards", &mut progress_state)?;
        }

        // Write graph shards
        if !self.stage_completed(
            "graph_shards",
            &[
                self.cache.call_graph_path(),
                self.cache.import_graph_path(),
                self.cache.module_graph_path(),
            ],
            &progress_state,
        ) {
            self.write_graph_shards(&mut stats, &progress)?;
            self.mark_stage_completed("graph_shards", &mut progress_state)?;
        }

        // Write symbol index (query-driven API v1)
        if !self.stage_completed(
            "symbol_index",
            &[self.cache.symbol_index_path()],
            &progress_state,
        ) {
            emit_progress(&progress, "Symbol index", 0, 1);
            self.write_symbol_index(&mut stats)?;
            emit_progress(&progress, "Symbol index", 1, 1);
            self.mark_stage_completed("symbol_index", &mut progress_state)?;
        }

        // Write function signature index (duplicate detection)
        if !self.stage_completed(
            "signature_index",
            &[self.cache.signature_index_path()],
            &progress_state,
        ) {
            emit_progress(&progress, "Signature index", 0, 1);
            self.write_signature_index(&mut stats)?;
            emit_progress(&progress, "Signature index", 1, 1);
            self.mark_stage_completed("signature_index", &mut progress_state)?;
        }

        // Write BM25 semantic search index (Phase 3)
        if !self.stage_completed(
            "bm25_index",
            &[self.cache.bm25_index_path()],
            &progress_state,
        ) {
            emit_progress(&progress, "BM25 index", 0, 1);
            self.write_bm25_index(&mut stats, &progress)?;
            emit_progress(&progress, "BM25 index", 1, 1);
            self.mark_stage_completed("bm25_index", &mut progress_state)?;
        }

        Ok(stats)
    }

    /// Write the repository overview
    fn write_repo_overview(&mut self, dir_path: &str, stats: &mut ShardStats) -> Result<()> {
        // Build file-to-module mapping for consistent naming with module shards
        let file_to_module = self.build_file_to_module_map();

        let overview = generate_repo_overview_with_modules(
            &self.all_summaries,
            dir_path,
            Some(&file_to_module),
        );
        self.overview = Some(overview.clone());

        // Create TOON output with metadata
        let toon = encode_repo_overview_with_meta(&overview, &self.progress);

        // Write to cache
        let path = self.cache.repo_overview_path();
        let mut file = fs::File::create(&path)?;
        file.write_all(toon.as_bytes())?;

        stats.overview_bytes = toon.len();
        stats.files_written += 1;

        Ok(())
    }

    fn progress_path(&self) -> std::path::PathBuf {
        self.cache.root.join("progress.json")
    }

    fn load_progress_state(&self) -> IndexProgress {
        let path = self.progress_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<IndexProgress>(&content) {
                return state;
            }
        }
        IndexProgress {
            version: 1,
            stages: HashMap::new(),
        }
    }

    fn mark_stage_completed(&self, stage: &str, state: &mut IndexProgress) -> Result<()> {
        state.stages.insert(
            stage.to_string(),
            StageProgress {
                status: "completed".to_string(),
            },
        );
        self.save_progress_state(state)
    }

    fn save_progress_state(&self, state: &IndexProgress) -> Result<()> {
        let path = self.progress_path();
        let content = serde_json::to_string_pretty(state).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to serialize progress state: {}", e),
            }
        })?;
        fs::write(path, content)?;
        Ok(())
    }

    fn stage_completed(
        &self,
        stage: &str,
        required_files: &[std::path::PathBuf],
        state: &IndexProgress,
    ) -> bool {
        let completed = state
            .stages
            .get(stage)
            .map(|s| s.status == "completed")
            .unwrap_or(false);
        if !completed {
            return false;
        }
        required_files.iter().all(|p| p.exists())
    }

    /// Write per-module shards
    ///
    /// Uses the module registry to get optimal (shortened) names for shards.
    fn write_module_shards(&self, stats: &mut ShardStats) -> Result<()> {
        for (full_module_path, summaries) in &self.modules {
            // Get the optimal shortened name from the registry
            let optimal_name = self.get_optimal_module_name(full_module_path);

            let toon = encode_module_shard(&optimal_name, summaries, &self.cache.repo_root);
            let path = self.cache.module_path(&optimal_name);

            let mut file = fs::File::create(&path)?;
            file.write_all(toon.as_bytes())?;

            stats.module_bytes += toon.len();
            stats.modules_written += 1;
        }

        stats.files_written += stats.modules_written;
        Ok(())
    }

    /// Write per-symbol shards
    ///
    /// This now iterates over summary.symbols to capture ALL symbols in each file,
    /// not just the primary symbol. This is the key fix for multi-symbol files.
    fn write_symbol_shards(
        &self,
        stats: &mut ShardStats,
        progress: &Option<ShardProgressCallback>,
    ) -> Result<()> {
        let total_symbols: usize = self
            .all_summaries
            .iter()
            .map(|summary| {
                if !summary.symbols.is_empty() {
                    summary.symbols.len()
                } else if summary.symbol_id.is_some() {
                    1
                } else {
                    0
                }
            })
            .sum();
        let mut processed = 0usize;

        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);
                    let toon = encode_symbol_shard_from_info(summary, symbol_info, &symbol_id);
                    let path = self.cache.symbol_path(&symbol_id.hash);

                    let mut file = fs::File::create(&path)?;
                    file.write_all(toon.as_bytes())?;

                    stats.symbol_bytes += toon.len();
                    stats.symbols_written += 1;
                    processed += 1;
                    if processed % 1000 == 0 || processed == total_symbols {
                        emit_progress(progress, "Symbol shards", processed, total_symbols.max(1));
                    }
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format for backward compatibility
                let toon = encode_symbol_shard(summary);
                let path = self.cache.symbol_path(&symbol_id.hash);

                let mut file = fs::File::create(&path)?;
                file.write_all(toon.as_bytes())?;

                stats.symbol_bytes += toon.len();
                stats.symbols_written += 1;
                processed += 1;
                if processed % 1000 == 0 || processed == total_symbols {
                    emit_progress(progress, "Symbol shards", processed, total_symbols.max(1));
                }
            }
        }

        stats.files_written += stats.symbols_written;
        Ok(())
    }

    /// Write graph shards (call graph, import graph, module graph)
    fn write_graph_shards(
        &self,
        stats: &mut ShardStats,
        progress: &Option<ShardProgressCallback>,
    ) -> Result<()> {
        // Build and write call graph
        emit_progress(progress, "Call graph", 0, self.all_summaries.len());
        let call_graph = build_call_graph(&self.all_summaries, progress);
        let graph_bytes = write_call_graph(&self.cache.call_graph_path(), &call_graph)?;
        stats.graph_bytes += graph_bytes;
        emit_progress(
            progress,
            "Call graph",
            self.all_summaries.len(),
            self.all_summaries.len(),
        );

        // Build and write import graph
        emit_progress(progress, "Import graph", 0, 1);
        let import_graph = build_import_graph(&self.all_summaries);
        let import_graph_toon = encode_import_graph(&import_graph);
        fs::write(self.cache.import_graph_path(), &import_graph_toon)?;
        stats.graph_bytes += import_graph_toon.len();
        emit_progress(progress, "Import graph", 1, 1);

        // Build file-to-module mapping for proper module names from registry
        let file_to_module = self.build_file_to_module_map();

        // Build and write module graph
        emit_progress(progress, "Module graph", 0, 1);
        let module_graph = build_module_graph(&self.modules, &file_to_module);
        let module_graph_toon = encode_module_graph(&module_graph);
        fs::write(self.cache.module_graph_path(), &module_graph_toon)?;
        stats.graph_bytes += module_graph_toon.len();
        emit_progress(progress, "Module graph", 1, 1);

        stats.files_written += 3;
        Ok(())
    }

    /// Write the lightweight symbol index for query-driven access
    ///
    /// Now writes entries for ALL symbols in summary.symbols, not just the primary one.
    fn write_symbol_index(&self, stats: &mut ShardStats) -> Result<()> {
        use crate::cache::SymbolIndexEntry;

        let path = self.cache.symbol_index_path();
        let mut file = fs::File::create(&path)?;

        // Build file-to-module mapping for proper module names from registry
        let file_to_module = self.build_file_to_module_map();

        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);
            // Get the optimal module name from registry, fallback to extraction
            let module_name = file_to_module
                .get(&summary.file)
                .cloned()
                .unwrap_or_else(|| extract_module_name(&summary.file));

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);

                    // Calculate cognitive complexity from control flow
                    // If symbol has its own control_flow, use that
                    // Otherwise, filter summary's control_flow_changes by symbol's line range
                    let (cc, nest) = if !symbol_info.control_flow.is_empty() {
                        (
                            calculate_cognitive_complexity(&symbol_info.control_flow),
                            max_nesting_depth(&symbol_info.control_flow),
                        )
                    } else {
                        // Filter file-level control flow by symbol's line range
                        let symbol_cf: Vec<_> = summary
                            .control_flow_changes
                            .iter()
                            .filter(|cf| {
                                cf.location.line >= symbol_info.start_line
                                    && cf.location.line <= symbol_info.end_line
                            })
                            .cloned()
                            .collect();
                        (
                            calculate_cognitive_complexity(&symbol_cf),
                            max_nesting_depth(&symbol_cf),
                        )
                    };

                    let entry = SymbolIndexEntry {
                        symbol: symbol_info.name.clone(),
                        hash: symbol_id.hash.clone(),
                        semantic_hash: symbol_id.semantic_hash.clone(),
                        kind: format!("{:?}", symbol_info.kind).to_lowercase(),
                        module: module_name.clone(),
                        file: summary.file.clone(),
                        lines: format!("{}-{}", symbol_info.start_line, symbol_info.end_line),
                        risk: format!("{:?}", symbol_info.behavioral_risk).to_lowercase(),
                        cognitive_complexity: cc,
                        max_nesting: nest,
                        is_escape_local: symbol_info.is_escape_local,
                        framework_entry_point: symbol_info.framework_entry_point,
                    };

                    // Write as JSONL (one JSON object per line)
                    let json = serde_json::to_string(&entry).map_err(|e| {
                        crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize symbol index entry: {}", e),
                        }
                    })?;
                    writeln!(file, "{}", json)?;

                    stats.index_entries += 1;
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format - use summary's control flow
                let cc = calculate_cognitive_complexity(&summary.control_flow_changes);
                let nest = max_nesting_depth(&summary.control_flow_changes);

                let entry = SymbolIndexEntry {
                    symbol: summary.symbol.clone().unwrap_or_default(),
                    hash: symbol_id.hash.clone(),
                    semantic_hash: symbol_id.semantic_hash.clone(),
                    kind: summary
                        .symbol_kind
                        .map(|k| format!("{:?}", k).to_lowercase())
                        .unwrap_or_else(|| "unknown".to_string()),
                    module: module_name,
                    file: summary.file.clone(),
                    lines: match (summary.start_line, summary.end_line) {
                        (Some(s), Some(e)) => format!("{}-{}", s, e),
                        (Some(s), None) => format!("{}", s),
                        _ => String::new(),
                    },
                    risk: format!("{:?}", summary.behavioral_risk).to_lowercase(),
                    cognitive_complexity: cc,
                    max_nesting: nest,
                    is_escape_local: false,
                    framework_entry_point: summary.framework_entry_point,
                };

                // Write as JSONL (one JSON object per line)
                let json = serde_json::to_string(&entry).map_err(|e| {
                    crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to serialize symbol index entry: {}", e),
                    }
                })?;
                writeln!(file, "{}", json)?;

                stats.index_entries += 1;
            }
        }

        stats.index_bytes = fs::metadata(&path)?.len() as usize;
        stats.files_written += 1;
        Ok(())
    }

    /// Write the function signature index for duplicate detection
    ///
    /// Generates FunctionSignature entries for each symbol to enable
    /// fast duplicate detection via two-phase matching.
    fn write_signature_index(&self, stats: &mut ShardStats) -> Result<()> {
        let path = self.cache.signature_index_path();
        let mut file = fs::File::create(&path)?;

        // Build file-to-module mapping for proper module names from registry
        let file_to_module = self.build_file_to_module_map();

        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);
            // Get the optimal module name from registry, fallback to extraction
            let module_name = file_to_module
                .get(&summary.file)
                .cloned()
                .unwrap_or_else(|| extract_module_name(&summary.file));

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    // Skip non-function symbols (classes, interfaces, etc. don't get signatures)
                    if !matches!(
                        symbol_info.kind,
                        SymbolKind::Function | SymbolKind::Method | SymbolKind::Component
                    ) {
                        continue;
                    }

                    let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);
                    let signature = FunctionSignature::from_symbol_info(
                        symbol_info,
                        &symbol_id.hash,
                        &summary.file,
                        &module_name,
                        None, // Use default boilerplate config
                    );

                    // Write as JSONL (one JSON object per line)
                    let json = serde_json::to_string(&signature).map_err(|e| {
                        crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize signature: {}", e),
                        }
                    })?;
                    writeln!(file, "{}", json)?;

                    stats.signature_entries += 1;
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format
                // Create a minimal SymbolInfo from the summary
                if let Some(ref name) = summary.symbol {
                    let symbol_info = SymbolInfo {
                        name: name.clone(),
                        kind: summary.symbol_kind.unwrap_or_default(),
                        start_line: summary.start_line.unwrap_or(1),
                        end_line: summary.end_line.unwrap_or(1),
                        is_exported: true,
                        is_default_export: false,
                        hash: Some(symbol_id.hash.clone()),
                        arguments: summary.arguments.clone(),
                        props: summary.props.clone(),
                        return_type: summary.return_type.clone(),
                        calls: summary.calls.clone(),
                        control_flow: summary.control_flow_changes.clone(),
                        state_changes: summary.state_changes.clone(),
                        behavioral_risk: summary.behavioral_risk,
                        decorators: Vec::new(),
                        is_escape_local: false,
                        framework_entry_point: summary.framework_entry_point,
                    };

                    let signature = FunctionSignature::from_symbol_info(
                        &symbol_info,
                        &symbol_id.hash,
                        &summary.file,
                        &module_name,
                        None,
                    );

                    let json = serde_json::to_string(&signature).map_err(|e| {
                        crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize signature: {}", e),
                        }
                    })?;
                    writeln!(file, "{}", json)?;

                    stats.signature_entries += 1;
                }
            }
        }

        stats.signature_bytes = fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
        stats.files_written += 1;
        Ok(())
    }

    /// Write the BM25 semantic search index
    ///
    /// Generates a BM25 index from all symbols for loose term queries
    /// like "authentication", "error handling", or "database connection".
    fn write_bm25_index(
        &self,
        stats: &mut ShardStats,
        progress: &Option<ShardProgressCallback>,
    ) -> Result<()> {
        let entries = AtomicUsize::new(0);
        let total_length = AtomicUsize::new(0);
        let total_docs: usize = self
            .all_summaries
            .iter()
            .map(|summary| {
                if !summary.symbols.is_empty() {
                    summary.symbols.len()
                } else if summary.symbol_id.is_some() {
                    1
                } else {
                    0
                }
            })
            .sum();

        // Build file-to-module mapping for proper module names from registry
        let file_to_module = self.build_file_to_module_map();

        emit_progress(progress, "BM25 index", 0, total_docs.max(1));

        let docs_by_summary: Vec<Vec<(Bm25Document, Vec<String>)>> = self
            .all_summaries
            .par_iter()
            .map(|summary| {
                let namespace = SymbolId::namespace_from_path(&summary.file);
                // Get the optimal module name from registry, fallback to extraction
                let module_name = file_to_module
                    .get(&summary.file)
                    .cloned()
                    .unwrap_or_else(|| extract_module_name(&summary.file));

                let file_terms = extract_terms_from_file_path(&summary.file);
                let module_terms = crate::bm25::tokenize(&module_name);

                let mut docs: Vec<(Bm25Document, Vec<String>)> = Vec::new();

                // If we have symbols in the new multi-symbol format, use those
                if !summary.symbols.is_empty() {
                    for symbol_info in &summary.symbols {
                        let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);
                        let kind_str = format!("{:?}", symbol_info.kind).to_lowercase();

                        // Extract searchable terms from this symbol
                        let mut terms = Vec::new();
                        terms.extend(crate::bm25::tokenize(&symbol_info.name));
                        terms.extend(file_terms.iter().cloned());
                        terms.push(kind_str.clone());
                        terms.extend(module_terms.iter().cloned());
                        let mut seen = std::collections::HashSet::new();
                        terms.retain(|t| seen.insert(t.clone()));

                        let doc = Bm25Document {
                            hash: symbol_id.hash,
                            symbol: symbol_info.name.clone(),
                            file: summary.file.clone(),
                            lines: format!("{}-{}", symbol_info.start_line, symbol_info.end_line),
                            kind: kind_str,
                            module: module_name.clone(),
                            risk: format!("{:?}", symbol_info.behavioral_risk).to_lowercase(),
                            doc_length: 0, // Will be set by add_document
                        };

                        docs.push((doc, terms));
                    }
                } else if let Some(ref symbol_id) = summary.symbol_id {
                    // Fallback to old single-symbol format
                    let kind_str = summary
                        .symbol_kind
                        .map(|k| format!("{:?}", k).to_lowercase())
                        .unwrap_or_else(|| "unknown".to_string());

                    let mut terms = Vec::new();
                    terms.extend(crate::bm25::tokenize(
                        summary.symbol.as_deref().unwrap_or(""),
                    ));
                    terms.extend(file_terms.iter().cloned());
                    terms.push(kind_str.clone());
                    terms.extend(module_terms.iter().cloned());
                    let mut seen = std::collections::HashSet::new();
                    terms.retain(|t| seen.insert(t.clone()));

                    let doc = Bm25Document {
                        hash: symbol_id.hash.clone(),
                        symbol: summary.symbol.clone().unwrap_or_default(),
                        file: summary.file.clone(),
                        lines: match (summary.start_line, summary.end_line) {
                            (Some(s), Some(e)) => format!("{}-{}", s, e),
                            (Some(s), None) => format!("{}", s),
                            _ => String::new(),
                        },
                        kind: kind_str,
                        module: module_name,
                        risk: format!("{:?}", summary.behavioral_risk).to_lowercase(),
                        doc_length: 0,
                    };

                    docs.push((doc, terms));
                }

                docs
            })
            .collect();
        let docs: Vec<(Bm25Document, Vec<String>)> =
            docs_by_summary.into_iter().flatten().collect();

        let db_path = self.cache.bm25_index_path();
        let mut conn =
            Connection::open(&db_path).map_err(|e| crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to open BM25 sqlite: {}", e),
            })?;
        crate::bm25::init_bm25_sqlite(&conn).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to init BM25 sqlite: {}", e),
            }
        })?;
        crate::bm25::clear_bm25_sqlite(&conn).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to clear BM25 sqlite: {}", e),
            }
        })?;
        let tx = conn
            .transaction()
            .map_err(|e| crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to start BM25 sqlite transaction: {}", e),
            })?;
        {
            let mut insert_doc = tx.prepare(
                "INSERT OR IGNORE INTO bm25_documents (doc_id, symbol, file, lines, kind, module, risk, doc_length) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            ).map_err(|e| {
                crate::McpDiffError::ExtractionFailure {
                    message: format!("Failed to prepare BM25 doc insert: {}", e),
                }
            })?;
            let mut insert_term = tx
                .prepare("INSERT OR IGNORE INTO bm25_terms (term, doc_id, tf) VALUES (?, ?, ?)")
                .map_err(|e| crate::McpDiffError::ExtractionFailure {
                    message: format!("Failed to prepare BM25 term insert: {}", e),
                })?;

            for (doc, terms) in docs {
                let doc_length = terms.len() as u32;
                let doc_id = doc.hash.clone();
                let inserted = insert_doc
                    .execute(rusqlite::params![
                        doc_id,
                        doc.symbol,
                        doc.file,
                        doc.lines,
                        doc.kind,
                        doc.module,
                        doc.risk,
                        doc_length as i64
                    ])
                    .map_err(|e| crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to insert BM25 doc: {}", e),
                    })?;
                if inserted > 0 {
                    for term in terms {
                        insert_term
                            .execute(rusqlite::params![term, doc_id, 1i64])
                            .map_err(|e| crate::McpDiffError::ExtractionFailure {
                                message: format!("Failed to insert BM25 term: {}", e),
                            })?;
                    }
                }
                total_length.fetch_add(doc_length as usize, Ordering::Relaxed);
                let current = entries.fetch_add(1, Ordering::Relaxed) + 1;
                if current % 1000 == 0 || current == total_docs {
                    emit_progress(progress, "BM25 index", current, total_docs.max(1));
                }
            }
        }

        emit_progress(progress, "BM25 index", total_docs, total_docs.max(1));
        stats.bm25_entries = entries.load(Ordering::Relaxed);

        let total_docs_u32 = stats.bm25_entries as u32;
        let avg_doc_length = if total_docs_u32 > 0 {
            total_length.load(Ordering::Relaxed) as f64 / total_docs_u32 as f64
        } else {
            0.0
        };

        tx.commit()
            .map_err(|e| crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to commit BM25 sqlite: {}", e),
            })?;
        crate::bm25::write_bm25_meta(&conn, total_docs_u32, avg_doc_length).map_err(|e| {
            crate::McpDiffError::ExtractionFailure {
                message: format!("Failed to write BM25 meta: {}", e),
            }
        })?;

        let path = self.cache.bm25_index_path();
        stats.bm25_bytes = fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
        stats.files_written += 1;
        Ok(())
    }

    /// Get the cache directory path
    pub fn cache_path(&self) -> &Path {
        &self.cache.root
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, usize) {
        (self.cache.size(), self.modules.len())
    }
}

/// Statistics about the sharding operation
#[derive(Debug, Default)]
pub struct ShardStats {
    /// Total files written
    pub files_written: usize,

    /// Bytes written for overview
    pub overview_bytes: usize,

    /// Number of module shards
    pub modules_written: usize,

    /// Bytes written for modules
    pub module_bytes: usize,

    /// Number of symbol shards
    pub symbols_written: usize,

    /// Bytes written for symbols
    pub symbol_bytes: usize,

    /// Bytes written for graphs
    pub graph_bytes: usize,

    /// Number of entries in symbol index
    pub index_entries: usize,

    /// Bytes written for symbol index
    pub index_bytes: usize,

    /// Number of entries in signature index (duplicate detection)
    pub signature_entries: usize,

    /// Bytes written for signature index
    pub signature_bytes: usize,

    /// Number of documents in BM25 semantic search index
    pub bm25_entries: usize,

    /// Bytes written for BM25 index
    pub bm25_bytes: usize,
}

impl ShardStats {
    /// Total bytes written
    pub fn total_bytes(&self) -> usize {
        self.overview_bytes
            + self.module_bytes
            + self.symbol_bytes
            + self.graph_bytes
            + self.index_bytes
            + self.signature_bytes
            + self.bm25_bytes
    }
}

// ============================================================================
// Encoding Functions
// ============================================================================

/// Encode repository overview with metadata
fn encode_repo_overview_with_meta(overview: &RepoOverview, progress: &IndexingStatus) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("repo_overview"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));

    if let Some(ref framework) = overview.framework {
        lines.push(format!("framework: \"{}\"", framework));
    }

    if let Some(ref database) = overview.database {
        lines.push(format!("database: \"{}\"", database));
    }

    // Patterns
    if !overview.patterns.is_empty() {
        let patterns: Vec<String> = overview
            .patterns
            .iter()
            .map(|p| format!("\"{}\"", p))
            .collect();
        lines.push(format!(
            "patterns[{}]: {}",
            overview.patterns.len(),
            patterns.join(",")
        ));
    }

    // Modules summary
    if !overview.modules.is_empty() {
        lines.push(format!(
            "modules[{}]{{name,purpose,files,risk}}:",
            overview.modules.len()
        ));
        for m in &overview.modules {
            lines.push(format!(
                "  {},\"{}\",{},{}",
                m.name,
                m.purpose,
                m.file_count,
                m.risk.as_str()
            ));
        }
    }

    // Stats
    lines.push(format!("files: {}", overview.stats.total_files));
    lines.push(format!(
        "risk_breakdown: \"high:{},medium:{},low:{}\"",
        overview.stats.high_risk, overview.stats.medium_risk, overview.stats.low_risk
    ));

    // Entry points
    if !overview.entry_points.is_empty() {
        let entries: Vec<String> = overview
            .entry_points
            .iter()
            .map(|e| e.to_string())
            .collect();
        lines.push(format!(
            "entry_points[{}]: {}",
            entries.len(),
            entries.join(",")
        ));
    }

    // Indexing status (if in progress)
    if progress.in_progress {
        lines.push(format!("indexing_status:"));
        lines.push(format!("  in_progress: true"));
        lines.push(format!("  files_indexed: {}", progress.files_indexed));
        lines.push(format!("  files_total: {}", progress.files_total));
        lines.push(format!("  percent: {}", progress.percent));
        if let Some(eta) = progress.eta_seconds {
            lines.push(format!("  eta_seconds: {}", eta));
        }
    }

    lines.join("\n")
}

/// Encode a module shard with all its files
///
/// Now lists ALL symbols from each file's summary.symbols, not just the primary one.
pub(crate) fn encode_module_shard(
    module_name: &str,
    summaries: &[SemanticSummary],
    repo_root: &Path,
) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("module_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("module: \"{}\"", module_name));
    lines.push(format!("file_count: {}", summaries.len()));

    // Collect ALL symbols from all files in this module
    let mut all_symbols: Vec<(String, String, SymbolKind, String, RiskLevel)> = Vec::new();
    let mut high = 0;
    let mut medium = 0;
    let mut low = 0;

    for summary in summaries {
        let namespace = SymbolId::namespace_from_path(&summary.file);

        // If we have multi-symbol format, use those
        if !summary.symbols.is_empty() {
            for symbol_info in &summary.symbols {
                let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);
                let lines_str = format!("{}-{}", symbol_info.start_line, symbol_info.end_line);
                all_symbols.push((
                    symbol_id.hash,
                    symbol_info.name.clone(),
                    symbol_info.kind,
                    lines_str,
                    symbol_info.behavioral_risk,
                ));

                match symbol_info.behavioral_risk {
                    RiskLevel::High => high += 1,
                    RiskLevel::Medium => medium += 1,
                    RiskLevel::Low => low += 1,
                }
            }
        } else if let Some(ref symbol_id) = summary.symbol_id {
            // Fallback to old format
            let lines_str = match (summary.start_line, summary.end_line) {
                (Some(s), Some(e)) => format!("{}-{}", s, e),
                _ => String::new(),
            };
            all_symbols.push((
                symbol_id.hash.clone(),
                summary.symbol.clone().unwrap_or_default(),
                summary.symbol_kind.unwrap_or_default(),
                lines_str,
                summary.behavioral_risk,
            ));

            match summary.behavioral_risk {
                RiskLevel::High => high += 1,
                RiskLevel::Medium => medium += 1,
                RiskLevel::Low => low += 1,
            }
        }
    }

    lines.push(format!(
        "risk_breakdown: \"high:{},medium:{},low:{}\"",
        high, medium, low
    ));

    // List all symbols with expanded info
    if !all_symbols.is_empty() {
        lines.push(format!(
            "symbols[{}]{{hash,name,kind,lines,risk}}:",
            all_symbols.len()
        ));
        for (hash, name, kind, lines_str, risk) in &all_symbols {
            lines.push(format!(
                "  {},\"{}\",{},{},{}",
                hash,
                name,
                kind.as_str(),
                lines_str,
                risk.as_str()
            ));
        }
    }

    // Add source file info for staleness detection
    lines.push(format!("_meta:"));
    lines.push(format!(
        "  generated_at: \"{}\"",
        chrono::Utc::now().to_rfc3339()
    ));
    lines.push(format!("  source_files[{}]:", summaries.len()));
    for s in summaries {
        if let Some(info) = SourceFileInfo::from_path(Path::new(&s.file), repo_root) {
            lines.push(format!("    path: \"{}\"", info.path));
            lines.push(format!("    mtime: {}", info.mtime));
        }
    }

    lines.join("\n")
}

/// Encode a single symbol shard (legacy format)
pub(crate) fn encode_symbol_shard(summary: &SemanticSummary) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("symbol_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));

    // Full symbol encoding
    lines.push(encode_toon(summary));

    lines.join("\n")
}

/// Encode a symbol shard from SymbolInfo (new multi-symbol format)
///
/// This creates a complete symbol shard from a SymbolInfo struct,
/// combining file-level metadata from the summary with symbol-specific data.
pub(crate) fn encode_symbol_shard_from_info(
    summary: &SemanticSummary,
    symbol_info: &SymbolInfo,
    symbol_id: &SymbolId,
) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("symbol_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("file: \"{}\"", summary.file));
    lines.push(format!("language: {}", summary.language));
    lines.push(format!("symbol_id: {}", symbol_id.hash));
    lines.push(format!("symbol_namespace: \"{}\"", symbol_id.namespace));
    lines.push(format!("symbol: {}", symbol_info.name));
    lines.push(format!("symbol_kind: {}", symbol_info.kind.as_str()));
    lines.push(format!(
        "lines: \"{}-{}\"",
        symbol_info.start_line, symbol_info.end_line
    ));

    if symbol_info.is_exported {
        lines.push(format!("public_surface_changed: true"));
    }

    lines.push(format!(
        "behavioral_risk: {}",
        symbol_info.behavioral_risk.as_str()
    ));

    // Arguments
    if !symbol_info.arguments.is_empty() {
        let args: Vec<String> = symbol_info
            .arguments
            .iter()
            .map(|a| {
                if let Some(ref t) = a.arg_type {
                    format!("{}:{}", a.name, t)
                } else {
                    a.name.clone()
                }
            })
            .collect();
        lines.push(format!("arguments[{}]: {}", args.len(), args.join(",")));
    }

    // Props
    if !symbol_info.props.is_empty() {
        let props: Vec<String> = symbol_info
            .props
            .iter()
            .map(|p| {
                if let Some(ref t) = p.prop_type {
                    format!("{}:{}", p.name, t)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        lines.push(format!("props[{}]: {}", props.len(), props.join(",")));
    }

    // Return type
    if let Some(ref ret) = symbol_info.return_type {
        lines.push(format!("return_type: \"{}\"", ret));
    }

    // Control flow
    if !symbol_info.control_flow.is_empty() {
        let cf: Vec<String> = symbol_info
            .control_flow
            .iter()
            .map(|c| c.kind.as_str().to_string())
            .collect();
        lines.push(format!("control_flow[{}]: {}", cf.len(), cf.join(",")));
    }

    // Calls - use symbol-level calls (symbol_info.calls) which are correctly attributed
    // during extraction via find_containing_symbol_by_line
    if !symbol_info.calls.is_empty() {
        let meaningful_calls: Vec<_> = symbol_info
            .calls
            .iter()
            .filter(|c| is_meaningful_call(&c.name, c.object.as_deref()))
            .collect();

        if !meaningful_calls.is_empty() {
            lines.push(format!(
                "calls[{}]{{name,obj,await,try,count}}:",
                meaningful_calls.len()
            ));

            // Deduplicate calls
            let mut call_counts: HashMap<String, (Option<String>, bool, bool, usize)> =
                HashMap::new();
            for call in &meaningful_calls {
                let key = format!("{}:{:?}", call.name, call.object);
                call_counts.entry(key).and_modify(|e| e.3 += 1).or_insert((
                    call.object.clone(),
                    call.is_awaited,
                    call.in_try,
                    1,
                ));
            }

            for (key, (obj, is_awaited, in_try, count)) in call_counts {
                let name = key.split(':').next().unwrap_or(&key);
                let obj_str = obj.as_deref().unwrap_or("_");
                let await_str = if is_awaited { "Y" } else { "_" };
                let try_str = if in_try { "Y" } else { "_" };
                let count_str = if count > 1 {
                    format!("\"{}\"", count)
                } else {
                    "_".to_string()
                };
                lines.push(format!(
                    "  {},{},{},{},{}",
                    name, obj_str, await_str, try_str, count_str
                ));
            }
        }
    }

    // Include file-level dependencies as context
    if !summary.added_dependencies.is_empty() {
        let deps: Vec<String> = summary
            .added_dependencies
            .iter()
            .take(20) // Limit to avoid huge output
            .cloned()
            .collect();
        lines.push(format!(
            "added_dependencies[{}]: {}",
            deps.len(),
            deps.join(",")
        ));
    }

    lines.join("\n")
}

/// Build a lookup map from symbol name to their SymbolIds
/// Returns: name -> Vec<(hash, namespace)> for disambiguation
fn build_symbol_lookup(summaries: &[SemanticSummary]) -> HashMap<String, Vec<(String, String)>> {
    let mut lookup: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for summary in summaries {
        if let Some(ref symbol_id) = summary.symbol_id {
            lookup
                .entry(symbol_id.symbol.clone())
                .or_default()
                .push((symbol_id.hash.clone(), symbol_id.namespace.clone()));
        }

        // Also index symbols from the symbols array
        for symbol in &summary.symbols {
            let hash = crate::overlay::compute_symbol_hash(symbol, &summary.file);
            let namespace = SymbolId::namespace_from_path(&summary.file);
            lookup
                .entry(symbol.name.clone())
                .or_default()
                .push((hash, namespace));
        }
    }

    // Deduplicate entries (same hash can appear multiple times)
    for entries in lookup.values_mut() {
        entries.sort();
        entries.dedup();
    }

    lookup
}

/// Resolve a call name to a symbol hash if possible
/// Returns the hash if uniquely resolved, or the original name if ambiguous/external
/// When multiple symbols have the same name, prefers same-file matches (local scope)
/// For external calls, includes package name if available from import_sources
fn resolve_call_to_hash(
    call_name: &str,
    lookup: &HashMap<String, Vec<(String, String)>>,
    same_file_prefix: &str,
    import_sources: &HashMap<String, String>,
) -> String {
    // Helper to find best match from a list, preferring same-file matches
    let find_best_match = |matches: &[(String, String)]| -> Option<String> {
        if matches.is_empty() {
            return None;
        }
        if matches.len() == 1 {
            return Some(matches[0].0.clone());
        }
        for (hash, _namespace) in matches {
            if hash.starts_with(&same_file_prefix) {
                return Some(hash.clone());
            }
        }
        // No same-file match, fall back to first
        Some(matches[0].0.clone())
    };

    // Try exact match first (e.g., "GetGoldAmount" or "PlayerEntity.GetGoldAmount")
    if let Some(matches) = lookup.get(call_name) {
        if let Some(resolved) = find_best_match(matches) {
            return resolved;
        }
    }

    // If call has an object (e.g., "playerEntity.GetGoldAmount"), try just the method name
    // This handles cases where we can't infer the type of the object variable
    if let Some(dot_pos) = call_name.rfind('.') {
        let method_name = &call_name[dot_pos + 1..];
        if !method_name.is_empty() {
            if let Some(matches) = lookup.get(method_name) {
                if let Some(resolved) = find_best_match(matches) {
                    return resolved;
                }
            }
        }
    }

    // No match - external call
    // Prefix with "ext:" to distinguish from hashes
    // Include package name if available: ext:package:symbol or ext:symbol

    // Try to find package from import_sources
    // For call like "Card.Grid", first try "Grid" (method), then "Card" (object)
    // For call like "useState", try "useState" directly
    let package = if let Some(dot_pos) = call_name.rfind('.') {
        let method_name = &call_name[dot_pos + 1..];
        let object_name = &call_name[..dot_pos];
        // For nested access like "Card.Grid", extract the root object
        let root_object = object_name.split('.').next().unwrap_or(object_name);

        // Try method first (e.g., Grid), then object (e.g., Card)
        import_sources
            .get(method_name)
            .or_else(|| import_sources.get(root_object))
    } else {
        import_sources.get(call_name)
    };

    if let Some(pkg) = package {
        format!("ext:{}:{}", pkg, call_name)
    } else {
        format!("ext:{}", call_name)
    }
}

/// Build call graph from summaries with resolved symbol hashes
/// Parallelized with Rayon for better performance on large codebases
/// Returns edges with edge_kind to distinguish calls from variable reads/writes
fn build_call_graph(
    summaries: &[SemanticSummary],
    progress: &Option<ShardProgressCallback>,
) -> HashMap<String, Vec<CallGraphEdge>> {
    use crate::overlay::compute_symbol_hash;

    let total = summaries.len();

    // Build lookup for resolving call names to hashes (must be done before parallel phase)
    let symbol_lookup = build_symbol_lookup(summaries);

    // Progress tracking
    let processed = AtomicUsize::new(0);

    let graph_fast: ahash::AHashMap<String, Vec<CallGraphEdge>> = summaries
        .par_iter()
        .with_min_len(32)
        .fold(
            ahash::AHashMap::new,
            |mut local_graph: ahash::AHashMap<String, Vec<CallGraphEdge>>, summary| {
                let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
                if total > 100 && (current % 500 == 0 || current == total) {
                    emit_progress(progress, "Call graph", current, total);
                }

                // Compute file hash once for this file (used to prefer same-file call resolution)
                let caller_file_hash = crate::overlay::extract_file_hash(
                    summary
                        .symbol_id
                        .as_ref()
                        .map(|s| s.hash.as_str())
                        .unwrap_or(""),
                )
                .to_string();
                // Fallback: compute from file path if no symbol_id
                let caller_file_hash = if caller_file_hash.is_empty() {
                    format!("{:08x}", crate::schema::fnv1a_hash(&summary.file) as u32)
                } else {
                    caller_file_hash
                };
                let same_file_prefix = if caller_file_hash.is_empty() {
                    String::new()
                } else {
                    format!("{}:", caller_file_hash)
                };

                // Process each symbol in the file
                for symbol in &summary.symbols {
                    let hash = compute_symbol_hash(symbol, &summary.file);
                    let mut edges: Vec<CallGraphEdge> = Vec::new();
                    let mut seen: ahash::AHashSet<(String, RefKind)> = ahash::AHashSet::new();

                    // Extract from the symbol's own calls array
                    for c in &symbol.calls {
                        let resolved = if let Some(ref obj) = c.object {
                            let mut call_name = String::with_capacity(obj.len() + 1 + c.name.len());
                            call_name.push_str(obj);
                            call_name.push('.');
                            call_name.push_str(&c.name);
                            resolve_call_to_hash(
                                &call_name,
                                &symbol_lookup,
                                &same_file_prefix,
                                &summary.import_sources,
                            )
                        } else {
                            resolve_call_to_hash(
                                &c.name,
                                &symbol_lookup,
                                &same_file_prefix,
                                &summary.import_sources,
                            )
                        };
                        let edge = CallGraphEdge::new(resolved, c.ref_kind);

                        // Deduplicate by callee+edge_kind
                        if seen.insert((edge.callee.clone(), edge.edge_kind)) {
                            edges.push(edge);
                        }
                    }

                    // Extract function calls from symbol's state_changes initializers (always call kind)
                    for state in &symbol.state_changes {
                        if !state.initializer.is_empty() {
                            if let Some(call_name) =
                                extract_call_from_initializer(&state.initializer)
                            {
                                let resolved = resolve_call_to_hash(
                                    &call_name,
                                    &symbol_lookup,
                                    &same_file_prefix,
                                    &summary.import_sources,
                                );
                                let edge = CallGraphEdge::call(resolved);
                                if seen.insert((edge.callee.clone(), edge.edge_kind)) {
                                    edges.push(edge);
                                }
                            }
                        }
                    }

                    if !edges.is_empty() {
                        local_graph.entry(hash).or_default().extend(edges);
                    }
                }

                // Also process file-level calls
                if let Some(ref symbol_id) = summary.symbol_id {
                    let mut edges: Vec<CallGraphEdge> = Vec::new();
                    let mut seen: ahash::AHashSet<(String, RefKind)> = ahash::AHashSet::new();

                    for c in &summary.calls {
                        let resolved = if let Some(ref obj) = c.object {
                            let mut call_name = String::with_capacity(obj.len() + 1 + c.name.len());
                            call_name.push_str(obj);
                            call_name.push('.');
                            call_name.push_str(&c.name);
                            resolve_call_to_hash(
                                &call_name,
                                &symbol_lookup,
                                &same_file_prefix,
                                &summary.import_sources,
                            )
                        } else {
                            resolve_call_to_hash(
                                &c.name,
                                &symbol_lookup,
                                &same_file_prefix,
                                &summary.import_sources,
                            )
                        };
                        let edge = CallGraphEdge::new(resolved, c.ref_kind);
                        if seen.insert((edge.callee.clone(), edge.edge_kind)) {
                            edges.push(edge);
                        }
                    }

                    for state in &summary.state_changes {
                        if !state.initializer.is_empty() {
                            if let Some(call_name) =
                                extract_call_from_initializer(&state.initializer)
                            {
                                let resolved = resolve_call_to_hash(
                                    &call_name,
                                    &symbol_lookup,
                                    &same_file_prefix,
                                    &summary.import_sources,
                                );
                                let edge = CallGraphEdge::call(resolved);
                                if seen.insert((edge.callee.clone(), edge.edge_kind)) {
                                    edges.push(edge);
                                }
                            }
                        }
                    }

                    // Include both functions (lowercase) and components (PascalCase) from dependencies
                    // Only exclude Rust-style namespace paths (::) - these are always call kind
                    for dep in &summary.added_dependencies {
                        if !dep.contains("::") {
                            let resolved = resolve_call_to_hash(
                                dep,
                                &symbol_lookup,
                                &same_file_prefix,
                                &summary.import_sources,
                            );
                            let edge = CallGraphEdge::call(resolved);
                            if seen.insert((edge.callee.clone(), edge.edge_kind)) {
                                edges.push(edge);
                            }
                        }
                    }

                    if !edges.is_empty() {
                        local_graph
                            .entry(symbol_id.hash.clone())
                            .or_default()
                            .extend(edges);
                    }
                }

                local_graph
            },
        )
        .reduce(ahash::AHashMap::new, |mut left, right| {
            for (hash, edges) in right {
                left.entry(hash).or_default().extend(edges);
            }
            left
        });

    if total > 0 {
        emit_progress(progress, "Call graph", total, total);
    }

    graph_fast.into_iter().collect()
}

/// Extract a function call name from an initializer expression
fn extract_call_from_initializer(init: &str) -> Option<String> {
    let trimmed = init.trim();

    // Skip simple literals and keywords
    if trimmed.is_empty()
        || trimmed.starts_with('"')
        || trimmed.starts_with('\'')
        || trimmed.parse::<i64>().is_ok()
        || trimmed.parse::<f64>().is_ok()
        || trimmed == "true"
        || trimmed == "false"
        || trimmed == "None"
        || trimmed == "null"
        || trimmed == "undefined"
    {
        return None;
    }

    // Look for function call pattern: something(...)
    // Use split_once for guaranteed UTF-8 safety
    if let Some((before_paren, _)) = trimmed.split_once('(') {
        // Handle method chains: take the last part
        // e.g., "self.cache.repo_overview_path()" -> "repo_overview_path"
        let call_part = before_paren
            .rsplit(&['.', ':'][..])
            .next()
            .unwrap_or(before_paren)
            .trim();

        // Skip if it's a type constructor (starts with uppercase)
        if call_part
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            // But allow things like Vec::new, HashMap::new
            if call_part == "new" || call_part == "default" {
                // Try to get the type name
                if let Some(type_part) = before_paren.rsplit("::").nth(1) {
                    return Some(format!("{}::{}", type_part.trim(), call_part));
                }
            }
            return None;
        }

        // Skip very short names that are likely not meaningful
        if call_part.len() < 2 {
            return None;
        }

        // Skip common noise
        let noise = [
            "iter",
            "map",
            "filter",
            "collect",
            "clone",
            "to_string",
            "len",
            "is_empty",
            "unwrap",
            "unwrap_or",
            "ok",
            "err",
            "as_ref",
            "as_str",
            "into",
            "from",
            "push",
            "pop",
            "get",
            "insert",
            "remove",
        ];
        if noise.contains(&call_part) {
            return None;
        }

        return Some(call_part.to_string());
    }

    None
}

/// Encode call graph with edge_kind information
/// Format: caller_hash: ["callee1", "callee2:read", "callee3:write"]
#[allow(dead_code)]
fn encode_call_graph(graph: &HashMap<String, Vec<CallGraphEdge>>) -> String {
    let mut lines = Vec::new();

    lines.push("_type: call_graph".to_string());
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("edges: {}", graph.len()));

    for (symbol_hash, edges) in graph {
        let edges_str = edges
            .iter()
            .map(|e| e.encode())
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!("{}: [{}]", symbol_hash, edges_str));
    }

    lines.join("\n")
}

fn write_call_graph(path: &Path, graph: &HashMap<String, Vec<CallGraphEdge>>) -> Result<usize> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);
    let mut bytes = 0usize;

    let header = format!(
        "_type: call_graph\nschema_version: \"{}\"\nedges: {}\n",
        SCHEMA_VERSION,
        graph.len()
    );
    writer.write_all(header.as_bytes())?;
    bytes += header.len();

    for (symbol_hash, edges) in graph {
        let mut line = String::new();
        line.push_str(symbol_hash);
        line.push_str(": [");
        for (idx, edge) in edges.iter().enumerate() {
            if idx > 0 {
                line.push(',');
            }
            line.push_str(&edge.encode());
        }
        line.push_str("]\n");
        writer.write_all(line.as_bytes())?;
        bytes += line.len();
    }

    writer.flush()?;
    Ok(bytes)
}

/// Build import graph from summaries
fn build_import_graph(summaries: &[SemanticSummary]) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for summary in summaries {
        if !summary.local_imports.is_empty() {
            graph.insert(summary.file.clone(), summary.local_imports.clone());
        }
    }

    graph
}

/// Encode import graph
fn encode_import_graph(graph: &HashMap<String, Vec<String>>) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("import_graph"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("files: {}", graph.len()));

    for (file, imports) in graph {
        let imports_str = imports
            .iter()
            .map(|i| format!("\"{}\"", i))
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!("\"{}\": [{}]", file, imports_str));
    }

    lines.join("\n")
}

/// Build module dependency graph
fn build_module_graph(
    modules: &HashMap<String, Vec<SemanticSummary>>,
    file_to_module: &HashMap<String, String>,
) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for (module_name, summaries) in modules {
        let mut deps: Vec<String> = Vec::new();

        for summary in summaries {
            for import in &summary.local_imports {
                // Get the optimal module name from registry, fallback to extraction
                let import_module = file_to_module
                    .get(import)
                    .cloned()
                    .unwrap_or_else(|| extract_module_name(import));
                if import_module != *module_name && !deps.contains(&import_module) {
                    deps.push(import_module);
                }
            }
        }

        if !deps.is_empty() {
            graph.insert(module_name.clone(), deps);
        }
    }

    graph
}

/// Encode module graph
fn encode_module_graph(graph: &HashMap<String, Vec<String>>) -> String {
    let mut lines = Vec::new();

    lines.push(toon_header("module_graph"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("modules: {}", graph.len()));

    for (module, deps) in graph {
        let deps_str = deps
            .iter()
            .map(|d| format!("\"{}\"", d))
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!("\"{}\": [{}]", module, deps_str));
    }

    lines.join("\n")
}

/// Extract module/namespace from file path.
///
/// Returns the path-based namespace (directory structure after src/).
/// For languages with real namespaces (Rust, Python, Java, Go), the extractor
/// should override this with the actual language namespace.
pub fn extract_module_name(file_path: &str) -> String {
    // Extract the portion of the path after /src/ (or similar source roots)
    // Order matters: more specific markers first (Assets/Scripts before Assets)
    let source_markers = [
        // Standard web/backend
        "/src/",
        "/lib/",
        "/app/",
        "/pages/",
        // Game engines (specific first)
        "/Assets/Scripts/", // Unity C# scripts
        "/Assets/",         // Unity (fallback for other assets)
        "/Source/",         // Unreal C++
        "/Content/",        // Unreal Blueprints
        "/scripts/",        // Godot GDScript
        "/addons/",         // Godot addons
        // Monorepos
        "/packages/",
        "/modules/",
    ];
    let mut relative_path = file_path;

    // Case-insensitive matching for cross-platform compatibility
    // (e.g., Unity uses /Packages/ while we list /packages/)
    let file_path_lower = file_path.to_lowercase();
    for marker in &source_markers {
        let marker_lower = marker.to_lowercase();
        if let Some(pos) = file_path_lower.find(&marker_lower) {
            relative_path = &file_path[pos + marker.len()..];
            break;
        }
    }

    // Also handle relative paths starting with src/ (matching absolute markers)
    // Case-insensitive for cross-platform compatibility
    if relative_path == file_path {
        let prefixes = [
            "src/",
            "lib/",
            "app/",
            "pages/",
            "assets/scripts/",
            "assets/",
            "source/",
            "content/",
            "scripts/",
            "addons/",
            "packages/",
            "modules/",
        ];
        for prefix in &prefixes {
            if file_path_lower.starts_with(prefix) {
                relative_path = &file_path[prefix.len()..];
                break;
            }
        }
    }

    // If still no marker found and path looks absolute, try to find project root
    // by detecting common project subdirectories (tests/, docs/, etc.)
    if relative_path == file_path && file_path.starts_with('/') {
        relative_path = detect_project_relative_path(file_path);
    }

    // Get the directory path (everything before the filename)
    let path = std::path::Path::new(relative_path);
    if let Some(parent) = path.parent() {
        let parent_str = parent.to_string_lossy();
        if !parent_str.is_empty() && parent_str != "." {
            // Convert path separators to dots for namespace
            return parent_str.replace('/', ".").replace('\\', ".");
        }
    }

    // File is directly in src/ - use filename without extension
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("root");

    // Skip generic names
    if matches!(stem, "index" | "mod" | "lib" | "main" | "__init__") {
        return "root".to_string();
    }

    stem.to_string()
}

/// Detect project root from absolute path and return relative path.
///
/// Looks for common project subdirectories (tests/, docs/, etc.) and
/// assumes the directory before them is the project root.
fn detect_project_relative_path(file_path: &str) -> &str {
    // Common project-level directories that indicate we're at project root
    let project_subdirs = [
        "/tests/",
        "/test/",
        "/docs/",
        "/doc/",
        "/scripts/",
        "/examples/",
        "/benchmarks/",
        "/benches/",
        "/tickets/",
        "/migrations/",
        "/fixtures/",
        "/specs/",
        "/__tests__/",
        "/__mocks__/",
    ];

    // Find the first project subdir in the path
    for subdir in &project_subdirs {
        if let Some(pos) = file_path.find(subdir) {
            // Return from the subdir onwards (without leading /)
            return &file_path[pos + 1..];
        }
    }

    // No project subdir found - try to detect Python package structure
    // Look for a directory that could be a Python package (lowercase, underscores)
    // followed by a Python file
    if file_path.ends_with(".py") {
        // Split into components and look for package-like directories
        let components: Vec<&str> = file_path.split('/').collect();

        // Look for patterns like /project-name/package_name/file.py
        // where package_name uses underscores (Python convention)
        for (i, component) in components.iter().enumerate() {
            // Skip empty components and root
            if component.is_empty() || i < 2 {
                continue;
            }

            // Python packages typically use underscores, not hyphens
            // If we find a directory with underscores followed by .py files,
            // that's likely our package root
            if component.contains('_') && !component.contains('-') {
                // Check if this could be a Python package name
                // Return from this component onwards
                let start_pos: usize = components[..i].iter().map(|c| c.len() + 1).sum();
                return &file_path[start_pos..];
            }
        }
    }

    // Fallback: if path has many components, try to find a reasonable cut point
    // Look for directories that look like project names (hyphenated or underscored)
    let components: Vec<&str> = file_path.split('/').collect();
    if components.len() > 4 {
        // Skip typical prefix directories (home, user, Dev, etc.)
        // Look for a directory that looks like a project name
        for (i, component) in components.iter().enumerate().skip(3) {
            if component.contains('-') || component.contains('_') {
                // This looks like a project directory, return everything after it
                let start_pos: usize = components[..=i].iter().map(|c| c.len() + 1).sum();
                if start_pos < file_path.len() {
                    return &file_path[start_pos..];
                }
            }
        }
    }

    // Give up - return original path
    file_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module_name() {
        // Files in subdirectories get dotted namespace from directory path
        assert_eq!(extract_module_name("src/api/users.ts"), "api");
        assert_eq!(
            extract_module_name("src/components/Button.tsx"),
            "components"
        );
        assert_eq!(extract_module_name("src/utils/format.ts"), "utils");
        assert_eq!(
            extract_module_name("src/features/auth/login.ts"),
            "features.auth"
        );

        // Files directly in src/ use filename as namespace
        assert_eq!(extract_module_name("src/cache.rs"), "cache");
        assert_eq!(extract_module_name("src/schema.rs"), "schema");

        // Generic filenames fallback to "root"
        assert_eq!(extract_module_name("src/index.ts"), "root");
        assert_eq!(extract_module_name("src/main.rs"), "root");

        // Absolute paths - extracts after /src/
        assert_eq!(
            extract_module_name("/home/user/project/src/git/branch.rs"),
            "git"
        );
        assert_eq!(
            extract_module_name("/home/user/project/src/mcp_server/mod.rs"),
            "mcp_server"
        );
        assert_eq!(
            extract_module_name("/home/user/my-test-worktree/src/App.tsx"),
            "App"
        );
        assert_eq!(
            extract_module_name("/home/user/my-test-worktree/src/utils/format.ts"),
            "utils"
        );

        // Nested directories use dots
        assert_eq!(
            extract_module_name("/project/src/server/api/handlers/users.ts"),
            "server.api.handlers"
        );

        // Unity paths (Assets/Scripts/)
        assert_eq!(
            extract_module_name("/project/Assets/Scripts/Game/Player.cs"),
            "Game"
        );
        assert_eq!(
            extract_module_name("/project/Assets/Scripts/UI/MainMenu.cs"),
            "UI"
        );
        assert_eq!(
            extract_module_name("Assets/Scripts/Entities/Enemy.cs"),
            "Entities"
        );

        // Unity fallback (Assets/ without Scripts)
        assert_eq!(
            extract_module_name("/project/Assets/Editor/BuildTools.cs"),
            "Editor"
        );

        // Unreal paths (Source/)
        assert_eq!(
            extract_module_name("/project/Source/MyGame/Character.cpp"),
            "MyGame"
        );
        assert_eq!(
            extract_module_name("Source/Game/Weapons/Gun.h"),
            "Game.Weapons"
        );

        // Godot paths (scripts/)
        assert_eq!(
            extract_module_name("/project/scripts/player/movement.gd"),
            "player"
        );
        assert_eq!(extract_module_name("scripts/enemies/boss.gd"), "enemies");

        // Monorepo paths (packages/)
        assert_eq!(
            extract_module_name("/repo/packages/core/utils/format.ts"),
            "core.utils"
        );
        assert_eq!(
            extract_module_name("packages/api/handlers/auth.ts"),
            "api.handlers"
        );

        // Absolute paths with project subdirectories (tests/, docs/, etc.)
        // These should be detected as project-relative
        assert_eq!(
            extract_module_name("/home/user/Dev/my-project/tests/test_db.py"),
            "tests"
        );
        assert_eq!(
            extract_module_name("/home/user/projects/semfora-pm/tests/unit/test_api.py"),
            "tests.unit"
        );
        assert_eq!(
            extract_module_name("/home/user/code/my-app/docs/api.md"),
            "docs"
        );
        assert_eq!(
            extract_module_name("/home/kadajett/Dev/Semfora_org/semfora-pm/tickets/backlog.yaml"),
            "tickets"
        );

        // Python packages with underscore naming
        assert_eq!(
            extract_module_name("/home/user/project/semfora_pm/db/connection.py"),
            "semfora_pm.db"
        );
        assert_eq!(
            extract_module_name("/home/user/project/my_package/utils/helpers.py"),
            "my_package.utils"
        );

        // Fallback for hyphenated project names
        assert_eq!(
            extract_module_name("/home/user/Dev/my-cool-project/config/settings.toml"),
            "config"
        );
    }

    #[test]
    fn test_shard_stats() {
        let stats = ShardStats {
            overview_bytes: 100,
            module_bytes: 500,
            symbol_bytes: 300,
            graph_bytes: 200,
            ..Default::default()
        };

        assert_eq!(stats.total_bytes(), 1100);
    }

    // ========================================================================
    // Conflict-Aware Module Name Stripping Tests
    // ========================================================================

    #[test]
    fn test_strip_first_component() {
        // Normal stripping
        assert_eq!(
            strip_first_component("src.game.player"),
            Some("game.player".to_string())
        );
        assert_eq!(
            strip_first_component("game.player"),
            Some("player".to_string())
        );
        assert_eq!(strip_first_component("a.b.c.d"), Some("b.c.d".to_string()));

        // Single component can't be stripped
        assert_eq!(strip_first_component("player"), None);
        assert_eq!(strip_first_component("root"), None);

        // Empty string
        assert_eq!(strip_first_component(""), None);
    }

    #[test]
    fn test_strip_n_components() {
        // Strip 0 returns original
        assert_eq!(strip_n_components("src.game.player", 0), "src.game.player");

        // Strip 1
        assert_eq!(strip_n_components("src.game.player", 1), "game.player");

        // Strip 2
        assert_eq!(strip_n_components("src.game.player", 2), "player");

        // Strip more than available returns original (no panic)
        assert_eq!(strip_n_components("src.game.player", 5), "src.game.player");

        // Single component
        assert_eq!(strip_n_components("root", 0), "root");
        assert_eq!(strip_n_components("root", 1), "root");
    }

    #[test]
    fn test_compute_optimal_names_no_conflict() {
        // All unique after full stripping - algorithm strips as much as possible
        let paths = vec![
            "src.game.player".to_string(),
            "src.game.enemy".to_string(),
            "src.utils.format".to_string(),
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Strips all the way to single final components since no conflicts
        // src.game.player -> game.player -> player (unique, keep stripping)
        assert_eq!(depth, 2);
        assert_eq!(result, vec!["player", "enemy", "format"]);
    }

    #[test]
    fn test_compute_optimal_names_with_conflict() {
        // Conflict at final level
        let paths = vec![
            "src.game.player".to_string(),
            "src.game.enemy".to_string(),
            "src.map.player".to_string(), // Conflicts with game.player at "player" level
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Should strip "src." but stop before stripping "game."/"map." due to conflict
        assert_eq!(depth, 1);
        assert_eq!(result, vec!["game.player", "game.enemy", "map.player"]);
    }

    #[test]
    fn test_compute_optimal_names_immediate_conflict() {
        // Conflict at first strip level
        let paths = vec![
            "src.player".to_string(),
            "lib.player".to_string(), // Would conflict if we strip src./lib.
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Can't strip anything - immediate conflict
        assert_eq!(depth, 0);
        assert_eq!(result, vec!["src.player", "lib.player"]);
    }

    #[test]
    fn test_compute_optimal_names_single_component_does_not_block() {
        // Single-component modules should NOT block stripping for multi-component ones
        let paths = vec![
            "root".to_string(), // Single component - preserved as-is
            "src.game.player".to_string(),
            "src.game.enemy".to_string(),
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Single-component is preserved, multi-component modules are stripped
        // "player" and "enemy" are unique so they get fully stripped
        assert_eq!(result[0], "root"); // Unchanged
        assert_eq!(result[1], "player"); // Stripped from src.game.player
        assert_eq!(result[2], "enemy"); // Stripped from src.game.enemy
        assert_eq!(depth, 2); // Multi-component modules got stripped twice
    }

    #[test]
    fn test_compute_optimal_names_mixed_single_multi_with_conflict() {
        // Single-component + multi-component with conflict at final level
        let paths = vec![
            "main".to_string(), // Single component
            "src.game.player".to_string(),
            "src.map.player".to_string(), // Conflicts with game.player at "player" level
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Single-component preserved, multi-component stripped until conflict
        assert_eq!(result[0], "main"); // Unchanged
        assert_eq!(result[1], "game.player"); // Stripped to game.player (conflict prevents further)
        assert_eq!(result[2], "map.player"); // Stripped to map.player (conflict prevents further)
        assert_eq!(depth, 1); // Only stripped "src." due to conflict at next level
    }

    #[test]
    fn test_compute_optimal_names_all_single_component() {
        // All single-component - nothing to strip
        let paths = vec!["root".to_string(), "main".to_string(), "lib".to_string()];

        let (result, depth) = compute_optimal_names(&paths);

        assert_eq!(result, paths); // All unchanged
        assert_eq!(depth, 0); // No stripping occurred
    }

    #[test]
    fn test_compute_optimal_names_empty() {
        let paths: Vec<String> = vec![];
        let (result, depth) = compute_optimal_names(&paths);

        assert_eq!(depth, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compute_optimal_names_deep_strip() {
        // All modules share deep common prefix
        let paths = vec![
            "home.user.project.src.api.users".to_string(),
            "home.user.project.src.api.auth".to_string(),
            "home.user.project.src.utils.helpers".to_string(),
        ];

        let (result, depth) = compute_optimal_names(&paths);

        // Strips all the way to final single component since all are unique
        // home.user.project.src.api.users -> ... -> users
        assert_eq!(depth, 5);
        assert_eq!(result, vec!["users", "auth", "helpers"]);
    }

    #[test]
    fn test_module_registry_basic() {
        // Use paths that will have conflicts to test partial stripping
        let paths = vec![
            "src.game.player".to_string(),
            "src.game.enemy".to_string(),
            "src.map.player".to_string(), // Conflicts with game.player at final level
        ];

        let registry = ModuleRegistry::from_full_paths(&paths);

        // With conflict, stops at game.player/map.player level
        assert_eq!(
            registry.get_short("src.game.player"),
            Some(&"game.player".to_string())
        );
        assert_eq!(
            registry.get_short("src.game.enemy"),
            Some(&"game.enemy".to_string())
        );
        assert_eq!(
            registry.get_short("src.map.player"),
            Some(&"map.player".to_string())
        );

        // Reverse lookup
        assert_eq!(
            registry.get_full("game.player"),
            Some(&"src.game.player".to_string())
        );

        // Non-existent
        assert_eq!(registry.get_short("nonexistent"), None);

        // Stats
        assert_eq!(registry.len(), 3);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_module_registry_conflict_detection() {
        let paths = vec!["src.game.player".to_string(), "src.map.player".to_string()];

        let registry = ModuleRegistry::from_full_paths(&paths);

        // Both should be preserved with parent prefix
        assert_eq!(
            registry.get_short("src.game.player"),
            Some(&"game.player".to_string())
        );
        assert_eq!(
            registry.get_short("src.map.player"),
            Some(&"map.player".to_string())
        );

        // "player" alone would conflict
        assert!(registry.has_conflict("game.player"));
        assert!(registry.has_conflict("map.player"));
        assert!(!registry.has_conflict("nonexistent"));
    }

    #[test]
    fn test_compute_full_module_path() {
        // Basic path
        assert_eq!(compute_full_module_path("src/game/player.rs"), "src.game");

        // Deeper path
        assert_eq!(
            compute_full_module_path("src/server/api/handlers/users.ts"),
            "src.server.api.handlers"
        );

        // Root file with generic name
        assert_eq!(compute_full_module_path("main.rs"), "root");
        assert_eq!(compute_full_module_path("index.ts"), "root");

        // Root file with specific name
        assert_eq!(compute_full_module_path("utils.ts"), "utils");
    }

    // ========================================================================
    // detect_project_relative_path() tests
    // ========================================================================

    #[test]
    fn test_detect_project_relative_path_tests_dir() {
        // Should find /tests/ and return from there
        assert_eq!(
            detect_project_relative_path("/home/user/project/tests/test_main.py"),
            "tests/test_main.py"
        );
        assert_eq!(
            detect_project_relative_path("/home/kadajett/Dev/semfora/tests/unit/test_api.py"),
            "tests/unit/test_api.py"
        );
    }

    #[test]
    fn test_detect_project_relative_path_docs_dir() {
        assert_eq!(
            detect_project_relative_path("/home/user/project/docs/api.md"),
            "docs/api.md"
        );
        assert_eq!(
            detect_project_relative_path("/home/user/code/my-app/doc/README.md"),
            "doc/README.md"
        );
    }

    #[test]
    fn test_detect_project_relative_path_scripts_dir() {
        assert_eq!(
            detect_project_relative_path("/home/user/project/scripts/build.sh"),
            "scripts/build.sh"
        );
    }

    #[test]
    fn test_detect_project_relative_path_examples_dir() {
        assert_eq!(
            detect_project_relative_path("/home/user/project/examples/demo.rs"),
            "examples/demo.rs"
        );
    }

    #[test]
    fn test_detect_project_relative_path_benchmarks_dir() {
        assert_eq!(
            detect_project_relative_path("/home/user/project/benchmarks/perf_test.rs"),
            "benchmarks/perf_test.rs"
        );
        assert_eq!(
            detect_project_relative_path("/home/user/project/benches/criterion.rs"),
            "benches/criterion.rs"
        );
    }

    #[test]
    fn test_detect_project_relative_path_jest_dirs() {
        assert_eq!(
            detect_project_relative_path("/home/user/project/__tests__/unit.test.ts"),
            "__tests__/unit.test.ts"
        );
        assert_eq!(
            detect_project_relative_path("/home/user/project/__mocks__/api.ts"),
            "__mocks__/api.ts"
        );
    }

    #[test]
    fn test_detect_project_relative_path_python_package() {
        // Python packages with underscores
        assert_eq!(
            detect_project_relative_path("/home/user/project/my_package/utils/helpers.py"),
            "my_package/utils/helpers.py"
        );
        assert_eq!(
            detect_project_relative_path("/home/user/Dev/semfora_pm/db/connection.py"),
            "semfora_pm/db/connection.py"
        );
    }

    #[test]
    fn test_detect_project_relative_path_hyphenated_project() {
        // Project directories with hyphens
        assert_eq!(
            detect_project_relative_path("/home/user/Dev/my-cool-project/config/settings.toml"),
            "config/settings.toml"
        );
    }

    #[test]
    fn test_detect_project_relative_path_fallback() {
        // Short paths should return as-is
        let short = "main.rs";
        assert_eq!(detect_project_relative_path(short), short);
    }

    // ========================================================================
    // extract_call_from_initializer() tests
    // ========================================================================

    #[test]
    fn test_extract_call_simple_function() {
        // Simple function call
        assert_eq!(
            extract_call_from_initializer("useState()"),
            Some("useState".to_string())
        );
        assert_eq!(
            extract_call_from_initializer("getData()"),
            Some("getData".to_string())
        );
    }

    #[test]
    fn test_extract_call_with_arguments() {
        // Function call with arguments
        assert_eq!(
            extract_call_from_initializer("useState(false)"),
            Some("useState".to_string())
        );
        assert_eq!(
            extract_call_from_initializer("fetchUser(userId)"),
            Some("fetchUser".to_string())
        );
    }

    #[test]
    fn test_extract_call_method_call() {
        // Method calls extract the last part - but common names like "get", "query" may be filtered as noise
        // The function uses rsplit('.') to get the last part, then filters noise

        // "get" is in the noise list, so axios.get returns None
        assert_eq!(extract_call_from_initializer("axios.get('/users')"), None);

        // More specific method names that aren't noise should work
        assert_eq!(
            extract_call_from_initializer("client.fetchUser()"),
            Some("fetchUser".to_string())
        );
        assert_eq!(
            extract_call_from_initializer("api.createOrder()"),
            Some("createOrder".to_string())
        );
    }

    #[test]
    fn test_extract_call_skip_literals() {
        // String literals
        assert_eq!(extract_call_from_initializer("\"hello\""), None);
        assert_eq!(extract_call_from_initializer("'world'"), None);

        // Numeric literals
        assert_eq!(extract_call_from_initializer("42"), None);
        assert_eq!(extract_call_from_initializer("3.14"), None);

        // Boolean literals
        assert_eq!(extract_call_from_initializer("true"), None);
        assert_eq!(extract_call_from_initializer("false"), None);

        // Null/undefined
        assert_eq!(extract_call_from_initializer("null"), None);
        assert_eq!(extract_call_from_initializer("undefined"), None);
    }

    #[test]
    fn test_extract_call_empty() {
        assert_eq!(extract_call_from_initializer(""), None);
        assert_eq!(extract_call_from_initializer("  "), None);
    }

    #[test]
    fn test_extract_call_array_object_literals() {
        // Array literal
        assert_eq!(extract_call_from_initializer("[]"), None);

        // Object literal
        assert_eq!(extract_call_from_initializer("{}"), None);
    }

    #[test]
    fn test_extract_call_new_expression() {
        // new expressions are treated as a single call "new X"
        // The function doesn't specifically parse JS "new" keyword, it just extracts the function call part
        assert_eq!(
            extract_call_from_initializer("new Map()"),
            Some("new Map".to_string())
        );
        assert_eq!(
            extract_call_from_initializer("new Date()"),
            Some("new Date".to_string())
        );

        // More complex new expressions
        assert_eq!(
            extract_call_from_initializer("new Promise()"),
            Some("new Promise".to_string())
        );
    }

    // ========================================================================
    // build_call_graph() tests (integration-style)
    // ========================================================================

    #[test]
    fn test_build_call_graph_empty() {
        let summaries: Vec<SemanticSummary> = vec![];
        let graph = build_call_graph(&summaries, &None);
        assert!(
            graph.is_empty(),
            "Empty summaries should produce empty graph"
        );
    }

    #[test]
    fn test_build_call_graph_no_calls() {
        use crate::schema::SymbolInfo;

        // Summaries with symbols but no calls
        let summaries = vec![SemanticSummary {
            file: "src/main.ts".to_string(),
            language: "typescript".to_string(),
            symbols: vec![SymbolInfo {
                name: "main".to_string(),
                kind: crate::schema::SymbolKind::Function,
                start_line: 1,
                end_line: 10,
                calls: vec![], // No calls
                ..Default::default()
            }],
            ..Default::default()
        }];

        let graph = build_call_graph(&summaries, &None);
        // No calls means no edges in the graph
        assert!(
            graph.is_empty() || graph.values().all(|v| v.is_empty()),
            "No calls should produce empty or no-edge graph"
        );
    }

    #[test]
    fn test_build_call_graph_with_calls() {
        use crate::schema::{Call, SymbolId, SymbolKind};

        let summaries = vec![
            SemanticSummary {
                file: "src/main.ts".to_string(),
                language: "typescript".to_string(),
                symbol_id: Some(SymbolId {
                    hash: "abcd1234:main_hash0001".to_string(),
                    semantic_hash: "main_hash0001".to_string(),
                    namespace: "src".to_string(),
                    symbol: "main".to_string(),
                    kind: SymbolKind::Function,
                    arity: 0,
                }),
                symbols: vec![],
                calls: vec![Call {
                    name: "helper".to_string(),
                    object: None,
                    ..Default::default()
                }],
                ..Default::default()
            },
            SemanticSummary {
                file: "src/utils.ts".to_string(),
                language: "typescript".to_string(),
                symbol_id: Some(SymbolId {
                    hash: "efgh5678:helper_hash01".to_string(),
                    semantic_hash: "helper_hash01".to_string(),
                    namespace: "src".to_string(),
                    symbol: "helper".to_string(),
                    kind: SymbolKind::Function,
                    arity: 0,
                }),
                symbols: vec![],
                calls: vec![],
                ..Default::default()
            },
        ];

        let graph = build_call_graph(&summaries, &None);
        // Should have at least one entry for main calling helper
        assert!(!graph.is_empty(), "Should produce a call graph with edges");
    }

    // ========================================================================
    // ShardStats tests
    // ========================================================================

    #[test]
    fn test_shard_stats_total_bytes_all_fields() {
        let stats = ShardStats {
            overview_bytes: 1000,
            module_bytes: 2000,
            symbol_bytes: 3000,
            graph_bytes: 4000,
            index_bytes: 5000,
            ..Default::default()
        };

        assert_eq!(stats.total_bytes(), 15000);
    }

    #[test]
    fn test_shard_stats_default() {
        let stats = ShardStats::default();
        assert_eq!(stats.total_bytes(), 0);
        assert_eq!(stats.files_written, 0);
        assert_eq!(stats.symbols_written, 0);
        assert_eq!(stats.modules_written, 0);
    }
}
