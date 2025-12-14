//! Helper functions for the MCP server
//!
//! This module contains shared utility functions used across MCP tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{extract, Lang, McpDiffError, SemanticSummary, CacheDir, ShardWriter};

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
                format!("{}...", &files_str[..100])
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
    let mut summaries = Vec::new();
    let mut total_bytes = 0usize;

    for file_path in files {
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        total_bytes += source.len();

        if let Ok(summary) = parse_and_extract(file_path, &source, lang) {
            summaries.push(summary);
        }
    }

    (summaries, total_bytes)
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
