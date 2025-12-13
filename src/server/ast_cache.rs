//! AST cache for incremental parsing
//!
//! This module provides caching of parsed ASTs (tree-sitter Trees) to enable
//! incremental parsing when files change. Tree-sitter's incremental parsing
//! can be 10-100x faster than full parsing for small edits.
//!
//! # How Incremental Parsing Works
//!
//! 1. Store the old source code and parsed tree
//! 2. When file changes, compute what bytes changed (InputEdit)
//! 3. Call tree.edit(&edit) to adjust node ranges
//! 4. Parse with parser.parse(new_source, Some(&old_tree))
//! 5. Tree-sitter reuses unchanged subtrees
//!
//! # Performance
//!
//! - Full parse: 5-50ms depending on file size
//! - Incremental parse: <1ms for small edits
//! - Memory: ~2-5KB per cached file

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use parking_lot::RwLock;
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::lang::Lang;

/// A cached file with its source and parsed AST
#[derive(Debug)]
pub struct CachedFile {
    /// The source code content
    pub source: String,
    /// The parsed tree-sitter Tree
    pub tree: Tree,
    /// Language of the file
    pub lang: Lang,
    /// Last time this entry was updated
    pub last_updated: Instant,
}

impl CachedFile {
    /// Create a new cached file entry
    pub fn new(source: String, tree: Tree, lang: Lang) -> Self {
        Self {
            source,
            tree,
            lang,
            last_updated: Instant::now(),
        }
    }
}

/// Cache for parsed ASTs, enabling incremental parsing
pub struct AstCache {
    /// Map from file path to cached AST state
    files: RwLock<HashMap<PathBuf, CachedFile>>,
    /// Parser instance (reused across parses)
    parser: RwLock<Parser>,
}

impl AstCache {
    /// Create a new empty AST cache
    pub fn new() -> Self {
        Self {
            files: RwLock::new(HashMap::new()),
            parser: RwLock::new(Parser::new()),
        }
    }

    /// Get a cached file if it exists
    pub fn get(&self, path: &PathBuf) -> Option<CachedFileRef> {
        let files = self.files.read();
        if files.contains_key(path) {
            // Return a reference that can be used to access the cached data
            Some(CachedFileRef {
                path: path.clone(),
                cache: self,
            })
        } else {
            None
        }
    }

    /// Check if a file is cached
    pub fn contains(&self, path: &PathBuf) -> bool {
        self.files.read().contains_key(path)
    }

    /// Parse a file, using incremental parsing if cached
    ///
    /// Returns the parsed tree and a flag indicating if incremental parsing was used.
    pub fn parse_file(
        &self,
        path: &PathBuf,
        new_source: &str,
        lang: Lang,
    ) -> Result<(Tree, ParseResult), String> {
        let mut parser = self.parser.write();
        parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|e| format!("Failed to set language: {}", e))?;

        // Check if we have a cached version
        let mut files = self.files.write();

        if let Some(cached) = files.get_mut(path) {
            // Same language check
            if cached.lang == lang {
                // Compute edit if sources differ
                if cached.source != new_source {
                    let edit = compute_edit(&cached.source, new_source);

                    // Apply edit to the tree
                    cached.tree.edit(&edit);

                    // Incremental parse
                    let new_tree = parser
                        .parse(new_source, Some(&cached.tree))
                        .ok_or_else(|| "Incremental parse failed".to_string())?;

                    // Get changed ranges for selective re-extraction
                    let changed_ranges = cached.tree.changed_ranges(&new_tree).collect::<Vec<_>>();

                    // Update cache
                    cached.source = new_source.to_string();
                    cached.tree = new_tree.clone();
                    cached.last_updated = Instant::now();

                    return Ok((
                        new_tree,
                        ParseResult::Incremental {
                            changed_ranges,
                            edit,
                        },
                    ));
                } else {
                    // Source unchanged, return cached tree
                    return Ok((cached.tree.clone(), ParseResult::Cached));
                }
            }
        }

        // Full parse (no cache or language mismatch)
        let tree = parser
            .parse(new_source, None)
            .ok_or_else(|| "Full parse failed".to_string())?;

        // Store in cache
        files.insert(
            path.clone(),
            CachedFile::new(new_source.to_string(), tree.clone(), lang),
        );

        Ok((tree, ParseResult::Full))
    }

    /// Remove a file from the cache (e.g., when deleted)
    pub fn remove(&self, path: &PathBuf) -> Option<CachedFile> {
        self.files.write().remove(path)
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        self.files.write().clear();
    }

    /// Get the number of cached files
    pub fn len(&self) -> usize {
        self.files.read().len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.files.read().is_empty()
    }

    /// Get cache statistics
    pub fn stats(&self) -> AstCacheStats {
        let files = self.files.read();
        let total_source_bytes: usize = files.values().map(|f| f.source.len()).sum();

        AstCacheStats {
            file_count: files.len(),
            total_source_bytes,
        }
    }

    /// Evict entries older than the specified duration
    pub fn evict_older_than(&self, max_age: std::time::Duration) {
        let now = Instant::now();
        self.files
            .write()
            .retain(|_, cached| now.duration_since(cached.last_updated) < max_age);
    }
}

impl Default for AstCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference to a cached file (for reading without holding the lock)
pub struct CachedFileRef<'a> {
    path: PathBuf,
    cache: &'a AstCache,
}

impl<'a> CachedFileRef<'a> {
    /// Get the cached source
    pub fn source(&self) -> Option<String> {
        self.cache.files.read().get(&self.path).map(|f| f.source.clone())
    }

    /// Get the cached tree (cloned)
    pub fn tree(&self) -> Option<Tree> {
        self.cache.files.read().get(&self.path).map(|f| f.tree.clone())
    }
}

/// Result of a parse operation
#[derive(Debug)]
pub enum ParseResult {
    /// Full parse was performed (no cache hit)
    Full,
    /// Source was unchanged, returned cached tree
    Cached,
    /// Incremental parse was performed
    Incremental {
        /// Ranges that changed structurally
        changed_ranges: Vec<tree_sitter::Range>,
        /// The edit that was applied
        edit: InputEdit,
    },
}

impl ParseResult {
    /// Check if this was an incremental parse
    pub fn is_incremental(&self) -> bool {
        matches!(self, ParseResult::Incremental { .. })
    }

    /// Check if this was a cache hit (no parsing needed)
    pub fn is_cached(&self) -> bool {
        matches!(self, ParseResult::Cached)
    }

    /// Get changed ranges if incremental
    pub fn changed_ranges(&self) -> Option<&[tree_sitter::Range]> {
        match self {
            ParseResult::Incremental { changed_ranges, .. } => Some(changed_ranges),
            _ => None,
        }
    }
}

/// Statistics about the AST cache
#[derive(Debug, Clone)]
pub struct AstCacheStats {
    /// Number of files in cache
    pub file_count: usize,
    /// Total bytes of source code cached
    pub total_source_bytes: usize,
}

/// Compute the InputEdit for tree-sitter given old and new source
///
/// This function computes where the source changed by finding the first
/// and last differing bytes. This is a simple approach that works well
/// for typical edits (insert, delete, modify in one location).
///
/// For more complex edits (multiple distant changes), this creates a
/// single edit spanning all changes, which is less optimal but still correct.
pub fn compute_edit(old_source: &str, new_source: &str) -> InputEdit {
    let old_bytes = old_source.as_bytes();
    let new_bytes = new_source.as_bytes();

    // Find first differing byte
    let start_byte = old_bytes
        .iter()
        .zip(new_bytes.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(old_bytes.len().min(new_bytes.len()));

    // Find last differing byte (from end)
    let old_suffix_len = old_bytes[start_byte..]
        .iter()
        .rev()
        .zip(new_bytes[start_byte..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    let old_end_byte = old_bytes.len() - old_suffix_len;
    let new_end_byte = new_bytes.len() - old_suffix_len;

    // Convert byte positions to Points (row, column)
    let start_point = byte_to_point(old_source, start_byte);
    let old_end_point = byte_to_point(old_source, old_end_byte);
    let new_end_point = byte_to_point(new_source, new_end_byte);

    InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: start_point,
        old_end_position: old_end_point,
        new_end_position: new_end_point,
    }
}

/// Convert a byte offset to a Point (row, column)
fn byte_to_point(source: &str, byte_offset: usize) -> Point {
    let mut row = 0;
    let mut col = 0;
    let mut current_byte = 0;

    for ch in source.chars() {
        if current_byte >= byte_offset {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_byte += ch.len_utf8();
    }

    Point { row, column: col }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_edit_insert() {
        let old = "function foo() {}";
        let new = "function foobar() {}";

        let edit = compute_edit(old, new);

        assert_eq!(edit.start_byte, 12); // "foo" -> "foobar"
        assert_eq!(edit.old_end_byte, 12);
        assert_eq!(edit.new_end_byte, 15);
    }

    #[test]
    fn test_compute_edit_delete() {
        let old = "function foobar() {}";
        let new = "function foo() {}";

        let edit = compute_edit(old, new);

        assert_eq!(edit.start_byte, 12);
        assert_eq!(edit.old_end_byte, 15);
        assert_eq!(edit.new_end_byte, 12);
    }

    #[test]
    fn test_compute_edit_replace() {
        let old = "let x = 1;";
        let new = "let x = 42;";

        let edit = compute_edit(old, new);

        assert_eq!(edit.start_byte, 8); // "1" -> "42"
        assert_eq!(edit.old_end_byte, 9);
        assert_eq!(edit.new_end_byte, 10);
    }

    #[test]
    fn test_compute_edit_multiline() {
        let old = "line1\nline2";
        let new = "line1\nmodified\nline2";

        let edit = compute_edit(old, new);

        assert!(edit.start_byte > 0);
        assert!(edit.new_end_byte > edit.old_end_byte);
    }

    #[test]
    fn test_byte_to_point() {
        let source = "line1\nline2\nline3";

        // Start of file
        let p = byte_to_point(source, 0);
        assert_eq!(p.row, 0);
        assert_eq!(p.column, 0);

        // Middle of line 1
        let p = byte_to_point(source, 3);
        assert_eq!(p.row, 0);
        assert_eq!(p.column, 3);

        // Start of line 2
        let p = byte_to_point(source, 6);
        assert_eq!(p.row, 1);
        assert_eq!(p.column, 0);

        // Middle of line 3
        let p = byte_to_point(source, 14);
        assert_eq!(p.row, 2);
        assert_eq!(p.column, 2);
    }

    #[test]
    fn test_ast_cache_new() {
        let cache = AstCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_ast_cache_full_parse() {
        let cache = AstCache::new();
        let path = PathBuf::from("/tmp/test.ts");
        let source = "function foo() { return 1; }";

        let (tree, result) = cache
            .parse_file(&path, source, Lang::TypeScript)
            .expect("Parse should succeed");

        assert!(matches!(result, ParseResult::Full));
        assert!(tree.root_node().kind() == "program");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_ast_cache_incremental_parse() {
        let cache = AstCache::new();
        let path = PathBuf::from("/tmp/test.ts");

        // First parse
        let source1 = "function foo() { return 1; }";
        let (_, result1) = cache
            .parse_file(&path, source1, Lang::TypeScript)
            .expect("Parse should succeed");
        assert!(matches!(result1, ParseResult::Full));

        // Incremental parse - change the function name
        let source2 = "function foobar() { return 1; }";
        let (tree2, result2) = cache
            .parse_file(&path, source2, Lang::TypeScript)
            .expect("Parse should succeed");

        assert!(result2.is_incremental());
        assert!(tree2.root_node().kind() == "program");

        // Note: changed_ranges() may be empty even for structural changes,
        // depending on how tree-sitter optimizes the comparison.
        // The important thing is that we got an incremental parse result.
        if let ParseResult::Incremental { edit, .. } = result2 {
            // The edit should reflect the change from "foo" to "foobar"
            assert!(edit.new_end_byte > edit.old_end_byte, "Edit should show bytes were added");
        }
    }

    #[test]
    fn test_ast_cache_cached() {
        let cache = AstCache::new();
        let path = PathBuf::from("/tmp/test.ts");
        let source = "function foo() { return 1; }";

        // First parse
        cache
            .parse_file(&path, source, Lang::TypeScript)
            .expect("Parse should succeed");

        // Same source - should be cached
        let (_, result) = cache
            .parse_file(&path, source, Lang::TypeScript)
            .expect("Parse should succeed");

        assert!(result.is_cached());
    }

    #[test]
    fn test_ast_cache_remove() {
        let cache = AstCache::new();
        let path = PathBuf::from("/tmp/test.ts");
        let source = "function foo() {}";

        cache
            .parse_file(&path, source, Lang::TypeScript)
            .expect("Parse should succeed");
        assert_eq!(cache.len(), 1);

        cache.remove(&path);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_ast_cache_stats() {
        let cache = AstCache::new();
        let path1 = PathBuf::from("/tmp/test1.ts");
        let path2 = PathBuf::from("/tmp/test2.ts");
        let source1 = "const x = 1;";
        let source2 = "const y = 2;";

        cache
            .parse_file(&path1, source1, Lang::TypeScript)
            .expect("Parse should succeed");
        cache
            .parse_file(&path2, source2, Lang::TypeScript)
            .expect("Parse should succeed");

        let stats = cache.stats();
        assert_eq!(stats.file_count, 2);
        assert_eq!(stats.total_source_bytes, source1.len() + source2.len());
    }
}
