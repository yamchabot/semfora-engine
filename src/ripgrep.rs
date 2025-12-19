//! Ripgrep integration for fallback search
//!
//! This module provides ripgrep-based search when no semantic index exists,
//! enabling immediate search without waiting for full indexing.
//!
//! # Use Cases
//!
//! 1. **No index exists** → ripgrep search instead of error
//! 2. **Working overlay** → search only uncommitted files (real-time)
//! 3. **Cross-reference** → verify symbol actually exists in source
//!
//! # Features
//!
//! - Respects `.gitignore` automatically via the `ignore` crate
//! - Adjacent block merging to reduce fragmentation
//! - Configurable merge threshold
//! - Line number and context support
//!
//! # Example
//!
//! ```ignore
//! use semfora_engine::ripgrep::{RipgrepSearcher, SearchOptions};
//!
//! let searcher = RipgrepSearcher::new();
//! let options = SearchOptions::new("fn main")
//!     .with_limit(100)
//!     .with_merge_threshold(3);
//!
//! let matches = searcher.search("/path/to/repo", &options)?;
//! for m in matches {
//!     println!("{}:{}: {}", m.file, m.line, m.content);
//! }
//! ```

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::error::{McpDiffError, Result};

// ============================================================================
// Core Types
// ============================================================================

/// A single search match with file location and content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Absolute or relative file path
    pub file: PathBuf,

    /// Line number (1-indexed)
    pub line: u64,

    /// Column number (1-indexed, byte offset)
    pub column: u64,

    /// Content of the matching line
    pub content: String,

    /// Context lines before the match (if requested)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<String>,

    /// Context lines after the match (if requested)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<String>,
}

impl SearchMatch {
    /// Create a new search match
    pub fn new(file: PathBuf, line: u64, column: u64, content: String) -> Self {
        Self {
            file,
            line,
            column,
            content,
            context_before: Vec::new(),
            context_after: Vec::new(),
        }
    }

    /// Add context lines before the match
    pub fn with_context_before(mut self, lines: Vec<String>) -> Self {
        self.context_before = lines;
        self
    }

    /// Add context lines after the match
    pub fn with_context_after(mut self, lines: Vec<String>) -> Self {
        self.context_after = lines;
        self
    }
}

/// A merged block of adjacent matches
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedBlock {
    /// File path
    pub file: PathBuf,

    /// Start line of the block (1-indexed)
    pub start_line: u64,

    /// End line of the block (1-indexed, inclusive)
    pub end_line: u64,

    /// All lines in the block (including non-matching context)
    pub lines: Vec<BlockLine>,

    /// Number of actual matches in this block
    pub match_count: usize,
}

/// A line within a merged block
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockLine {
    /// Line number (1-indexed)
    pub line: u64,

    /// Content of the line
    pub content: String,

    /// Whether this line contains a match
    pub is_match: bool,
}

/// Search options for configuring ripgrep behavior
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// The search pattern (regex)
    pub pattern: String,

    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,

    /// Maximum number of matches to return
    pub limit: Option<usize>,

    /// Merge threshold: merge adjacent blocks within N lines
    pub merge_threshold: usize,

    /// Case-insensitive search
    pub case_insensitive: bool,

    /// File type filters (e.g., "rs", "ts")
    pub file_types: Vec<String>,
}

impl SearchOptions {
    /// Create new search options with a pattern
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            respect_gitignore: true,
            limit: None,
            merge_threshold: 3, // Default: merge blocks within 3 lines
            case_insensitive: false,
            file_types: Vec::new(),
        }
    }

    /// Set whether to respect .gitignore
    pub fn with_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Set maximum number of matches
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set merge threshold for adjacent blocks
    pub fn with_merge_threshold(mut self, threshold: usize) -> Self {
        self.merge_threshold = threshold;
        self
    }

    /// Set case-insensitive search
    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = true;
        self
    }

    /// Set file type filters
    pub fn with_file_types(mut self, types: Vec<String>) -> Self {
        self.file_types = types;
        self
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self::new("")
    }
}

// ============================================================================
// RipgrepSearcher
// ============================================================================

/// Ripgrep-based file searcher
///
/// Provides search functionality with .gitignore support and block merging.
#[derive(Debug, Clone, Default)]
pub struct RipgrepSearcher;

impl RipgrepSearcher {
    /// Create a new ripgrep searcher
    pub fn new() -> Self {
        Self
    }

    /// Search for a pattern in the given directory
    ///
    /// Returns a list of matches, respecting .gitignore and other options.
    pub fn search(&self, root: &Path, options: &SearchOptions) -> Result<Vec<SearchMatch>> {
        // Build regex matcher
        let matcher = self.build_matcher(options)?;

        // Collect matches
        let matches = Arc::new(Mutex::new(Vec::new()));
        let limit = options.limit;

        // Build file walker
        let walker = self.build_walker(root, options)?;

        // Search each file
        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Skip directories
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
                continue;
            }

            let path = entry.path();

            // Check file type filters
            if !options.file_types.is_empty() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if !options.file_types.iter().any(|t| t == ext) {
                        continue;
                    }
                } else {
                    continue; // No extension, skip
                }
            }

            // Check limit
            if let Some(lim) = limit {
                let current_count = matches.lock().unwrap().len();
                if current_count >= lim {
                    break;
                }
            }

            // Search this file
            if let Err(_) = self.search_file(path, &matcher, &matches, limit) {
                // Skip files that can't be searched (binary, permission issues, etc.)
                continue;
            }
        }

        let result = Arc::try_unwrap(matches)
            .map(|m| m.into_inner().unwrap())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());

        Ok(result)
    }

    /// Search and merge adjacent blocks
    ///
    /// Adjacent matches within `merge_threshold` lines are combined into blocks.
    pub fn search_merged(&self, root: &Path, options: &SearchOptions) -> Result<Vec<MergedBlock>> {
        let matches = self.search(root, options)?;
        Ok(self.merge_matches(matches, options.merge_threshold))
    }

    /// Search specific files only
    pub fn search_files(
        &self,
        files: &[PathBuf],
        options: &SearchOptions,
    ) -> Result<Vec<SearchMatch>> {
        let matcher = self.build_matcher(options)?;
        let matches = Arc::new(Mutex::new(Vec::new()));
        let limit = options.limit;

        for path in files {
            if !path.exists() {
                continue;
            }

            // Check limit
            if let Some(lim) = limit {
                let current_count = matches.lock().unwrap().len();
                if current_count >= lim {
                    break;
                }
            }

            if let Err(_) = self.search_file(path, &matcher, &matches, limit) {
                continue;
            }
        }

        let result = Arc::try_unwrap(matches)
            .map(|m| m.into_inner().unwrap())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());

        Ok(result)
    }

    /// Build a regex matcher from options
    fn build_matcher(&self, options: &SearchOptions) -> Result<RegexMatcher> {
        let mut builder = grep_regex::RegexMatcherBuilder::new();
        builder.case_insensitive(options.case_insensitive);

        builder
            .build(&options.pattern)
            .map_err(|e| McpDiffError::QueryError {
                message: format!("Invalid regex pattern: {}", e),
            })
    }

    /// Build a file walker from options
    fn build_walker(&self, root: &Path, options: &SearchOptions) -> Result<ignore::Walk> {
        let mut builder = WalkBuilder::new(root);

        // Respect .gitignore
        builder.git_ignore(options.respect_gitignore);
        builder.git_global(options.respect_gitignore);
        builder.git_exclude(options.respect_gitignore);

        // Do not follow symlinks
        builder.follow_links(false);

        // Include hidden files in search (hidden=true means "process hidden files")
        builder.hidden(true);

        Ok(builder.build())
    }

    /// Search a single file
    fn search_file(
        &self,
        path: &Path,
        matcher: &RegexMatcher,
        matches: &Arc<Mutex<Vec<SearchMatch>>>,
        limit: Option<usize>,
    ) -> Result<()> {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build();

        let path_buf = path.to_path_buf();

        searcher
            .search_path(
                matcher,
                path,
                UTF8(|line_num, line| {
                    // Check limit
                    if let Some(lim) = limit {
                        let m = matches.lock().unwrap();
                        if m.len() >= lim {
                            return Ok(false); // Stop searching
                        }
                    }

                    // Find column (byte offset of first match)
                    let column = matcher
                        .find(line.as_bytes())
                        .ok()
                        .flatten()
                        .map(|m| m.start() as u64 + 1)
                        .unwrap_or(1);

                    let search_match = SearchMatch::new(
                        path_buf.clone(),
                        line_num,
                        column,
                        line.trim_end().to_string(),
                    );

                    matches.lock().unwrap().push(search_match);
                    Ok(true)
                }),
            )
            .map_err(|e| McpDiffError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

        Ok(())
    }

    /// Merge adjacent matches into blocks
    ///
    /// Matches within `threshold` lines of each other are merged into a single block.
    pub fn merge_matches(&self, matches: Vec<SearchMatch>, threshold: usize) -> Vec<MergedBlock> {
        if matches.is_empty() {
            return Vec::new();
        }

        // Group matches by file
        let mut by_file: HashMap<PathBuf, Vec<SearchMatch>> = HashMap::new();
        for m in matches {
            by_file.entry(m.file.clone()).or_default().push(m);
        }

        let mut blocks = Vec::new();

        for (file, mut file_matches) in by_file {
            // Sort by line number
            file_matches.sort_by_key(|m| m.line);

            let mut current_block: Option<MergedBlock> = None;

            for m in file_matches {
                match &mut current_block {
                    Some(block) => {
                        // Check if this match is within threshold of the current block
                        if m.line <= block.end_line + threshold as u64 + 1 {
                            // Extend the block
                            // Fill in any gap lines (non-matching)
                            for line_num in (block.end_line + 1)..m.line {
                                block.lines.push(BlockLine {
                                    line: line_num,
                                    content: String::new(), // Context would go here if we had it
                                    is_match: false,
                                });
                            }
                            // Add the match
                            block.lines.push(BlockLine {
                                line: m.line,
                                content: m.content,
                                is_match: true,
                            });
                            block.end_line = m.line;
                            block.match_count += 1;
                        } else {
                            // Start a new block
                            blocks.push(current_block.take().unwrap());
                            current_block = Some(MergedBlock {
                                file: file.clone(),
                                start_line: m.line,
                                end_line: m.line,
                                lines: vec![BlockLine {
                                    line: m.line,
                                    content: m.content,
                                    is_match: true,
                                }],
                                match_count: 1,
                            });
                        }
                    }
                    None => {
                        // First match - start a new block
                        current_block = Some(MergedBlock {
                            file: file.clone(),
                            start_line: m.line,
                            end_line: m.line,
                            lines: vec![BlockLine {
                                line: m.line,
                                content: m.content,
                                is_match: true,
                            }],
                            match_count: 1,
                        });
                    }
                }
            }

            // Don't forget the last block
            if let Some(block) = current_block {
                blocks.push(block);
            }
        }

        blocks
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test directory with files
    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Initialize as a git repository so .gitignore is respected
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to init git repo");

        // Create some Rust files
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/main.rs"),
            r#"fn main() {
    println!("Hello, world!");
}

fn helper() {
    // This is a helper function
    let x = 42;
}
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("src/lib.rs"),
            r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }
}
"#,
        )
        .unwrap();

        // Create a .gitignore
        fs::write(
            dir.path().join(".gitignore"),
            r#"target/
*.log
ignored_file.rs
"#,
        )
        .unwrap();

        // Create an ignored directory with a file
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::write(
            dir.path().join("target/ignored.rs"),
            "fn ignored() { println!(\"This should be ignored\"); }",
        )
        .unwrap();

        // Create an explicitly ignored file
        fs::write(
            dir.path().join("src/ignored_file.rs"),
            "fn should_be_ignored() {}",
        )
        .unwrap();

        dir
    }

    // ========================================================================
    // TDD Tests from SEM-46 Requirements
    // ========================================================================

    #[test]
    fn test_basic_search_finds_match() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("fn main");

        let matches = searcher.search(dir.path(), &options).unwrap();

        assert!(!matches.is_empty(), "Should find at least one match");
        assert!(
            matches.iter().any(|m| m.content.contains("fn main")),
            "Should find 'fn main'"
        );
    }

    #[test]
    fn test_search_respects_gitignore() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("ignored");

        let matches = searcher.search(dir.path(), &options).unwrap();

        // Should NOT find matches in target/ or ignored_file.rs
        for m in &matches {
            let path_str = m.file.to_string_lossy();
            assert!(
                !path_str.contains("target"),
                "Should not search in target/: {:?}",
                m.file
            );
            assert!(
                !path_str.contains("ignored_file"),
                "Should not search ignored_file.rs: {:?}",
                m.file
            );
        }
    }

    #[test]
    fn test_search_returns_line_numbers() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("fn main");

        let matches = searcher.search(dir.path(), &options).unwrap();

        assert!(!matches.is_empty());
        for m in &matches {
            assert!(m.line > 0, "Line numbers should be 1-indexed");
            assert!(m.column > 0, "Column numbers should be 1-indexed");
        }

        // Check that we find fn main on line 1
        let main_match = matches.iter().find(|m| m.content.contains("fn main"));
        assert!(main_match.is_some());
        assert_eq!(main_match.unwrap().line, 1, "fn main should be on line 1");
    }

    #[test]
    fn test_search_limit_works() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();

        // Search for "fn" which should match multiple times
        let options = SearchOptions::new("fn").with_limit(2);

        let matches = searcher.search(dir.path(), &options).unwrap();

        assert!(
            matches.len() <= 2,
            "Should respect limit of 2, got {}",
            matches.len()
        );
    }

    #[test]
    fn test_search_specific_files() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();

        // Only search main.rs
        let files = vec![dir.path().join("src/main.rs")];
        let options = SearchOptions::new("fn");

        let matches = searcher.search_files(&files, &options).unwrap();

        // All matches should be from main.rs
        for m in &matches {
            assert!(
                m.file.ends_with("main.rs"),
                "All matches should be from main.rs, got {:?}",
                m.file
            );
        }
    }

    #[test]
    fn test_adjacent_block_merging() {
        let dir = TempDir::new().unwrap();

        // Create a file with matches close together
        fs::write(
            dir.path().join("test.rs"),
            r#"fn one() {}
fn two() {}
fn three() {}
// gap
// gap
// gap
// gap
// gap
fn four() {}
fn five() {}
"#,
        )
        .unwrap();

        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("fn").with_merge_threshold(2);

        let blocks = searcher.search_merged(dir.path(), &options).unwrap();

        // Should have 2 blocks: one->three (lines 1-3) and four->five (lines 9-10)
        // because the gap is 5 lines which is > threshold of 2
        assert_eq!(
            blocks.len(),
            2,
            "Should merge into 2 blocks, got {}",
            blocks.len()
        );

        // First block should have 3 matches (one, two, three)
        let first_block = &blocks[0];
        assert_eq!(
            first_block.match_count, 3,
            "First block should have 3 matches"
        );

        // Second block should have 2 matches (four, five)
        let second_block = &blocks[1];
        assert_eq!(
            second_block.match_count, 2,
            "Second block should have 2 matches"
        );
    }

    #[test]
    fn test_merge_threshold_configurable() {
        let dir = TempDir::new().unwrap();

        // Create a file with matches 3 lines apart
        fs::write(
            dir.path().join("test.rs"),
            r#"fn alpha() {}
// line 2
// line 3
fn beta() {}
"#,
        )
        .unwrap();

        let searcher = RipgrepSearcher::new();

        // With threshold=1, they should NOT merge (3 lines gap > 1)
        let options_small = SearchOptions::new("fn").with_merge_threshold(1);
        let blocks_small = searcher.search_merged(dir.path(), &options_small).unwrap();
        assert_eq!(
            blocks_small.len(),
            2,
            "With threshold=1, should have 2 separate blocks"
        );

        // With threshold=3, they SHOULD merge (3 lines gap <= 3)
        let options_large = SearchOptions::new("fn").with_merge_threshold(3);
        let blocks_large = searcher.search_merged(dir.path(), &options_large).unwrap();
        assert_eq!(
            blocks_large.len(),
            1,
            "With threshold=3, should merge into 1 block"
        );
    }

    // ========================================================================
    // Additional Tests for Edge Cases
    // ========================================================================

    #[test]
    fn test_search_empty_results() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("nonexistent_pattern_xyz123");

        let matches = searcher.search(dir.path(), &options).unwrap();

        assert!(matches.is_empty(), "Should return empty for no matches");
    }

    #[test]
    fn test_search_case_insensitive() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();

        // Case-sensitive should not find "FN MAIN"
        let options_sensitive = SearchOptions::new("FN MAIN");
        let matches_sensitive = searcher.search(dir.path(), &options_sensitive).unwrap();
        assert!(
            matches_sensitive.is_empty(),
            "Case-sensitive should not find uppercase"
        );

        // Case-insensitive should find it
        let options_insensitive = SearchOptions::new("FN MAIN").case_insensitive();
        let matches_insensitive = searcher.search(dir.path(), &options_insensitive).unwrap();
        assert!(
            !matches_insensitive.is_empty(),
            "Case-insensitive should find uppercase"
        );
    }

    #[test]
    fn test_search_with_file_type_filter() {
        let dir = TempDir::new().unwrap();

        // Create files with different extensions
        fs::write(dir.path().join("test.rs"), "fn rust_func() {}").unwrap();
        fs::write(dir.path().join("test.ts"), "function ts_func() {}").unwrap();
        fs::write(dir.path().join("test.py"), "def python_func(): pass").unwrap();

        let searcher = RipgrepSearcher::new();

        // Search only .rs files
        let options = SearchOptions::new("func").with_file_types(vec!["rs".to_string()]);
        let matches = searcher.search(dir.path(), &options).unwrap();

        assert_eq!(matches.len(), 1, "Should only find match in .rs file");
        assert!(matches[0].file.to_string_lossy().ends_with(".rs"));
    }

    #[test]
    fn test_search_respects_gitignore_disabled() {
        let dir = setup_test_dir();
        let searcher = RipgrepSearcher::new();

        // With gitignore disabled, should find matches in target/
        let options = SearchOptions::new("ignored").with_gitignore(false);
        let matches = searcher.search(dir.path(), &options).unwrap();

        // Should find matches in target/ now
        let has_target_match = matches
            .iter()
            .any(|m| m.file.to_string_lossy().contains("target"));
        assert!(
            has_target_match,
            "Should find matches in target/ when gitignore disabled"
        );
    }

    #[test]
    fn test_merge_empty_matches() {
        let searcher = RipgrepSearcher::new();
        let matches: Vec<SearchMatch> = Vec::new();

        let blocks = searcher.merge_matches(matches, 3);

        assert!(
            blocks.is_empty(),
            "Empty matches should produce empty blocks"
        );
    }

    #[test]
    fn test_merge_single_match() {
        let searcher = RipgrepSearcher::new();
        let matches = vec![SearchMatch::new(
            PathBuf::from("test.rs"),
            10,
            1,
            "fn single() {}".to_string(),
        )];

        let blocks = searcher.merge_matches(matches, 3);

        assert_eq!(blocks.len(), 1, "Single match should produce one block");
        assert_eq!(blocks[0].match_count, 1);
        assert_eq!(blocks[0].start_line, 10);
        assert_eq!(blocks[0].end_line, 10);
    }

    #[test]
    fn test_merge_multiple_files() {
        let searcher = RipgrepSearcher::new();
        let matches = vec![
            SearchMatch::new(PathBuf::from("a.rs"), 1, 1, "fn a1() {}".to_string()),
            SearchMatch::new(PathBuf::from("a.rs"), 2, 1, "fn a2() {}".to_string()),
            SearchMatch::new(PathBuf::from("b.rs"), 1, 1, "fn b1() {}".to_string()),
            SearchMatch::new(PathBuf::from("b.rs"), 2, 1, "fn b2() {}".to_string()),
        ];

        let blocks = searcher.merge_matches(matches, 5);

        // Should have 2 blocks (one per file)
        assert_eq!(blocks.len(), 2, "Should have one block per file");

        for block in &blocks {
            assert_eq!(block.match_count, 2, "Each block should have 2 matches");
        }
    }

    #[test]
    fn test_invalid_regex_pattern() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.rs"), "fn test() {}").unwrap();

        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("[invalid regex");

        let result = searcher.search(dir.path(), &options);

        assert!(result.is_err(), "Invalid regex should return error");
        if let Err(McpDiffError::QueryError { message }) = result {
            assert!(message.contains("regex"), "Error should mention regex");
        } else {
            panic!("Expected QueryError");
        }
    }

    #[test]
    fn test_search_nonexistent_directory() {
        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("test");

        let result = searcher.search(Path::new("/nonexistent/path/xyz"), &options);

        // Should return empty results (walker yields nothing) not an error
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_search_match_column_position() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.rs"), "    fn indented() {}").unwrap();

        let searcher = RipgrepSearcher::new();
        let options = SearchOptions::new("fn");

        let matches = searcher.search(dir.path(), &options).unwrap();

        assert_eq!(matches.len(), 1);
        // "fn" starts at column 5 (after 4 spaces)
        assert_eq!(
            matches[0].column, 5,
            "Column should account for leading spaces"
        );
    }
}
