//! Auto-indexing for the socket server daemon
//!
//! Provides functions to index a directory on-demand when no cache exists.
//! This is used when a client connects to a repo that hasn't been indexed yet.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::CacheDir;
use crate::extract::extract;
use crate::lang::Lang;
use crate::schema::SemanticSummary;
use crate::search::is_test_file;
use crate::shard::ShardWriter;

/// Options for indexing
#[derive(Debug, Clone)]
pub struct IndexOptions {
    /// Maximum directory depth to traverse
    pub max_depth: usize,
    /// Include test files in the index
    pub include_tests: bool,
    /// File extensions to include (empty = all supported)
    pub extensions: Vec<String>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            max_depth: 10,
            include_tests: false,
            extensions: Vec::new(),
        }
    }
}

/// Result of an indexing operation
#[derive(Debug)]
pub struct IndexResult {
    pub files_analyzed: usize,
    pub symbols_written: usize,
    pub modules_written: usize,
    pub cache_path: PathBuf,
}

/// Index a directory and write to the provided cache
///
/// This is the main entry point for auto-indexing. It:
/// 1. Collects all supported source files
/// 2. Analyzes each file to extract semantic summaries
/// 3. Writes the shards to the cache directory
pub fn index_directory(dir_path: &Path, cache: CacheDir, options: &IndexOptions) -> anyhow::Result<IndexResult> {
    tracing::info!("Starting index for {:?} -> cache {}", dir_path, cache.repo_hash);

    // Create shard writer with the provided cache
    let mut shard_writer = ShardWriter::with_cache(cache.clone())?;

    // Collect files to analyze
    let files = collect_source_files(dir_path, options);
    tracing::info!("Found {} files to analyze", files.len());

    if files.is_empty() {
        tracing::warn!("No files found to index in {:?}", dir_path);
        return Ok(IndexResult {
            files_analyzed: 0,
            symbols_written: 0,
            modules_written: 0,
            cache_path: cache.root.clone(),
        });
    }

    // Analyze files and collect summaries
    let mut summaries: Vec<SemanticSummary> = Vec::new();
    let total = files.len();

    for (idx, file_path) in files.iter().enumerate() {
        if (idx + 1) % 50 == 0 || idx + 1 == total {
            tracing::debug!("Processing: {}/{}", idx + 1, total);
        }

        // Try to detect language
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Read and analyze file
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("Skipping {}: {}", file_path.display(), e);
                continue;
            }
        };

        // Parse and extract
        let summary = match parse_and_extract(file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("Failed to analyze {}: {}", file_path.display(), e);
                continue;
            }
        };

        summaries.push(summary);
    }

    let files_analyzed = summaries.len();
    tracing::info!("Analyzed {} files", files_analyzed);

    // Add summaries to shard writer
    shard_writer.add_summaries(summaries);

    // Write all shards
    let dir_str = dir_path.display().to_string();
    let stats = shard_writer.write_all(&dir_str)?;

    tracing::info!(
        "Index complete: {} symbols, {} modules written to {}",
        stats.symbols_written,
        stats.modules_written,
        cache.root.display()
    );

    Ok(IndexResult {
        files_analyzed,
        symbols_written: stats.symbols_written,
        modules_written: stats.modules_written,
        cache_path: cache.root,
    })
}

/// Check if a cache needs indexing (no symbol index exists)
pub fn needs_indexing(cache: &CacheDir) -> bool {
    !cache.has_symbol_index()
}

/// Collect all supported source files from a directory
fn collect_source_files(dir: &Path, options: &IndexOptions) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, options.max_depth, 0, options, &mut files);
    files
}

fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    options: &IndexOptions,
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
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
                || name == ".next"
                || name == "coverage"
                || name == "__pycache__"
                || name == "vendor"
                || name == ".git"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, current_depth + 1, options, files);
        } else if path.is_file() {
            // Check if it's a supported extension
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Check extension filter if provided
                if !options.extensions.is_empty() {
                    if !options.extensions.iter().any(|e| e == ext) {
                        continue;
                    }
                }

                // Check if language is supported
                if Lang::from_extension(ext).is_ok() {
                    // Skip test files unless include_tests is set
                    if !options.include_tests {
                        if let Some(path_str) = path.to_str() {
                            if is_test_file(path_str) {
                                continue;
                            }
                        }
                    }
                    files.push(path);
                }
            }
        }
    }
}

/// Parse a file and extract semantic summary
fn parse_and_extract(file_path: &Path, source: &str, lang: Lang) -> anyhow::Result<SemanticSummary> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang.tree_sitter_language())?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse {}", file_path.display()))?;

    let summary = extract(file_path, source, &tree, lang)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_collect_files_basic() {
        let dir = tempdir().unwrap();
        // Note: Don't use "test.ts" - it matches test file patterns and gets filtered
        let file_path = dir.path().join("index.ts");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "export function foo() {{ return 1; }}").unwrap();

        let options = IndexOptions::default();
        let files = collect_source_files(dir.path(), &options);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], file_path);
    }

    #[test]
    fn test_collect_files_skips_node_modules() {
        let dir = tempdir().unwrap();

        // Create node_modules directory with a file
        let nm_dir = dir.path().join("node_modules");
        fs::create_dir(&nm_dir).unwrap();
        let nm_file = nm_dir.join("package.ts");
        let mut file = File::create(&nm_file).unwrap();
        writeln!(file, "export const x = 1;").unwrap();

        // Create a regular source file
        let src_file = dir.path().join("src.ts");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "export const y = 2;").unwrap();

        let options = IndexOptions::default();
        let files = collect_source_files(dir.path(), &options);

        // Should only find src.ts, not the node_modules file
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], src_file);
    }

    #[test]
    fn test_needs_indexing() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_worktree(dir.path()).unwrap();

        // Fresh cache should need indexing
        assert!(needs_indexing(&cache));
    }
}
