//! Layer synchronization logic (SEM-52, SEM-104)
//!
//! This module implements incremental layer updates based on the update
//! strategy determined by drift detection.
//!
//! # Update Strategies
//!
//! | Strategy    | When                | Action                          |
//! |-------------|---------------------|--------------------------------|
//! | Fresh       | 0 files changed     | No action                      |
//! | Incremental | < 10 files          | Reparse changed files only     |
//! | Rebase      | < 30% of repo       | Reconcile overlay with new base|
//! | FullRebuild | >= 30% of repo      | Discard and recreate           |
//!
//! # Performance Targets
//!
//! - Single file change: < 500ms (typically <50ms with incremental parsing)
//! - Incremental update (< 10 files): 10x faster than full rebuild
//!
//! # Incremental Parsing (tree-sitter)
//!
//! When a file changes, we use tree-sitter's incremental parsing:
//! 1. Retrieve old source and AST from cache
//! 2. Compute InputEdit for what changed
//! 3. Call tree.edit(&edit) to adjust node ranges
//! 4. Parse with parser.parse(new_source, Some(&old_tree))
//! 5. Use changed_ranges() for selective re-extraction
//!
//! Performance: <1ms for small edits vs 5-50ms for full parse

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::drift::UpdateStrategy;
use crate::duplicate::{DuplicateDetector, DuplicateKind, FunctionSignature};
use crate::error::Result;
use crate::extract::extract;
use crate::lang::Lang;
use crate::overlay::{LayerKind, SymbolState};
use crate::schema::SymbolInfo;

use super::ast_cache::AstCache;
use super::events::{
    emit_event, DuplicateDetectedEvent, DuplicateFunctionInfo, DuplicateSimilarInfo,
};
use super::state::ServerState;

/// Statistics from a layer update operation
#[derive(Debug, Clone, Default)]
pub struct LayerUpdateStats {
    /// Number of files processed
    pub files_processed: usize,
    /// Number of symbols added
    pub symbols_added: usize,
    /// Number of symbols removed
    pub symbols_removed: usize,
    /// Number of symbols modified
    pub symbols_modified: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Update strategy used
    pub strategy: String,
    /// Number of files parsed incrementally (vs full reparse)
    pub incremental_parses: usize,
    /// Number of files where cached AST was reused (no changes)
    pub cached_parses: usize,
    /// Number of files that required full parsing
    pub full_parses: usize,
    /// Total parse time in microseconds
    pub parse_time_us: u64,
}

impl LayerUpdateStats {
    /// Create stats for a no-op update
    pub fn fresh() -> Self {
        Self {
            strategy: "Fresh".to_string(),
            ..Default::default()
        }
    }
}

/// Result of a rebase operation
#[derive(Debug, Clone)]
pub struct RebaseResult {
    /// Symbols that were preserved
    pub preserved: usize,
    /// Symbols that conflicted and were resolved
    pub conflicts_resolved: usize,
    /// Symbols that were discarded (no longer valid)
    pub discarded: usize,
}

/// Layer synchronization engine
///
/// Handles incremental updates, rebases, and full rebuilds based on
/// the update strategy from drift detection.
///
/// Includes an AST cache for tree-sitter incremental parsing, which
/// dramatically speeds up re-parsing when files have small changes.
pub struct LayerSynchronizer {
    /// Repository root path
    repo_root: PathBuf,
    /// Optional cache directory for persisting changes to disk
    cache_dir: Option<crate::cache::CacheDir>,
    /// AST cache for incremental parsing
    ast_cache: Arc<AstCache>,
}

impl LayerSynchronizer {
    /// Create a new synchronizer for a repository
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            cache_dir: None,
            ast_cache: Arc::new(AstCache::new()),
        }
    }

    /// Create a new synchronizer with disk cache enabled
    pub fn with_cache(repo_root: PathBuf, cache_dir: crate::cache::CacheDir) -> Self {
        Self {
            repo_root,
            cache_dir: Some(cache_dir),
            ast_cache: Arc::new(AstCache::new()),
        }
    }

    /// Create a new synchronizer with a shared AST cache
    pub fn with_ast_cache(repo_root: PathBuf, ast_cache: Arc<AstCache>) -> Self {
        Self {
            repo_root,
            cache_dir: None,
            ast_cache,
        }
    }

    /// Get AST cache statistics
    pub fn ast_cache_stats(&self) -> super::ast_cache::AstCacheStats {
        self.ast_cache.stats()
    }

    /// Update a layer using the specified strategy
    ///
    /// This is the main entry point for layer updates. It dispatches
    /// to the appropriate method based on the update strategy.
    pub fn update_layer(
        &self,
        state: &ServerState,
        layer: LayerKind,
        strategy: UpdateStrategy,
    ) -> Result<LayerUpdateStats> {
        let start = Instant::now();

        let stats = match strategy {
            UpdateStrategy::Fresh => LayerUpdateStats::fresh(),
            UpdateStrategy::Incremental(ref files) => {
                self.incremental_update(state, layer, files)?
            }
            UpdateStrategy::Rebase => {
                let result = self.rebase_layer(state, layer)?;
                LayerUpdateStats {
                    symbols_added: result.preserved,
                    symbols_removed: result.discarded,
                    symbols_modified: result.conflicts_resolved,
                    strategy: "Rebase".to_string(),
                    ..Default::default()
                }
            }
            UpdateStrategy::FullRebuild => self.full_rebuild_layer(state, layer)?,
        };

        let mut stats = stats;
        stats.duration_ms = start.elapsed().as_millis() as u64;

        // Mark layer as fresh after successful update
        state.mark_layer_fresh(layer);

        Ok(stats)
    }

    /// Incremental update - reparse only changed files
    ///
    /// This is the fastest update path for small changes (< 10 files).
    /// It parses only the changed files and updates the symbol index.
    /// After all files are processed, regenerates all graphs (call, import, module).
    pub fn incremental_update(
        &self,
        state: &ServerState,
        layer: LayerKind,
        changed_files: &[PathBuf],
    ) -> Result<LayerUpdateStats> {
        let mut stats = LayerUpdateStats {
            files_processed: changed_files.len(),
            strategy: format!("Incremental ({} files)", changed_files.len()),
            ..Default::default()
        };

        for file_path in changed_files {
            let file_stats = self.update_single_file(state, layer, file_path)?;
            stats.symbols_added += file_stats.symbols_added;
            stats.symbols_removed += file_stats.symbols_removed;
            stats.symbols_modified += file_stats.symbols_modified;
            stats.incremental_parses += file_stats.incremental_parses;
            stats.cached_parses += file_stats.cached_parses;
            stats.full_parses += file_stats.full_parses;
            stats.parse_time_us += file_stats.parse_time_us;
        }

        // After updating all files, regenerate graphs from the updated symbol index
        // This ensures call graph, import graph, and module graph stay in sync
        if let Some(ref cache_dir) = self.cache_dir {
            tracing::info!("[SYNC] Regenerating graphs after incremental update");
            match cache_dir.regenerate_graphs() {
                Ok(result) => {
                    tracing::info!(
                        "[SYNC] Graph regeneration complete: {} files -> {} call edges, {} import edges, {} module edges",
                        result.files_processed,
                        result.call_graph_entries,
                        result.import_graph_entries,
                        result.module_graph_entries
                    );
                }
                Err(e) => {
                    tracing::warn!("[SYNC] Graph regeneration failed: {}", e);
                    // Don't fail the whole update if graph regeneration fails
                }
            }
        }

        Ok(stats)
    }

    /// Update a single file in a layer
    ///
    /// This is the core incremental update operation:
    /// 1. Read file contents
    /// 2. Parse with tree-sitter (using incremental parsing if cached)
    /// 3. Extract symbols
    /// 4. Compare with existing symbols
    /// 5. Update overlay
    /// 6. Update disk cache (if cache_dir is set)
    fn update_single_file(
        &self,
        state: &ServerState,
        layer: LayerKind,
        file_path: &PathBuf,
    ) -> Result<LayerUpdateStats> {
        let mut stats = LayerUpdateStats::default();
        let full_path = self.repo_root.join(file_path);

        // Check if file exists
        if !full_path.exists() {
            // File was deleted - mark all its symbols as deleted
            // Also remove from AST cache
            self.ast_cache.remove(&full_path);
            return self.mark_file_deleted(state, layer, file_path);
        }

        // Determine language
        let lang = match Lang::from_path(&full_path) {
            Ok(l) => l,
            Err(_) => return Ok(stats), // Skip unsupported files
        };

        // Read file contents
        let source = std::fs::read_to_string(&full_path)?;

        // Parse with AST cache (incremental if cached)
        let parse_start = Instant::now();
        let (tree, parse_result) = self.ast_cache.parse_file(&full_path, &source, lang)
            .map_err(|e| crate::error::McpDiffError::ParseFailure { message: e })?;
        let parse_duration = parse_start.elapsed();
        stats.parse_time_us = parse_duration.as_micros() as u64;

        // Track parse type for statistics
        match &parse_result {
            super::ast_cache::ParseResult::Full => {
                stats.full_parses = 1;
                tracing::debug!(
                    "[SYNC] Full parse for {:?} in {:?}",
                    file_path,
                    parse_duration
                );
            }
            super::ast_cache::ParseResult::Cached => {
                stats.cached_parses = 1;
                tracing::debug!(
                    "[SYNC] Cache hit for {:?} (no changes)",
                    file_path
                );
            }
            super::ast_cache::ParseResult::Incremental { changed_ranges, .. } => {
                stats.incremental_parses = 1;
                tracing::info!(
                    "[SYNC] Incremental parse for {:?} in {:?} ({} changed ranges)",
                    file_path,
                    parse_duration,
                    changed_ranges.len()
                );
            }
        }

        // Extract symbols
        let summary = extract(&full_path, &source, &tree, lang)?;

        // Get existing symbol hashes for this file
        let existing_hashes: HashSet<String> = state.read(|index| {
            let overlay = index.layer(layer);
            // Get hashes from symbols_by_file directly
            overlay.symbols_by_file
                .get(file_path)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect()
        });

        // Compute new symbol hashes and build index entries for cache
        let mut new_hashes = HashSet::new();
        let mut index_entries = Vec::new();

        // Extract module name from file path
        let module_name = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();

        // Pre-compute hashes and index entries before consuming symbols
        // NOTE: Always use full_path (absolute) for hash computation - this is the canonical rule
        let symbols_with_hashes: Vec<_> = summary.symbols.into_iter().map(|symbol| {
            let hash = crate::overlay::compute_symbol_hash(&symbol, &full_path.to_string_lossy());

            // Build index entry for disk cache
            let entry = crate::cache::SymbolIndexEntry {
                symbol: symbol.name.clone(),
                hash: hash.clone(),
                semantic_hash: crate::overlay::extract_semantic_hash(&hash).to_string(),
                kind: format!("{:?}", symbol.kind).to_lowercase(),
                module: module_name.clone(),
                file: full_path.to_string_lossy().to_string(),
                lines: format!("{}-{}", symbol.start_line, symbol.end_line),
                risk: format!("{:?}", symbol.behavioral_risk).to_lowercase(),
                cognitive_complexity: 0, // TODO: Calculate from control_flow
                max_nesting: 0,          // TODO: Calculate from control_flow
            };

            (symbol, hash, entry)
        }).collect();

        // Collect new symbols for duplicate checking (before consuming them)
        let new_symbols_for_check: Vec<(SymbolInfo, String)> = symbols_with_hashes
            .iter()
            .filter(|(_, hash, _)| !existing_hashes.contains(hash))
            .map(|(symbol, hash, _)| (symbol.clone(), hash.clone()))
            .collect();

        // Update overlay with new symbols
        state.write(|index| {
            let overlay = index.layer_mut(layer);

            for (symbol, hash, entry) in symbols_with_hashes {
                new_hashes.insert(hash.clone());
                index_entries.push(entry);

                let is_new = !existing_hashes.contains(&hash);
                let symbol_state = SymbolState::active(symbol);

                if is_new {
                    stats.symbols_added += 1;
                } else {
                    stats.symbols_modified += 1;
                }

                overlay.upsert(hash, symbol_state);
            }

            // Mark removed symbols as deleted
            for hash in existing_hashes.difference(&new_hashes) {
                overlay.delete(hash);
                stats.symbols_removed += 1;
            }
        });

        // Check for duplicates in newly added symbols (non-blocking)
        if !new_symbols_for_check.is_empty() {
            self.check_and_emit_duplicates(&new_symbols_for_check, &full_path.to_string_lossy());
        }

        // Update disk cache if available
        if let Some(ref cache_dir) = self.cache_dir {
            if let Err(e) = cache_dir.update_symbol_index_for_file(
                &full_path.to_string_lossy(),
                index_entries,
            ) {
                tracing::warn!("[SYNC] Failed to update disk cache for {:?}: {}", file_path, e);
            } else {
                tracing::info!("[SYNC] Updated disk cache for {:?}: {} symbols", file_path, new_hashes.len());
            }
        }

        stats.files_processed = 1;
        Ok(stats)
    }

    /// Mark all symbols from a deleted file as deleted
    fn mark_file_deleted(
        &self,
        state: &ServerState,
        layer: LayerKind,
        file_path: &PathBuf,
    ) -> Result<LayerUpdateStats> {
        let mut stats = LayerUpdateStats::default();

        state.write(|index| {
            let overlay = index.layer_mut(layer);
            // Get hashes from symbols_by_file directly
            let hashes = overlay.symbols_by_file
                .get(file_path)
                .cloned()
                .unwrap_or_default();

            for hash in hashes {
                overlay.delete(&hash);
                stats.symbols_removed += 1;
            }
        });

        stats.files_processed = 1;
        Ok(stats)
    }

    /// Rebase layer - reconcile overlay with new base
    ///
    /// This is used when the base branch has moved (e.g., after pulling).
    /// It preserves local changes while incorporating base changes.
    pub fn rebase_layer(&self, state: &ServerState, layer: LayerKind) -> Result<RebaseResult> {
        use std::collections::HashMap;

        let mut result = RebaseResult {
            preserved: 0,
            conflicts_resolved: 0,
            discarded: 0,
        };

        // Get current overlay state and base content hashes in a single read
        let (overlay_hashes, base_hashes, base_content_map): (
            HashSet<String>,
            HashSet<String>,
            HashMap<String, Option<String>>
        ) = state.read(|index| {
            let overlay = index.layer(layer);
            let base = index.layer(LayerKind::Base);

            let overlay_hashes: HashSet<_> = overlay.symbols.keys().cloned().collect();
            let base_hashes: HashSet<_> = base.symbols.keys().cloned().collect();

            // Pre-collect base content hashes for common symbols
            let base_content_map: HashMap<_, _> = base.symbols
                .iter()
                .map(|(hash, state)| (hash.clone(), state.base_content_hash().map(|s| s.to_string())))
                .collect();

            (overlay_hashes, base_hashes, base_content_map)
        });

        // Symbols in both layers - check for conflicts
        let common: HashSet<_> = overlay_hashes.intersection(&base_hashes).cloned().collect();

        // Symbols only in overlay - preserve if still valid
        let overlay_only: HashSet<_> = overlay_hashes.difference(&base_hashes).cloned().collect();

        state.write(|index| {
            let overlay = index.layer_mut(layer);

            // Preserve overlay-only symbols (local changes not in base)
            result.preserved = overlay_only.len();

            // For common symbols, check if overlay version differs from base
            for hash in common {
                // If content hashes match, remove from overlay (use base version)
                let overlay_content = overlay
                    .symbols
                    .get(&hash)
                    .and_then(|s| s.base_content_hash().map(|h| h.to_string()));
                let base_content = base_content_map.get(&hash).cloned().flatten();

                if overlay_content == base_content {
                    // Same content - can remove from overlay
                    overlay.symbols.remove(&hash);
                    result.discarded += 1;
                } else {
                    // Different content - conflict, keep overlay version
                    result.conflicts_resolved += 1;
                }
            }
        });

        Ok(result)
    }

    /// Full rebuild - discard overlay and recreate from scratch
    ///
    /// This is used when too many files have changed (>= 30% of repo).
    /// It's more efficient to rebuild than to incrementally update.
    pub fn full_rebuild_layer(
        &self,
        state: &ServerState,
        layer: LayerKind,
    ) -> Result<LayerUpdateStats> {
        let start = Instant::now();

        // Clear the layer first
        let old_count = state.read(|index| index.layer(layer).active_count());
        state.clear_layer(layer);

        // For base layer, we need to reindex the entire repo
        // For other layers, we need to reprocess the appropriate diff
        let stats = match layer {
            LayerKind::Base => {
                // Full repo reindex would be done by the sharding system
                // Here we just return stats for the clear operation
                LayerUpdateStats {
                    symbols_removed: old_count,
                    strategy: "FullRebuild (cleared)".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ..Default::default()
                }
            }
            LayerKind::Branch => {
                // Get changed files since base
                let changed = crate::git::get_changed_files("HEAD~1", "HEAD", Some(self.repo_root.as_path()))?;
                let paths: Vec<PathBuf> = changed.into_iter().map(|c| PathBuf::from(c.path)).collect();
                self.incremental_update(state, layer, &paths)?
            }
            LayerKind::Working => {
                // Get uncommitted changes
                let changed = crate::git::get_changed_files("HEAD", "HEAD", Some(self.repo_root.as_path()))?;
                let paths: Vec<PathBuf> = changed.into_iter().map(|c| PathBuf::from(c.path)).collect();
                self.incremental_update(state, layer, &paths)?
            }
            LayerKind::AI => {
                // AI layer is in-memory only, just clear it
                LayerUpdateStats {
                    symbols_removed: old_count,
                    strategy: "FullRebuild (AI cleared)".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ..Default::default()
                }
            }
        };

        Ok(stats)
    }

    /// Check for duplicates of newly added/modified symbols and emit events
    ///
    /// This is called after symbols are upserted during incremental updates.
    /// It checks each new symbol against the signature index and emits
    /// DuplicateDetectedEvent for any matches found.
    fn check_and_emit_duplicates(
        &self,
        new_symbols: &[(SymbolInfo, String)], // (symbol, hash)
        file_path: &str,
    ) {
        // Only check if we have a cache directory with signature index
        let cache_dir = match &self.cache_dir {
            Some(c) => c,
            None => return,
        };

        // Load existing signatures
        let sig_path = cache_dir.signature_index_path();
        if !sig_path.exists() {
            return;
        }

        let signatures: Vec<FunctionSignature> = match std::fs::File::open(&sig_path) {
            Ok(file) => {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .flatten()
                    .filter_map(|line| serde_json::from_str(&line).ok())
                    .collect()
            }
            Err(_) => return,
        };

        if signatures.is_empty() {
            return;
        }

        // Create detector with default threshold
        let detector = DuplicateDetector::new(0.90);

        // Check each new symbol for duplicates
        for (symbol, hash) in new_symbols {
            // Generate signature for this symbol
            let sig = FunctionSignature::from_symbol_info(symbol, hash, file_path, None);

            // Skip if no business logic (utility functions, etc.)
            if !sig.has_business_logic {
                continue;
            }

            // Find duplicates
            let matches = detector.find_duplicates(&sig, &signatures);

            if !matches.is_empty() {
                // Build event
                let new_func = DuplicateFunctionInfo {
                    name: symbol.name.clone(),
                    file: file_path.to_string(),
                    lines: format!("{}-{}", symbol.start_line, symbol.end_line),
                    hash: hash.clone(),
                };

                let similar: Vec<DuplicateSimilarInfo> = matches
                    .iter()
                    .map(|m| {
                        let kind_str = match m.kind {
                            DuplicateKind::Exact => "exact",
                            DuplicateKind::Near => "near",
                            DuplicateKind::Divergent => "divergent",
                        };
                        DuplicateSimilarInfo {
                            name: m.symbol.name.clone(),
                            file: m.symbol.file.clone(),
                            lines: format!("{}-{}", m.symbol.start_line, m.symbol.end_line),
                            hash: m.symbol.hash.clone(),
                            similarity: (m.similarity * 100.0) as u8,
                            kind: kind_str.to_string(),
                            differences: m.differences.iter().map(|d| d.to_string()).collect(),
                        }
                    })
                    .collect();

                let event = DuplicateDetectedEvent::new(new_func, similar);
                emit_event(&event);

                tracing::info!(
                    "[SYNC] Duplicate detected: {} has {} similar functions",
                    symbol.name,
                    matches.len()
                );
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_update_stats_fresh() {
        let stats = LayerUpdateStats::fresh();
        assert_eq!(stats.files_processed, 0);
        assert_eq!(stats.strategy, "Fresh");
    }

    #[test]
    fn test_rebase_result_default() {
        let result = RebaseResult {
            preserved: 10,
            conflicts_resolved: 2,
            discarded: 5,
        };
        assert_eq!(result.preserved, 10);
        assert_eq!(result.conflicts_resolved, 2);
        assert_eq!(result.discarded, 5);
    }
}
