//! Helper functions for the MCP server
//!

// Helpers module - cache freshness and symbol validation utilities

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use std::collections::{HashMap, HashSet};

use super::formatting::toon_header;
use crate::duplicate::DuplicateDetector;
use crate::indexing::{
    analyze_files_with_stats as indexing_analyze_files_with_stats,
    collect_files as indexing_collect_files, should_skip_path as indexing_should_skip_path,
};
use crate::cache::{load_function_signatures as cache_load_function_signatures, split_respecting_quotes};
use crate::{
    extract_module_name, CacheDir, Lang, SemanticSummary, ShardWriter, SymbolIndexEntry,
};

// ============================================================================
// Staleness Info Struct
// ============================================================================

/// Detailed staleness information for cache validation
#[derive(Debug, Clone)]
pub struct StalenessInfo {
    /// Whether the cache is considered stale
    pub is_stale: bool,
    /// Age of the cache in seconds
    pub age_seconds: u64,
    /// List of modified files (relative paths)
    pub modified_files: Vec<String>,
    /// Total number of files checked
    pub files_checked: usize,
}

// ============================================================================
// Index Generation Result
// ============================================================================

/// Result of index auto-generation
#[derive(Debug, Clone)]
pub struct IndexGenerationResult {
    /// Time taken to generate the index in milliseconds
    pub duration_ms: u64,
    /// Number of files analyzed
    pub files_analyzed: usize,
    /// Number of modules written
    pub modules_written: usize,
    /// Number of symbols written
    pub symbols_written: usize,
    /// Compression percentage achieved
    pub compression_pct: f64,
}

// ============================================================================
// Cache Staleness Detection
// ============================================================================

/// Check cache staleness with detailed information
///
/// Returns detailed staleness info including age, modified files, and whether
/// auto-refresh should be triggered based on the max_age threshold.
pub fn check_cache_staleness_detailed(cache: &CacheDir, max_age_seconds: u64) -> StalenessInfo {
    let overview_path = cache.repo_overview_path();

    // Get overview mtime
    let overview_mtime = match fs::metadata(&overview_path) {
        Ok(m) => m.modified().ok(),
        Err(_) => {
            return StalenessInfo {
                is_stale: true,
                age_seconds: 0,
                modified_files: vec![],
                files_checked: 0,
            };
        }
    };

    let overview_time = match overview_mtime {
        Some(t) => t,
        None => {
            return StalenessInfo {
                is_stale: true,
                age_seconds: 0,
                modified_files: vec![],
                files_checked: 0,
            };
        }
    };

    // Calculate age
    let age_seconds = SystemTime::now()
        .duration_since(overview_time)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Collect modified files
    let mut modified_files = Vec::new();
    let max_to_check = 200;
    let mut files_checked = 0;

    if let Ok(entries) = collect_source_files_for_staleness(&cache.repo_root, max_to_check) {
        files_checked = entries.len();
        for (path, mtime) in entries {
            if mtime > overview_time {
                modified_files.push(path);
            }
        }
    }

    // Determine if stale based on age OR modified files
    let is_stale = age_seconds > max_age_seconds || !modified_files.is_empty();

    StalenessInfo {
        is_stale,
        age_seconds,
        modified_files,
        files_checked,
    }
}

/// Collect source files with their modification times for staleness checking
fn collect_source_files_for_staleness(
    dir: &Path,
    max_files: usize,
) -> std::io::Result<Vec<(String, SystemTime)>> {
    let mut results = Vec::new();
    collect_source_files_recursive(dir, 5, 0, &mut results, max_files);
    Ok(results)
}

fn collect_source_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    results: &mut Vec<(String, SystemTime)>,
    max_files: usize,
) {
    if current_depth > max_depth || results.len() >= max_files {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if results.len() >= max_files {
            return;
        }

        let path = entry.path();

        // Skip hidden files/directories and common non-source directories
        if indexing_should_skip_path(&path) {
            continue;
        }

        if path.is_dir() {
            collect_source_files_recursive(&path, max_depth, current_depth + 1, results, max_files);
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Only check supported source files
                if Lang::from_extension(ext).is_ok() {
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(mtime) = metadata.modified() {
                            let rel_path = path
                                .strip_prefix(dir)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();
                            results.push((rel_path, mtime));
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Index Generation
// ============================================================================

/// Generate a sharded index for a directory.
///
/// This is the core indexing logic used by both `generate_index` (explicit)
/// and `ensure_index` (auto-generation). Returns statistics about what was generated.
pub fn generate_index_internal(
    dir_path: &Path,
    max_depth: usize,
    extensions: &[String],
) -> Result<IndexGenerationResult, String> {
    let start = std::time::Instant::now();

    // Create shard writer
    let mut shard_writer = ShardWriter::new(dir_path)
        .map_err(|e| format!("Failed to initialize shard writer: {}", e))?;

    // Collect files
    let files = indexing_collect_files(dir_path, max_depth, extensions);

    if files.is_empty() {
        return Ok(IndexGenerationResult {
            duration_ms: start.elapsed().as_millis() as u64,
            files_analyzed: 0,
            modules_written: 0,
            symbols_written: 0,
            compression_pct: 0.0,
        });
    }

    // Analyze files
    let (summaries, total_bytes) = indexing_analyze_files_with_stats(&files);

    // Add summaries to shard writer
    shard_writer.add_summaries(summaries.clone());

    // Write all shards
    let dir_str = dir_path.display().to_string();
    let stats = shard_writer
        .write_all(&dir_str)
        .map_err(|e| format!("Failed to write shards: {}", e))?;

    // Set the indexed SHA and status hash for staleness tracking
    if let Ok(cache) = CacheDir::for_repo(dir_path) {
        if let Ok(sha) = crate::git::git_command(&["rev-parse", "HEAD"], Some(dir_path)) {
            let _ = cache.set_indexed_sha(&sha);
        }
        // Also save status hash so we don't re-index the same uncommitted changes
        if let Some(status_hash) = cache.compute_status_hash() {
            let _ = cache.set_status_hash(&status_hash);
        }
    }

    // Calculate compression
    let compression = if total_bytes > 0 {
        ((total_bytes as f64 - stats.total_bytes() as f64) / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(IndexGenerationResult {
        duration_ms: start.elapsed().as_millis() as u64,
        files_analyzed: summaries.len(),
        modules_written: stats.modules_written,
        symbols_written: stats.symbols_written,
        compression_pct: compression,
    })
}

// ============================================================================
// Partial Reindexing
// ============================================================================

/// Result of a partial reindex operation
#[derive(Debug, Clone)]
pub struct PartialReindexResult {
    /// Number of files that were reindexed
    pub files_reindexed: usize,
    /// Number of modules that were updated
    pub modules_updated: usize,
    /// Time taken in milliseconds
    pub duration_ms: u64,
}

/// Partially reindex only the changed files.
///
/// This is an incremental update that:
/// 1. Analyzes only the changed files
/// 2. Groups them by module
/// 3. For each affected module, loads existing summaries, removes old entries
///    for changed files, adds new summaries, and rewrites the module shard
/// 4. Updates the indexed SHA to current HEAD
///
/// This is much faster than a full reindex for small changes (<50 files).
pub fn partial_reindex(
    cache: &CacheDir,
    changed_files: &[PathBuf],
) -> Result<PartialReindexResult, String> {
    let start = std::time::Instant::now();

    // Filter to only valid source files
    let valid_files: Vec<PathBuf> = changed_files
        .iter()
        .filter(|f| f.exists() && Lang::from_path(f).is_ok())
        .cloned()
        .collect();

    if valid_files.is_empty() {
        // No valid files to reindex - just update the SHA
        if let Ok(sha) = crate::git::git_command(&["rev-parse", "HEAD"], Some(&cache.repo_root)) {
            let _ = cache.set_indexed_sha(&sha);
        }
        return Ok(PartialReindexResult {
            files_reindexed: 0,
            modules_updated: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Analyze only the changed files (parallel)
    let (new_summaries, _) = indexing_analyze_files_with_stats(&valid_files);

    // Build file-to-module mapping from existing cache for consistent module names
    // This ensures partial reindex uses the same module names as the full index
    let file_to_module: HashMap<String, String> = {
        let mut map = HashMap::new();
        for module_name in cache.list_modules() {
            if let Ok(summaries) = cache.load_module_summaries(&module_name) {
                for summary in summaries {
                    map.insert(summary.file.clone(), module_name.clone());
                }
            }
        }
        map
    };

    // Group new summaries by module
    let mut new_by_module: HashMap<String, Vec<SemanticSummary>> = HashMap::new();
    for summary in &new_summaries {
        // Get the optimal module name from cache, fallback to extraction
        let module = file_to_module
            .get(&summary.file)
            .cloned()
            .unwrap_or_else(|| extract_module_name(&summary.file));
        new_by_module
            .entry(module)
            .or_default()
            .push(summary.clone());
    }

    // Track which files were changed (for removing old entries)
    let changed_file_set: HashSet<String> = valid_files
        .iter()
        .filter_map(|p| p.to_str())
        .map(|s| s.to_string())
        .collect();

    // Also track modules that might have files removed (deleted files)
    let deleted_files: Vec<&PathBuf> = changed_files.iter().filter(|f| !f.exists()).collect();

    for deleted in &deleted_files {
        if let Some(path_str) = deleted.to_str() {
            // Get the optimal module name from cache, fallback to extraction
            let module = file_to_module
                .get(path_str)
                .cloned()
                .unwrap_or_else(|| extract_module_name(path_str));
            // Ensure module is in our update set even if no new summaries
            new_by_module.entry(module).or_default();
            // Track this file as changed (for removal)
            // Note: we use the path string since the file doesn't exist
        }
    }

    let mut modules_updated = 0;

    // Update each affected module
    for (module_name, new_module_summaries) in &new_by_module {
        // Load existing summaries for this module
        let existing = cache.load_module_summaries(module_name).unwrap_or_default();

        // Remove entries for changed files (they'll be replaced or deleted)
        let mut updated: Vec<SemanticSummary> = existing
            .into_iter()
            .filter(|s| !changed_file_set.contains(&s.file))
            .collect();

        // Add new summaries
        updated.extend(new_module_summaries.clone());

        // Skip writing empty modules (all files deleted)
        if updated.is_empty() {
            // Delete the module file
            let module_path = cache.module_path(module_name);
            let _ = fs::remove_file(module_path);
            modules_updated += 1;
            continue;
        }

        // Encode and write the updated module shard
        let toon = crate::shard::encode_module_shard(module_name, &updated, &cache.repo_root);
        let module_path = cache.module_path(module_name);

        if let Err(e) = fs::write(&module_path, toon) {
            return Err(format!("Failed to write module {}: {}", module_name, e));
        }

        modules_updated += 1;
    }

    // Update symbol shards for changed files
    // For simplicity, we'll delete old symbol files and create new ones
    // (Symbol files are keyed by hash which includes file path)
    update_symbol_shards(cache, &new_summaries)?;

    // Update the indexed SHA
    if let Ok(sha) = crate::git::git_command(&["rev-parse", "HEAD"], Some(&cache.repo_root)) {
        let _ = cache.set_indexed_sha(&sha);
    }

    // Update the status hash (so we don't re-index the same uncommitted changes)
    if let Some(status_hash) = cache.compute_status_hash() {
        let _ = cache.set_status_hash(&status_hash);
    }

    // Touch repo_overview to update mtime (stats might be slightly stale but that's OK)
    let overview_path = cache.repo_overview_path();
    if overview_path.exists() {
        // Just touch the file to update mtime
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(&overview_path)
            .and_then(|f| f.set_len(f.metadata()?.len()));
    }

    Ok(PartialReindexResult {
        files_reindexed: valid_files.len(),
        modules_updated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Update symbol shards for the given summaries
fn update_symbol_shards(cache: &CacheDir, summaries: &[SemanticSummary]) -> Result<(), String> {
    use crate::schema::SymbolId;

    for summary in summaries {
        let namespace = SymbolId::namespace_from_path(&summary.file);

        // If we have symbols in the multi-symbol format, use those
        if !summary.symbols.is_empty() {
            for symbol_info in &summary.symbols {
                let symbol_id = symbol_info.to_symbol_id(&namespace, &summary.file);
                let toon =
                    crate::shard::encode_symbol_shard_from_info(summary, symbol_info, &symbol_id);
                let path = cache.symbol_path(&symbol_id.hash);

                if let Err(e) = fs::write(&path, toon) {
                    return Err(format!("Failed to write symbol {}: {}", symbol_id.hash, e));
                }
            }
        } else if let Some(symbol_id) = SymbolId::from_summary(summary) {
            // Fallback to primary symbol
            let toon = crate::shard::encode_symbol_shard(summary);
            let path = cache.symbol_path(&symbol_id.hash);

            if let Err(e) = fs::write(&path, toon) {
                return Err(format!("Failed to write symbol {}: {}", symbol_id.hash, e));
            }
        }
    }

    Ok(())
}

// ============================================================================
// Automatic Index Freshness
// ============================================================================

/// Result of ensuring index freshness
#[derive(Clone)]
pub struct FreshnessResult {
    /// The cache directory (ready to use)
    pub cache: CacheDir,
    /// Whether the index was refreshed
    pub refreshed: bool,
    /// How it was refreshed (if at all)
    pub refresh_type: RefreshType,
    /// Number of files updated (if partial refresh)
    pub files_updated: usize,
    /// Time taken for refresh in milliseconds
    pub duration_ms: u64,
}

/// Type of index refresh performed
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshType {
    /// Index was already fresh
    None,
    /// Only changed files were reindexed
    Partial,
    /// Full index regeneration
    Full,
}

impl RefreshType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefreshType::None => "none",
            RefreshType::Partial => "partial",
            RefreshType::Full => "full",
        }
    }
}

/// Default threshold for partial vs full reindex
const DEFAULT_MAX_STALE_FILES: usize = 50;

/// Ensure the index is fresh before executing a query.
///
/// This function is called at the start of query tools to transparently
/// handle stale indexes. It:
/// 1. Checks if the index exists and is fresh
/// 2. If stale with few changes (<= max_stale_files): partial reindex
/// 3. If stale with many changes (> max_stale_files): full reindex
/// 4. If no index exists: full index generation
///
/// The decision of what to reindex is made entirely by the engine based on
/// git status and file changes - the LLM does not influence this decision.
pub fn ensure_fresh_index(
    repo_path: &Path,
    max_stale_files: Option<usize>,
) -> Result<FreshnessResult, String> {
    let start = std::time::Instant::now();
    let threshold = max_stale_files.unwrap_or(DEFAULT_MAX_STALE_FILES);

    // Get or create cache directory
    let cache =
        CacheDir::for_repo(repo_path).map_err(|e| format!("Failed to access cache: {}", e))?;

    // Check if index exists at all
    let overview_path = cache.repo_overview_path();
    if !overview_path.exists() {
        // No index exists - do full generation
        let result = generate_index_internal(repo_path, 10, &[])?;

        // Re-get cache after generation (it may have been created)
        let cache = CacheDir::for_repo(repo_path)
            .map_err(|e| format!("Failed to access cache after generation: {}", e))?;

        return Ok(FreshnessResult {
            cache,
            refreshed: true,
            refresh_type: RefreshType::Full,
            files_updated: result.files_analyzed,
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Index exists - check staleness
    let staleness = cache.quick_staleness_check();

    if !staleness.is_stale {
        // Index is fresh - nothing to do
        return Ok(FreshnessResult {
            cache,
            refreshed: false,
            refresh_type: RefreshType::None,
            files_updated: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Index is stale - decide between partial and full reindex
    let changed_count = staleness.changed_files.len();

    if changed_count <= threshold && changed_count > 0 {
        // Partial reindex - only update changed files
        let result = partial_reindex(&cache, &staleness.changed_files)?;

        return Ok(FreshnessResult {
            cache,
            refreshed: true,
            refresh_type: RefreshType::Partial,
            files_updated: result.files_reindexed,
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Too many changes or can't determine - full reindex
    let result = generate_index_internal(repo_path, 10, &[])?;

    // Update the indexed SHA after full reindex
    if let Ok(sha) = crate::git::git_command(&["rev-parse", "HEAD"], Some(&cache.repo_root)) {
        let _ = cache.set_indexed_sha(&sha);
    }

    Ok(FreshnessResult {
        cache,
        refreshed: true,
        refresh_type: RefreshType::Full,
        files_updated: result.files_analyzed,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Format a freshness note for inclusion in query responses
pub fn format_freshness_note(result: &FreshnessResult) -> Option<String> {
    if !result.refreshed {
        return None;
    }

    match result.refresh_type {
        RefreshType::None => None,
        RefreshType::Partial => Some(format!(
            "⚡ Index refreshed ({} files updated in {}ms)",
            result.files_updated, result.duration_ms
        )),
        RefreshType::Full => Some(format!(
            "🔄 Index regenerated ({} files in {}ms)",
            result.files_updated, result.duration_ms
        )),
    }
}

// ============================================================================
// Symbol Validation Helpers
// ============================================================================

/// Information about a duplicate match
#[derive(Debug, Clone)]
pub struct DuplicateMatch {
    /// Name of the duplicate symbol
    pub name: String,
    /// File containing the duplicate
    pub file: String,
    /// Similarity score (0.0 to 1.0)
    pub similarity: f64,
}

/// Information about a caller
#[derive(Debug, Clone)]
pub struct CallerInfo {
    /// Name of the calling symbol
    pub name: String,
    /// Hash of the calling symbol
    pub hash: String,
    /// Risk level of the caller
    pub risk: String,
}

/// Result of validating a single symbol
#[derive(Debug, Clone)]
pub struct SymbolValidationResult {
    /// Symbol name
    pub symbol: String,
    /// File containing the symbol
    pub file: String,
    /// Line range (e.g., "45-89")
    pub lines: String,
    /// Symbol kind (fn, struct, etc.)
    pub kind: String,
    /// Symbol hash
    pub hash: String,
    /// Cognitive complexity score
    pub cognitive_complexity: usize,
    /// Maximum nesting depth
    pub max_nesting: usize,
    /// Risk level
    pub risk: String,
    /// Complexity-related concerns
    pub complexity_concerns: Vec<String>,
    /// Similar symbols (potential duplicates)
    pub duplicates: Vec<DuplicateMatch>,
    /// Functions that call this symbol
    pub callers: Vec<CallerInfo>,
    /// High-risk callers specifically
    pub high_risk_callers: Vec<String>,
    /// Actionable suggestions
    pub suggestions: Vec<String>,
}

/// Find a symbol by hash in the index
pub fn find_symbol_by_hash(cache: &CacheDir, hash: &str) -> Result<SymbolIndexEntry, String> {
    let entries = cache
        .load_all_symbol_entries()
        .map_err(|e| format!("Failed to load symbol index: {}", e))?;

    entries
        .into_iter()
        .find(|e| e.hash == hash)
        .ok_or_else(|| format!("Symbol {} not found in index", hash))
}

/// Find a symbol by file path and line number
pub fn find_symbol_by_location(
    cache: &CacheDir,
    file_path: &str,
    line: usize,
) -> Result<SymbolIndexEntry, String> {
    let entries = cache
        .load_all_symbol_entries()
        .map_err(|e| format!("Failed to load symbol index: {}", e))?;

    entries
        .into_iter()
        .find(|e| {
            // Check if file matches
            if !e.file.ends_with(file_path) && !file_path.ends_with(&e.file) {
                return false;
            }
            // Check if line is within range
            if let Some((start, end)) = e.lines.split_once('-') {
                if let (Ok(s), Ok(en)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    return line >= s && line <= en;
                }
            }
            false
        })
        .ok_or_else(|| format!("No symbol found at {}:{}", file_path, line))
}

/// Assess complexity concerns for a symbol
pub fn assess_complexity(entry: &SymbolIndexEntry) -> Vec<String> {
    let mut concerns = Vec::new();

    if entry.cognitive_complexity > 15 {
        concerns.push(
            "High cognitive complexity (>15) - consider breaking into smaller functions"
                .to_string(),
        );
    } else if entry.cognitive_complexity > 10 {
        concerns.push("Moderate cognitive complexity (>10) - may be hard to maintain".to_string());
    }

    if entry.max_nesting > 4 {
        concerns.push("Deep nesting (>4) - consider early returns or guard clauses".to_string());
    }

    concerns
}

/// Find duplicates for a symbol using the duplicate detector
pub fn find_symbol_duplicates(
    cache: &CacheDir,
    symbol_hash: &str,
    threshold: f64,
    max_results: usize,
) -> Vec<DuplicateMatch> {
    let mut duplicates = Vec::new();

    // Load signatures from cache
    let signatures = match cache_load_function_signatures(cache) {
        Ok(sigs) => sigs,
        Err(_) => return duplicates,
    };

    // Find the target signature
    let target_sig = match signatures.iter().find(|s| s.symbol_hash == symbol_hash) {
        Some(sig) => sig,
        None => return duplicates,
    };

    // Use duplicate detector
    let detector = DuplicateDetector::new(threshold);
    let matches = detector.find_duplicates(target_sig, &signatures);

    for m in matches.iter().take(max_results) {
        duplicates.push(DuplicateMatch {
            name: m.symbol.name.clone(),
            file: m.symbol.file.clone(),
            similarity: m.similarity,
        });
    }

    duplicates
}

/// Build a reverse call graph (callee -> callers) from the call graph file
pub fn build_reverse_call_graph(cache: &CacheDir) -> HashMap<String, Vec<String>> {
    let mut reverse_graph: HashMap<String, Vec<String>> = HashMap::new();

    let call_graph_path = cache.call_graph_path();
    if !call_graph_path.exists() {
        return reverse_graph;
    }

    let content = match fs::read_to_string(&call_graph_path) {
        Ok(c) => c,
        Err(_) => return reverse_graph,
    };

    for line in content.lines() {
        // Skip metadata lines
        if line.starts_with("_type:")
            || line.starts_with("schema_version:")
            || line.starts_with("edges:")
        {
            continue;
        }

        // Look for ": [" to find separator between caller hash and callee array
        // Using just ':' would incorrectly split hashes like "file_hash:symbol_hash"
        if let Some(sep_pos) = line.find(": [") {
            let caller = line[..sep_pos].trim().to_string();
            let rest = line[sep_pos + 2..].trim();

            if rest.starts_with('[') && rest.ends_with(']') {
                let inner = &rest[1..rest.len() - 1];
                // Use split_respecting_quotes for callees with commas in their names
                for callee in split_respecting_quotes(inner) {
                    // Skip external calls
                    if !callee.starts_with("ext:") {
                        reverse_graph
                            .entry(callee)
                            .or_default()
                            .push(caller.clone());
                    }
                }
            }
        }
    }

    reverse_graph
}

/// Find callers for a symbol
pub fn find_symbol_callers(
    cache: &CacheDir,
    symbol_hash: &str,
    max_callers: usize,
) -> (Vec<CallerInfo>, Vec<String>) {
    let mut callers = Vec::new();
    let mut high_risk_callers = Vec::new();

    // Build reverse call graph
    let reverse_graph = build_reverse_call_graph(cache);

    // Load symbol names for resolution
    let symbol_names: HashMap<String, (String, String)> = cache
        .load_all_symbol_entries()
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| (e.hash.clone(), (e.symbol.clone(), e.risk.clone())))
                .collect()
        })
        .unwrap_or_default();

    // Find direct callers
    if let Some(caller_hashes) = reverse_graph.get(symbol_hash) {
        for caller_hash in caller_hashes.iter().take(max_callers) {
            if let Some((name, risk)) = symbol_names.get(caller_hash) {
                callers.push(CallerInfo {
                    name: name.clone(),
                    hash: caller_hash.clone(),
                    risk: risk.clone(),
                });
                if risk == "high" {
                    high_risk_callers.push(name.clone());
                }
            } else {
                callers.push(CallerInfo {
                    name: caller_hash.clone(),
                    hash: caller_hash.clone(),
                    risk: "unknown".to_string(),
                });
            }
        }
    }

    (callers, high_risk_callers)
}

/// Generate suggestions based on validation results
pub fn generate_validation_suggestions(
    complexity_concerns: &[String],
    duplicates: &[DuplicateMatch],
    callers: &[CallerInfo],
) -> Vec<String> {
    let mut suggestions = Vec::new();

    // Add complexity concerns as suggestions
    suggestions.extend(complexity_concerns.iter().cloned());

    // Add duplicate suggestion if any
    if let Some(dup) = duplicates.first() {
        suggestions.push(format!(
            "{:.0}% similar to {} - consider consolidation",
            dup.similarity * 100.0,
            dup.name
        ));
    }

    // Add high impact radius suggestion
    if callers.len() > 10 {
        suggestions
            .push("High impact radius (>10 callers) - changes require careful testing".to_string());
    }

    suggestions
}

/// Validate a single symbol and return the validation result
pub fn validate_single_symbol(
    cache: &CacheDir,
    entry: &SymbolIndexEntry,
    duplicate_threshold: f64,
) -> SymbolValidationResult {
    // Assess complexity
    let complexity_concerns = assess_complexity(entry);

    // Find duplicates
    let duplicates = find_symbol_duplicates(cache, &entry.hash, duplicate_threshold, 5);

    // Find callers
    let (callers, high_risk_callers) = find_symbol_callers(cache, &entry.hash, 20);

    // Generate suggestions
    let suggestions = generate_validation_suggestions(&complexity_concerns, &duplicates, &callers);

    SymbolValidationResult {
        symbol: entry.symbol.clone(),
        file: entry.file.clone(),
        lines: entry.lines.clone(),
        kind: entry.kind.clone(),
        hash: entry.hash.clone(),
        cognitive_complexity: entry.cognitive_complexity,
        max_nesting: entry.max_nesting,
        risk: entry.risk.clone(),
        complexity_concerns,
        duplicates,
        callers,
        high_risk_callers,
        suggestions,
    }
}

/// Format a single validation result to TOON format
pub fn format_validation_result(result: &SymbolValidationResult) -> String {
    let mut output = toon_header("validation_result");
    output.push_str(&format!("symbol: {}\n", result.symbol));
    output.push_str(&format!("file: {}\n", result.file));
    output.push_str(&format!("lines: {}\n", result.lines));
    output.push_str(&format!("kind: {}\n", result.kind));
    output.push_str(&format!("hash: {}\n", result.hash));

    output.push_str("\ncomplexity:\n");
    output.push_str(&format!("  cognitive: {}\n", result.cognitive_complexity));
    output.push_str(&format!("  max_nesting: {}\n", result.max_nesting));
    output.push_str(&format!("  risk: {}\n", result.risk));

    output.push_str("\nduplicates:\n");
    if result.duplicates.is_empty() {
        output.push_str("  (none found above threshold)\n");
    } else {
        output.push_str(&format!("  count: {}\n", result.duplicates.len()));
        for dup in &result.duplicates {
            output.push_str(&format!(
                "  - {} ({}) [{:.0}%]\n",
                dup.name,
                dup.file,
                dup.similarity * 100.0
            ));
        }
    }

    output.push_str("\ncallers:\n");
    output.push_str(&format!("  direct: {}\n", result.callers.len()));
    if !result.high_risk_callers.is_empty() {
        output.push_str(&format!(
            "  high_risk_callers: [{}]\n",
            result.high_risk_callers.join(", ")
        ));
    }
    if !result.callers.is_empty() {
        output.push_str("  list:\n");
        for caller in result.callers.iter().take(10) {
            output.push_str(&format!("    - {} ({})\n", caller.name, caller.hash));
        }
        if result.callers.len() > 10 {
            output.push_str(&format!("    ... and {} more\n", result.callers.len() - 10));
        }
    }

    output.push_str("\nsuggestions:\n");
    if result.suggestions.is_empty() {
        output.push_str("  (none - symbol looks good)\n");
    } else {
        for suggestion in &result.suggestions {
            output.push_str(&format!("  - {}\n", suggestion));
        }
    }

    output
}

/// Batch validate multiple symbols and return aggregated results
pub fn validate_symbols_batch(
    cache: &CacheDir,
    entries: &[SymbolIndexEntry],
    duplicate_threshold: f64,
) -> Vec<SymbolValidationResult> {
    entries
        .iter()
        .map(|entry| validate_single_symbol(cache, entry, duplicate_threshold))
        .collect()
}

/// Format batch validation results with summary
pub fn format_batch_validation_results(
    results: &[SymbolValidationResult],
    context_name: &str,
) -> String {
    // Summary header
    let mut output = toon_header("batch_validation_results");
    output.push_str(&format!("context: {}\n", context_name));
    output.push_str(&format!("total_symbols: {}\n", results.len()));

    // Calculate summary stats
    let high_complexity: Vec<_> = results
        .iter()
        .filter(|r| r.cognitive_complexity > 15)
        .collect();
    let moderate_complexity: Vec<_> = results
        .iter()
        .filter(|r| r.cognitive_complexity > 10 && r.cognitive_complexity <= 15)
        .collect();
    let deep_nesting: Vec<_> = results.iter().filter(|r| r.max_nesting > 4).collect();
    let with_duplicates: Vec<_> = results
        .iter()
        .filter(|r| !r.duplicates.is_empty())
        .collect();
    let high_impact: Vec<_> = results.iter().filter(|r| r.callers.len() > 10).collect();

    output.push_str("\nsummary:\n");
    output.push_str(&format!(
        "  high_complexity: {} (>15 cognitive)\n",
        high_complexity.len()
    ));
    output.push_str(&format!(
        "  moderate_complexity: {} (10-15 cognitive)\n",
        moderate_complexity.len()
    ));
    output.push_str(&format!(
        "  deep_nesting: {} (>4 levels)\n",
        deep_nesting.len()
    ));
    output.push_str(&format!(
        "  potential_duplicates: {}\n",
        with_duplicates.len()
    ));
    output.push_str(&format!(
        "  high_impact: {} (>10 callers)\n",
        high_impact.len()
    ));

    // List symbols needing attention (high complexity first)
    if !high_complexity.is_empty() {
        output.push_str("\nhigh_complexity_symbols:\n");
        for r in high_complexity.iter().take(10) {
            output.push_str(&format!(
                "  - {} (cc:{}, nest:{}) {}\n",
                r.symbol, r.cognitive_complexity, r.max_nesting, r.file
            ));
        }
        if high_complexity.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", high_complexity.len() - 10));
        }
    }

    if !deep_nesting.is_empty() {
        output.push_str("\ndeep_nesting_symbols:\n");
        for r in deep_nesting.iter().take(10) {
            output.push_str(&format!(
                "  - {} (nest:{}) {}\n",
                r.symbol, r.max_nesting, r.file
            ));
        }
        if deep_nesting.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", deep_nesting.len() - 10));
        }
    }

    if !with_duplicates.is_empty() {
        output.push_str("\nsymbols_with_duplicates:\n");
        for r in with_duplicates.iter().take(10) {
            if let Some(dup) = r.duplicates.first() {
                output.push_str(&format!(
                    "  - {} ~ {} ({:.0}%)\n",
                    r.symbol,
                    dup.name,
                    dup.similarity * 100.0
                ));
            }
        }
        if with_duplicates.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", with_duplicates.len() - 10));
        }
    }

    if !high_impact.is_empty() {
        output.push_str("\nhigh_impact_symbols:\n");
        for r in high_impact.iter().take(10) {
            output.push_str(&format!(
                "  - {} ({} callers) {}\n",
                r.symbol,
                r.callers.len(),
                r.file
            ));
        }
        if high_impact.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", high_impact.len() - 10));
        }
    }

    // All symbols table (compact)
    output.push_str(&format!(
        "\nall_symbols[{}]{{name,cc,nest,dups,callers,risk}}:\n",
        results.len()
    ));
    for r in results.iter().take(50) {
        output.push_str(&format!(
            "  {},{},{},{},{},{}\n",
            r.symbol,
            r.cognitive_complexity,
            r.max_nesting,
            r.duplicates.len(),
            r.callers.len(),
            r.risk
        ));
    }
    if results.len() > 50 {
        output.push_str(&format!("  ... and {} more\n", results.len() - 50));
    }

    output
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Complexity Assessment Tests
    // ========================================================================

    fn make_entry(cognitive: usize, nesting: usize) -> SymbolIndexEntry {
        SymbolIndexEntry {
            symbol: "test_fn".to_string(),
            hash: "abc123".to_string(),
            semantic_hash: "".to_string(),
            kind: "fn".to_string(),
            module: "test".to_string(),
            file: "test.rs".to_string(),
            lines: "1-10".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: cognitive,
            max_nesting: nesting,
            is_escape_local: false,
            framework_entry_point: crate::schema::FrameworkEntryPoint::None,
        }
    }

    #[test]
    fn test_assess_complexity_low() {
        let entry = make_entry(5, 2);
        let concerns = assess_complexity(&entry);
        assert!(concerns.is_empty());
    }

    #[test]
    fn test_assess_complexity_moderate() {
        let entry = make_entry(12, 3);
        let concerns = assess_complexity(&entry);
        assert_eq!(concerns.len(), 1);
        assert!(concerns[0].contains("Moderate cognitive complexity"));
    }

    #[test]
    fn test_assess_complexity_high() {
        let entry = make_entry(20, 3);
        let concerns = assess_complexity(&entry);
        assert_eq!(concerns.len(), 1);
        assert!(concerns[0].contains("High cognitive complexity"));
    }

    #[test]
    fn test_assess_complexity_deep_nesting() {
        let entry = make_entry(5, 5);
        let concerns = assess_complexity(&entry);
        assert_eq!(concerns.len(), 1);
        assert!(concerns[0].contains("Deep nesting"));
    }

    #[test]
    fn test_assess_complexity_multiple_concerns() {
        let entry = make_entry(20, 6);
        let concerns = assess_complexity(&entry);
        assert_eq!(concerns.len(), 2);
    }

    // ========================================================================
    // Validation Suggestion Tests
    // ========================================================================

    #[test]
    fn test_generate_validation_suggestions_empty() {
        let suggestions = generate_validation_suggestions(&[], &[], &[]);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_generate_validation_suggestions_complexity_only() {
        let concerns = vec!["High cognitive complexity".to_string()];
        let suggestions = generate_validation_suggestions(&concerns, &[], &[]);
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].contains("complexity"));
    }

    #[test]
    fn test_generate_validation_suggestions_with_duplicate() {
        let duplicates = vec![DuplicateMatch {
            name: "similar_fn".to_string(),
            file: "other.rs".to_string(),
            similarity: 0.95,
        }];
        let suggestions = generate_validation_suggestions(&[], &duplicates, &[]);
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].contains("similar_fn"));
        assert!(suggestions[0].contains("95%"));
    }

    #[test]
    fn test_generate_validation_suggestions_high_impact() {
        let callers: Vec<CallerInfo> = (0..15)
            .map(|i| CallerInfo {
                name: format!("caller_{}", i),
                hash: format!("hash_{}", i),
                risk: "low".to_string(),
            })
            .collect();
        let suggestions = generate_validation_suggestions(&[], &[], &callers);
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].contains("High impact radius"));
    }

    // ========================================================================
    // Freshness Note Formatting Tests
    // ========================================================================

    #[test]
    fn test_format_freshness_note_not_refreshed() {
        let result = FreshnessResult {
            cache: CacheDir::for_repo(&std::env::temp_dir()).unwrap(),
            refreshed: false,
            refresh_type: RefreshType::None,
            files_updated: 0,
            duration_ms: 0,
        };
        assert!(format_freshness_note(&result).is_none());
    }

    #[test]
    fn test_format_freshness_note_partial() {
        let result = FreshnessResult {
            cache: CacheDir::for_repo(&std::env::temp_dir()).unwrap(),
            refreshed: true,
            refresh_type: RefreshType::Partial,
            files_updated: 5,
            duration_ms: 123,
        };
        let note = format_freshness_note(&result).unwrap();
        assert!(note.contains("5 files"));
        assert!(note.contains("123ms"));
        assert!(note.contains("⚡"));
    }

    #[test]
    fn test_format_freshness_note_full() {
        let result = FreshnessResult {
            cache: CacheDir::for_repo(&std::env::temp_dir()).unwrap(),
            refreshed: true,
            refresh_type: RefreshType::Full,
            files_updated: 100,
            duration_ms: 500,
        };
        let note = format_freshness_note(&result).unwrap();
        assert!(note.contains("100 files"));
        assert!(note.contains("500ms"));
        assert!(note.contains("🔄"));
    }

    // ========================================================================
    // Validation Result Formatting Tests
    // ========================================================================

    #[test]
    fn test_format_validation_result_basic() {
        let result = SymbolValidationResult {
            symbol: "my_function".to_string(),
            file: "src/lib.rs".to_string(),
            lines: "10-25".to_string(),
            kind: "fn".to_string(),
            hash: "abc123".to_string(),
            cognitive_complexity: 8,
            max_nesting: 3,
            risk: "low".to_string(),
            complexity_concerns: vec![],
            duplicates: vec![],
            callers: vec![],
            high_risk_callers: vec![],
            suggestions: vec![],
        };

        let output = format_validation_result(&result);
        assert!(output.contains("_type: validation_result"));
        assert!(output.contains("symbol: my_function"));
        assert!(output.contains("file: src/lib.rs"));
        assert!(output.contains("cognitive: 8"));
        assert!(output.contains("(none - symbol looks good)"));
    }

    #[test]
    fn test_format_validation_result_with_duplicates() {
        let result = SymbolValidationResult {
            symbol: "duplicate_fn".to_string(),
            file: "src/a.rs".to_string(),
            lines: "1-10".to_string(),
            kind: "fn".to_string(),
            hash: "def456".to_string(),
            cognitive_complexity: 5,
            max_nesting: 2,
            risk: "low".to_string(),
            complexity_concerns: vec![],
            duplicates: vec![DuplicateMatch {
                name: "similar_fn".to_string(),
                file: "src/b.rs".to_string(),
                similarity: 0.92,
            }],
            callers: vec![],
            high_risk_callers: vec![],
            suggestions: vec![],
        };

        let output = format_validation_result(&result);
        assert!(output.contains("similar_fn"));
        assert!(output.contains("92%"));
    }

    #[test]
    fn test_format_validation_result_with_callers() {
        let result = SymbolValidationResult {
            symbol: "called_fn".to_string(),
            file: "src/lib.rs".to_string(),
            lines: "5-15".to_string(),
            kind: "fn".to_string(),
            hash: "ghi789".to_string(),
            cognitive_complexity: 3,
            max_nesting: 1,
            risk: "low".to_string(),
            complexity_concerns: vec![],
            duplicates: vec![],
            callers: vec![
                CallerInfo {
                    name: "caller_one".to_string(),
                    hash: "hash1".to_string(),
                    risk: "low".to_string(),
                },
                CallerInfo {
                    name: "caller_two".to_string(),
                    hash: "hash2".to_string(),
                    risk: "high".to_string(),
                },
            ],
            high_risk_callers: vec!["caller_two".to_string()],
            suggestions: vec![],
        };

        let output = format_validation_result(&result);
        assert!(output.contains("direct: 2"));
        assert!(output.contains("caller_one"));
        assert!(output.contains("caller_two"));
        assert!(output.contains("high_risk_callers:"));
    }

    // ========================================================================
    // Batch Validation Formatting Tests
    // ========================================================================

    #[test]
    fn test_format_batch_validation_results_empty() {
        let results: Vec<SymbolValidationResult> = vec![];
        let output = format_batch_validation_results(&results, "test_module");

        assert!(output.contains("_type: batch_validation_results"));
        assert!(output.contains("context: test_module"));
        assert!(output.contains("total_symbols: 0"));
    }

    #[test]
    fn test_format_batch_validation_results_with_issues() {
        let results = vec![
            SymbolValidationResult {
                symbol: "complex_fn".to_string(),
                file: "src/complex.rs".to_string(),
                lines: "1-50".to_string(),
                kind: "fn".to_string(),
                hash: "hash1".to_string(),
                cognitive_complexity: 20,
                max_nesting: 6,
                risk: "high".to_string(),
                complexity_concerns: vec![],
                duplicates: vec![],
                callers: vec![],
                high_risk_callers: vec![],
                suggestions: vec![],
            },
            SymbolValidationResult {
                symbol: "simple_fn".to_string(),
                file: "src/simple.rs".to_string(),
                lines: "1-10".to_string(),
                kind: "fn".to_string(),
                hash: "hash2".to_string(),
                cognitive_complexity: 3,
                max_nesting: 1,
                risk: "low".to_string(),
                complexity_concerns: vec![],
                duplicates: vec![],
                callers: vec![],
                high_risk_callers: vec![],
                suggestions: vec![],
            },
        ];

        let output = format_batch_validation_results(&results, "my_module");
        assert!(output.contains("total_symbols: 2"));
        assert!(output.contains("high_complexity: 1"));
        assert!(output.contains("deep_nesting: 1"));
        assert!(output.contains("complex_fn"));
    }

    // ========================================================================
    // StalenessInfo Default Value Tests
    // ========================================================================

    #[test]
    fn test_staleness_info_struct() {
        let info = StalenessInfo {
            is_stale: true,
            age_seconds: 3600,
            modified_files: vec!["file1.rs".to_string()],
            files_checked: 10,
        };
        assert!(info.is_stale);
        assert_eq!(info.age_seconds, 3600);
        assert_eq!(info.modified_files.len(), 1);
        assert_eq!(info.files_checked, 10);
    }

    #[test]
    fn test_index_generation_result_struct() {
        let result = IndexGenerationResult {
            duration_ms: 500,
            files_analyzed: 100,
            modules_written: 10,
            symbols_written: 500,
            compression_pct: 75.5,
        };
        assert_eq!(result.duration_ms, 500);
        assert_eq!(result.files_analyzed, 100);
        assert_eq!(result.compression_pct, 75.5);
    }

    #[test]
    fn test_partial_reindex_result_struct() {
        let result = PartialReindexResult {
            files_reindexed: 5,
            modules_updated: 2,
            duration_ms: 100,
        };
        assert_eq!(result.files_reindexed, 5);
        assert_eq!(result.modules_updated, 2);
        assert_eq!(result.duration_ms, 100);
    }
}
