//! Parallel index generation with progress reporting
//!
//! This module provides the core parallel file analysis functionality,
//! combining Rayon's parallel iteration with optional progress reporting.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::parsing::parse_and_extract;
use crate::{Lang, SemanticSummary};

/// Progress callback type for index generation.
///
/// The callback receives (current_count, total_count) for progress reporting.
/// Named `IndexingProgressCallback` to avoid conflict with `sqlite_export::ProgressCallback`.
pub type IndexingProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

/// Result of parallel index generation.
#[derive(Debug, Clone)]
pub struct IndexGenerationResult {
    /// Successfully parsed semantic summaries
    pub summaries: Vec<SemanticSummary>,
    /// Total bytes of source code processed
    pub total_bytes: usize,
    /// Number of files that failed to process
    pub errors: usize,
}

/// Analyze files in parallel with optional progress reporting.
///
/// This function processes files using Rayon's parallel iterator, providing
/// significant speedup on multi-core systems. It combines the best of both
/// the CLI (progress reporting, error counting) and MCP (parallel processing)
/// implementations.
///
/// # Arguments
///
/// * `files` - Slice of file paths to analyze
/// * `progress` - Optional callback for progress updates (called every 50 files)
/// * `verbose` - If true, print errors for files that fail to process
///
/// # Returns
///
/// An `IndexGenerationResult` containing:
/// - `summaries`: Successfully parsed semantic summaries
/// - `total_bytes`: Total bytes of source code processed
/// - `errors`: Number of files that failed to process
///
/// # Example
///
/// ```ignore
/// use semfora_engine::indexing::{collect_files, analyze_files_parallel, IndexingProgressCallback};
///
/// let files = collect_files(&repo_dir, 10, &[]);
///
/// // With progress reporting
/// let progress: IndexingProgressCallback = Box::new(|current, total| {
///     eprintln!("Progress: {}/{} ({:.0}%)", current, total,
///         (current as f64 / total as f64) * 100.0);
/// });
///
/// let result = analyze_files_parallel(&files, Some(progress), true);
/// println!("Processed {} files with {} errors", result.summaries.len(), result.errors);
/// ```
pub fn analyze_files_parallel(
    files: &[PathBuf],
    progress: Option<IndexingProgressCallback>,
    verbose: bool,
) -> IndexGenerationResult {
    let total = files.len();
    let processed = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);
    let total_bytes = AtomicUsize::new(0);

    let summaries: Vec<SemanticSummary> = files
        .par_iter()
        .filter_map(|file_path| {
            let current = processed.fetch_add(1, Ordering::Relaxed);

            // Progress callback (every 50 files to avoid too much overhead)
            if let Some(ref cb) = progress {
                if current % 50 == 0 {
                    cb(current, total);
                }
            }

            // Determine language from file extension
            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(e) => {
                    if verbose {
                        eprintln!("Skipping {}: {}", file_path.display(), e);
                    }
                    return None;
                }
            };

            // Read file contents
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    if verbose {
                        eprintln!("Error reading {}: {}", file_path.display(), e);
                    }
                    return None;
                }
            };

            total_bytes.fetch_add(source.len(), Ordering::Relaxed);

            // Parse and extract semantic summary
            match parse_and_extract(file_path, &source, lang) {
                Ok(summary) => Some(summary),
                Err(e) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    if verbose {
                        eprintln!("Error parsing {}: {}", file_path.display(), e);
                    }
                    None
                }
            }
        })
        .collect();

    // Final progress report
    if let Some(ref cb) = progress {
        cb(total, total);
    }

    IndexGenerationResult {
        summaries,
        total_bytes: total_bytes.load(Ordering::Relaxed),
        errors: errors.load(Ordering::Relaxed),
    }
}

/// Backward-compatible function that returns (summaries, total_bytes).
///
/// This matches the signature of the original `analyze_files_with_stats`
/// in helpers.rs for easier migration.
pub fn analyze_files_with_stats(files: &[PathBuf]) -> (Vec<SemanticSummary>, usize) {
    let result = analyze_files_parallel(files, None, false);
    (result.summaries, result.total_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_analyze_empty_list() {
        let files: Vec<PathBuf> = vec![];
        let result = analyze_files_parallel(&files, None, false);

        assert_eq!(result.summaries.len(), 0);
        assert_eq!(result.total_bytes, 0);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn test_analyze_with_progress() {
        let files: Vec<PathBuf> = vec![];
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let progress: IndexingProgressCallback = Box::new(move |_, _| {
            call_count_clone.fetch_add(1, Ordering::Relaxed);
        });

        let result = analyze_files_parallel(&files, Some(progress), false);

        // Should have at least called for final report
        assert!(call_count.load(Ordering::Relaxed) >= 1);
        assert_eq!(result.summaries.len(), 0);
    }

    #[test]
    fn test_analyze_files_with_stats_compat() {
        let files: Vec<PathBuf> = vec![];
        let (summaries, bytes) = analyze_files_with_stats(&files);

        assert_eq!(summaries.len(), 0);
        assert_eq!(bytes, 0);
    }
}
