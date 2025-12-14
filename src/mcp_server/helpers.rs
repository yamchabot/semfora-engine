//! Helper functions for the MCP server
//!

use std::fs;
use crate::utils::truncate_to_char_boundary;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::collections::{HashMap, HashSet};

use crate::{extract, extract_module_name, Lang, McpDiffError, SemanticSummary, CacheDir, ShardWriter};

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

/// Check if the cache is potentially stale
/// Compares repo_overview.toon mtime against source files in the repo
pub fn check_cache_staleness(cache: &CacheDir) -> Option<String> {
    let overview_path = cache.repo_overview_path();
    let overview_mtime = match fs::metadata(&overview_path) {
        Ok(m) => m.modified().ok(),
        Err(_) => return None,
    };

    let overview_time = match overview_mtime {
        Some(t) => t,
        None => return None,
    };

    // Scan source files in repo_root for any newer than cache
    let mut stale_files = Vec::new();
    let max_to_check = 100; // Limit for performance

    if let Ok(entries) = collect_source_files_for_staleness(&cache.repo_root, max_to_check) {
        for (path, mtime) in entries {
            if mtime > overview_time {
                stale_files.push(path);
                if stale_files.len() >= 5 {
                    break; // Don't list too many
                }
            }
        }
    }

    if stale_files.is_empty() {
        None
    } else {
        let files_str = stale_files.join(", ");
        Some(format!(
            "⚠️ Index may be stale. {} file(s) modified since last indexing: {}. Run generate_index to refresh.",
            stale_files.len(),
            if files_str.len() > 100 {
                format!("{}...", truncate_to_char_boundary(&files_str, 100))
            } else {
                files_str
            }
        ))
    }
}

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
        if should_skip_path(&path) {
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
// File Collection
// ============================================================================

/// Recursively collect supported files from a directory
pub fn collect_files(dir: &Path, max_depth: usize, extensions: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, max_depth, 0, extensions, &mut files);
    files
}

fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    extensions: &[String],
    files: &mut Vec<PathBuf>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories and common non-source directories
        if should_skip_path(&path) {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, current_depth + 1, extensions, files);
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Check extension filter if provided
                if !extensions.is_empty() && !extensions.iter().any(|e| e == ext) {
                    continue;
                }

                // Check if language is supported
                if Lang::from_extension(ext).is_ok() {
                    files.push(path);
                }
            }
        }
    }
}

/// Check if a path should be skipped during file collection
fn should_skip_path(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "dist"
            || name == "build"
            || name == ".next"
            || name == "coverage"
            || name == "__pycache__"
            || name == "vendor"
    } else {
        false
    }
}

// ============================================================================
// Parsing
// ============================================================================

/// Parse source and extract semantic summary
pub fn parse_and_extract(
    file_path: &Path,
    source: &str,
    lang: Lang,
) -> Result<SemanticSummary, McpDiffError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language: {:?}", e),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: "Failed to parse file".to_string(),
        })?;

    extract(file_path, source, &tree, lang)
}

// ============================================================================
// Index Generation
// ============================================================================

/// Analyze a collection of files and return their semantic summaries with total bytes
pub fn analyze_files_with_stats(files: &[PathBuf]) -> (Vec<SemanticSummary>, usize) {
    let total_bytes_atomic = AtomicUsize::new(0);

    let summaries: Vec<SemanticSummary> = files
        .par_iter()
        .filter_map(|file_path| {
            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(_) => return None,
            };

            total_bytes_atomic.fetch_add(source.len(), Ordering::Relaxed);

            parse_and_extract(file_path, &source, lang).ok()
        })
        .collect();

    (summaries, total_bytes_atomic.load(Ordering::Relaxed))
}

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
    let files = collect_files(dir_path, max_depth, extensions);

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
    let (summaries, total_bytes) = analyze_files_with_stats(&files);

    // Add summaries to shard writer
    shard_writer.add_summaries(summaries.clone());

    // Write all shards
    let dir_str = dir_path.display().to_string();
    let stats = shard_writer.write_all(&dir_str)
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
    let (new_summaries, _) = analyze_files_with_stats(&valid_files);

    // Group new summaries by module
    let mut new_by_module: HashMap<String, Vec<SemanticSummary>> = HashMap::new();
    for summary in &new_summaries {
        let module = extract_module_name(&summary.file);
        new_by_module.entry(module).or_default().push(summary.clone());
    }

    // Track which files were changed (for removing old entries)
    let changed_file_set: HashSet<String> = valid_files
        .iter()
        .filter_map(|p| p.to_str())
        .map(|s| s.to_string())
        .collect();

    // Also track modules that might have files removed (deleted files)
    let deleted_files: Vec<&PathBuf> = changed_files
        .iter()
        .filter(|f| !f.exists())
        .collect();

    for deleted in &deleted_files {
        if let Some(path_str) = deleted.to_str() {
            let module = extract_module_name(path_str);
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
fn update_symbol_shards(
    cache: &CacheDir,
    summaries: &[SemanticSummary],
) -> Result<(), String> {
    use crate::schema::SymbolId;

    for summary in summaries {
        let namespace = SymbolId::namespace_from_path(&summary.file);

        // If we have symbols in the multi-symbol format, use those
        if !summary.symbols.is_empty() {
            for symbol_info in &summary.symbols {
                let symbol_id = symbol_info.to_symbol_id(&namespace);
                let toon = crate::shard::encode_symbol_shard_from_info(summary, symbol_info, &symbol_id);
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
    let cache = CacheDir::for_repo(repo_path)
        .map_err(|e| format!("Failed to access cache: {}", e))?;

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
// Repo Overview Filtering
// ============================================================================

/// Test directory patterns to exclude
const TEST_DIR_PATTERNS: &[&str] = &[
    "tests",
    "__tests__",
    "test-repos",
    "test_",
    "_test",
    "spec",
    "fixtures",
];

/// Check if a module name indicates a test directory
fn is_test_module(name: &str) -> bool {
    let lower = name.to_lowercase();
    TEST_DIR_PATTERNS.iter().any(|pattern| {
        lower == *pattern
            || lower.starts_with(&format!("{}/", pattern))
            || lower.ends_with(&format!("/{}", pattern))
            || lower.contains(&format!("/{}/", pattern))
    })
}

/// Filter repo overview TOON content to limit modules and exclude test dirs
///
/// # Arguments
/// * `content` - The raw TOON content from repo_overview.toon
/// * `max_modules` - Maximum number of modules to include (0 = no limit)
/// * `exclude_test_dirs` - Whether to exclude test directories
///
/// # Returns
/// Filtered TOON content with updated module count
pub fn filter_repo_overview(content: &str, max_modules: usize, exclude_test_dirs: bool) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut output = String::new();
    let mut in_modules = false;
    let mut modules_collected: Vec<&str> = Vec::new();
    let mut total_modules = 0;
    let mut excluded_count = 0;

    for line in &lines {
        // Detect modules section start: "modules[N]{...}:"
        if line.starts_with("modules[") && line.ends_with(':') {
            in_modules = true;
            // Parse original count from "modules[30]..." format
            if let Some(count_str) = line.strip_prefix("modules[").and_then(|s| s.split(']').next()) {
                total_modules = count_str.parse().unwrap_or(0);
            }
            continue;
        }

        if in_modules {
            // Module lines are indented with 2 spaces and contain commas
            if line.starts_with("  ") && line.contains(',') {
                // Parse module name (first field)
                let module_name = line.trim().split(',').next().unwrap_or("");

                // Check if should exclude
                if exclude_test_dirs && is_test_module(module_name) {
                    excluded_count += 1;
                    continue;
                }

                modules_collected.push(line);
            } else {
                // End of modules section - flush collected modules
                in_modules = false;

                // Apply limit
                let final_modules: Vec<&str> = if max_modules > 0 && modules_collected.len() > max_modules {
                    modules_collected.iter().take(max_modules).copied().collect()
                } else {
                    modules_collected.clone()
                };

                // Write modules header with actual count
                let header_parts: Vec<&str> = lines.iter()
                    .find(|l| l.starts_with("modules["))
                    .map(|l| l.split(']').collect::<Vec<_>>())
                    .unwrap_or_default();

                if header_parts.len() >= 2 {
                    // Show filtered count and note total
                    if excluded_count > 0 || (max_modules > 0 && modules_collected.len() > max_modules) {
                        let showing = final_modules.len();
                        output.push_str(&format!(
                            "modules[{}/{}]{}\n",
                            showing,
                            total_modules,
                            header_parts[1]
                        ));
                    } else {
                        output.push_str(&format!("modules[{}]{}\n", final_modules.len(), header_parts[1]));
                    }
                }

                // Write filtered modules
                for module_line in &final_modules {
                    output.push_str(module_line);
                    output.push('\n');
                }

                // Write the current line (which ended the modules section)
                output.push_str(line);
                output.push('\n');

                // Clear for potential future modules sections
                modules_collected.clear();
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    // Handle case where file ends during modules section
    if in_modules && !modules_collected.is_empty() {
        let final_modules: Vec<&str> = if max_modules > 0 && modules_collected.len() > max_modules {
            modules_collected.iter().take(max_modules).copied().collect()
        } else {
            modules_collected.clone()
        };

        for module_line in &final_modules {
            output.push_str(module_line);
            output.push('\n');
        }
    }

    output
}
