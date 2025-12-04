//! Overlay system for layered state management
//!
//! This module implements the core data structures for SEM-44:
//! - `LayerKind` - Types of layers (Base, Branch, Working, AI)
//! - `SymbolState` - State of a symbol in an overlay (Active, Deleted, Modified)
//! - `FileMove` - Tracks file renames for path resolution
//! - `LayerMeta` - Metadata for tracking indexed SHA, merge-base, timestamps
//! - `Overlay` - Per-layer symbol storage
//! - `LayeredIndex` - Full layer stack management
//!
//! # Layered State Model
//!
//! ```text
//! Layer 0: BASE (main/master)     - Persistent, full sharded index
//!     ↓
//! Layer 1: BRANCH                 - Commits since diverging from base
//!     ↓
//! Layer 2: WORKING                - Uncommitted changes (staged + unstaged)
//!     ↓
//! Layer 3: AI PROPOSED            - In-memory changes not yet on disk
//! ```
//!
//! # Query Resolution
//!
//! When looking up a symbol, check layers top-down (AI → Working → Branch → Base).
//! First match wins. A `Deleted` marker stops the search and returns None.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::schema::{fnv1a_hash, SymbolInfo};

// ============================================================================
// Layer Types
// ============================================================================

/// Kind of layer in the overlay stack
///
/// Layers are ordered from lowest (Base) to highest (AI).
/// Higher layers shadow lower layers during queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    /// Base layer - main/master branch, persistent full sharded index
    Base = 0,
    /// Branch layer - commits since diverging from base
    Branch = 1,
    /// Working layer - uncommitted changes (staged + unstaged)
    Working = 2,
    /// AI layer - proposed changes not yet on disk
    AI = 3,
}

impl LayerKind {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Branch => "branch",
            Self::Working => "working",
            Self::AI => "ai",
        }
    }

    /// Get all layer kinds in order from highest to lowest priority
    pub fn all_descending() -> [LayerKind; 4] {
        [LayerKind::AI, LayerKind::Working, LayerKind::Branch, LayerKind::Base]
    }

    /// Get all layer kinds in order from lowest to highest priority
    pub fn all_ascending() -> [LayerKind; 4] {
        [LayerKind::Base, LayerKind::Branch, LayerKind::Working, LayerKind::AI]
    }
}

impl std::fmt::Display for LayerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Symbol State
// ============================================================================

/// State of a symbol in an overlay
///
/// Symbols can be active (added/modified), deleted, or modified from a base version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum SymbolState {
    /// Symbol is active (new or modified)
    Active {
        /// The symbol information
        symbol: SymbolInfo,
        /// File path where this symbol is located (for file-based lookups)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_path: Option<PathBuf>,
        /// Hash of the base content this was derived from (for conflict detection)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_content_hash: Option<String>,
    },
    /// Symbol was deleted in this layer
    Deleted {
        /// Original symbol hash that was deleted
        original_hash: String,
        /// When the deletion occurred (Unix timestamp)
        deleted_at: u64,
    },
}

impl SymbolState {
    /// Create a new active symbol state
    pub fn active(symbol: SymbolInfo) -> Self {
        Self::Active {
            symbol,
            file_path: None,
            base_content_hash: None,
        }
    }

    /// Create a new active symbol state with file path
    pub fn active_at(symbol: SymbolInfo, file_path: PathBuf) -> Self {
        Self::Active {
            symbol,
            file_path: Some(file_path),
            base_content_hash: None,
        }
    }

    /// Create a new active symbol state with base content tracking
    pub fn active_with_base(symbol: SymbolInfo, base_content_hash: String) -> Self {
        Self::Active {
            symbol,
            file_path: None,
            base_content_hash: Some(base_content_hash),
        }
    }

    /// Create a new active symbol state with file path and base content tracking
    pub fn active_at_with_base(symbol: SymbolInfo, file_path: PathBuf, base_content_hash: String) -> Self {
        Self::Active {
            symbol,
            file_path: Some(file_path),
            base_content_hash: Some(base_content_hash),
        }
    }

    /// Create a deleted symbol state
    pub fn deleted(original_hash: String) -> Self {
        let deleted_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self::Deleted {
            original_hash,
            deleted_at,
        }
    }

    /// Check if this is an active state
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// Check if this is a deleted state
    pub fn is_deleted(&self) -> bool {
        matches!(self, Self::Deleted { .. })
    }

    /// Get the symbol if active
    pub fn as_symbol(&self) -> Option<&SymbolInfo> {
        match self {
            Self::Active { symbol, .. } => Some(symbol),
            Self::Deleted { .. } => None,
        }
    }

    /// Get the file path if available
    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Active { file_path, .. } => file_path.as_ref(),
            Self::Deleted { .. } => None,
        }
    }

    /// Get the base content hash if available
    pub fn base_content_hash(&self) -> Option<&str> {
        match self {
            Self::Active { base_content_hash, .. } => base_content_hash.as_deref(),
            Self::Deleted { .. } => None,
        }
    }
}

// ============================================================================
// File Move Tracking
// ============================================================================

/// Tracks a file rename/move for path resolution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMove {
    /// Original file path (before move)
    pub from_path: PathBuf,
    /// New file path (after move)
    pub to_path: PathBuf,
    /// When the move was recorded (Unix timestamp)
    pub moved_at: u64,
    /// Git commit where the move occurred (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

impl FileMove {
    /// Create a new file move record
    pub fn new(from_path: PathBuf, to_path: PathBuf) -> Self {
        let moved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            from_path,
            to_path,
            moved_at,
            commit_sha: None,
        }
    }

    /// Create a new file move record with commit info
    pub fn with_commit(from_path: PathBuf, to_path: PathBuf, commit_sha: String) -> Self {
        let moved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            from_path,
            to_path,
            moved_at,
            commit_sha: Some(commit_sha),
        }
    }
}

// ============================================================================
// Layer Metadata
// ============================================================================

/// Metadata for a layer
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayerMeta {
    /// Layer kind
    pub kind: Option<LayerKind>,
    /// Git SHA this layer was indexed at
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_sha: Option<String>,
    /// Merge base SHA (for branch layer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_base_sha: Option<String>,
    /// When this layer was created (Unix timestamp)
    pub created_at: u64,
    /// When this layer was last updated (Unix timestamp)
    pub updated_at: u64,
    /// Number of symbols in this layer
    pub symbol_count: usize,
    /// Number of deleted symbols in this layer
    pub deleted_count: usize,
    /// Number of file moves tracked
    pub move_count: usize,
}

impl LayerMeta {
    /// Create new metadata for a layer
    pub fn new(kind: LayerKind) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            kind: Some(kind),
            indexed_sha: None,
            merge_base_sha: None,
            created_at: now,
            updated_at: now,
            symbol_count: 0,
            deleted_count: 0,
            move_count: 0,
        }
    }

    /// Update the timestamp
    pub fn touch(&mut self) {
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
}

// ============================================================================
// Overlay
// ============================================================================

/// Per-layer symbol storage
///
/// An overlay contains symbols that were added/modified/deleted in a specific layer.
/// Symbols are indexed by their content-addressable hash.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Overlay {
    /// Layer metadata
    pub meta: LayerMeta,
    /// Symbols in this overlay, keyed by symbol hash
    pub symbols: HashMap<String, SymbolState>,
    /// Deleted symbol hashes (for quick lookup)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub deleted: HashSet<String>,
    /// File moves tracked in this layer
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<FileMove>,
    /// Symbols indexed by file path for quick file-based lookups
    #[serde(skip)]
    pub symbols_by_file: HashMap<PathBuf, Vec<String>>,
}

// Custom Deserialize implementation that rebuilds the file index after deserialization
impl<'de> Deserialize<'de> for Overlay {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Shadow struct for deserialization
        #[derive(Deserialize)]
        struct OverlayData {
            meta: LayerMeta,
            symbols: HashMap<String, SymbolState>,
            #[serde(default)]
            deleted: HashSet<String>,
            #[serde(default)]
            moves: Vec<FileMove>,
        }

        let data = OverlayData::deserialize(deserializer)?;
        let mut overlay = Overlay {
            meta: data.meta,
            symbols: data.symbols,
            deleted: data.deleted,
            moves: data.moves,
            symbols_by_file: HashMap::new(),
        };
        overlay.rebuild_file_index();
        Ok(overlay)
    }
}

impl Overlay {
    /// Create a new empty overlay for a layer kind
    pub fn new(kind: LayerKind) -> Self {
        Self {
            meta: LayerMeta::new(kind),
            symbols: HashMap::new(),
            deleted: HashSet::new(),
            moves: Vec::new(),
            symbols_by_file: HashMap::new(),
        }
    }

    /// Get the layer kind
    pub fn kind(&self) -> Option<LayerKind> {
        self.meta.kind
    }

    /// Check if the overlay is empty
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty() && self.deleted.is_empty() && self.moves.is_empty()
    }

    /// Get the number of active symbols
    pub fn active_count(&self) -> usize {
        self.symbols.values().filter(|s| s.is_active()).count()
    }

    /// Get the number of deleted symbols
    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }

    /// Insert or update a symbol
    ///
    /// Returns the previous state if any.
    pub fn upsert(&mut self, hash: String, state: SymbolState) -> Option<SymbolState> {
        // Update file index if this is an active symbol with a file path
        if let Some(file_path) = state.file_path() {
            self.symbols_by_file
                .entry(file_path.clone())
                .or_default()
                .push(hash.clone());
        }

        // Update deleted set
        if state.is_deleted() {
            self.deleted.insert(hash.clone());
        } else {
            self.deleted.remove(&hash);
        }

        self.meta.touch();
        self.update_counts();
        self.symbols.insert(hash, state)
    }

    /// Mark a symbol as deleted
    ///
    /// Returns true if the symbol existed and was marked as deleted,
    /// false if creating a tombstone for a non-existent symbol.
    pub fn delete(&mut self, hash: &str) -> bool {
        let existed = self.symbols.contains_key(hash);
        self.deleted.insert(hash.to_string());
        let state = SymbolState::deleted(hash.to_string());
        self.symbols.insert(hash.to_string(), state);
        self.meta.touch();
        self.update_counts();
        existed
    }

    /// Get a symbol by hash
    pub fn get(&self, hash: &str) -> Option<&SymbolState> {
        self.symbols.get(hash)
    }

    /// Check if a symbol is deleted in this layer
    pub fn is_deleted(&self, hash: &str) -> bool {
        self.deleted.contains(hash)
    }

    /// Record a file move
    pub fn record_move(&mut self, from_path: PathBuf, to_path: PathBuf) {
        self.moves.push(FileMove::new(from_path, to_path));
        self.meta.touch();
        self.update_counts();
    }

    /// Resolve a file path through move history
    ///
    /// Given an old path, returns the current path after all moves.
    pub fn resolve_path(&self, path: &PathBuf) -> PathBuf {
        let mut current = path.clone();
        for mv in &self.moves {
            if mv.from_path == current {
                current = mv.to_path.clone();
            }
        }
        current
    }

    /// Get all symbols for a file path
    pub fn get_file_symbols(&self, path: &PathBuf) -> Vec<&SymbolState> {
        self.symbols_by_file
            .get(path)
            .map(|hashes| {
                hashes
                    .iter()
                    .filter_map(|h| self.symbols.get(h))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update metadata counts
    fn update_counts(&mut self) {
        self.meta.symbol_count = self.active_count();
        self.meta.deleted_count = self.deleted_count();
        self.meta.move_count = self.moves.len();
    }

    /// Rebuild the file index from symbols
    ///
    /// This should be called after deserialization to rebuild the
    /// `symbols_by_file` index which is not serialized.
    pub fn rebuild_file_index(&mut self) {
        self.symbols_by_file.clear();
        for (hash, state) in &self.symbols {
            if let Some(file_path) = state.file_path() {
                self.symbols_by_file
                    .entry(file_path.clone())
                    .or_default()
                    .push(hash.clone());
            }
        }
    }
}

// ============================================================================
// Content-Addressable Symbol Hash
// ============================================================================

/// Compute a content-addressable hash for a symbol
///
/// This hash is based on the symbol's content, not its location,
/// so it survives file moves and refactors.
pub fn compute_symbol_hash(symbol: &SymbolInfo, file_path: &str) -> String {
    // Hash based on: namespace + name + kind + signature
    let namespace = crate::schema::SymbolId::namespace_from_path(file_path);
    let signature = format!(
        "{}:{}:{}:{}",
        namespace,
        symbol.name,
        symbol.kind.as_str(),
        symbol.arguments.len() + symbol.props.len()
    );
    format!("{:016x}", fnv1a_hash(&signature))
}

/// Compute a hash of symbol content for conflict detection
///
/// This hash changes when the symbol's implementation changes,
/// used to detect when the base changed under an overlay.
pub fn compute_content_hash(symbol: &SymbolInfo) -> String {
    // Hash the semantic content
    let content = format!(
        "{}:{}:{:?}:{:?}:{:?}",
        symbol.name,
        symbol.kind.as_str(),
        symbol.arguments,
        symbol.props,
        symbol.calls
    );
    format!("{:016x}", fnv1a_hash(&content))
}

// ============================================================================
// Layered Index
// ============================================================================

/// Full layer stack for managing overlays
///
/// Provides query resolution across all layers with proper precedence.
#[derive(Debug, Clone, Default)]
pub struct LayeredIndex {
    /// Base layer (full index)
    pub base: Overlay,
    /// Branch layer (commits since base)
    pub branch: Overlay,
    /// Working layer (uncommitted changes)
    pub working: Overlay,
    /// AI layer (proposed changes)
    pub ai: Overlay,
}

impl LayeredIndex {
    /// Create a new empty layered index
    pub fn new() -> Self {
        Self {
            base: Overlay::new(LayerKind::Base),
            branch: Overlay::new(LayerKind::Branch),
            working: Overlay::new(LayerKind::Working),
            ai: Overlay::new(LayerKind::AI),
        }
    }

    /// Get a reference to a specific layer
    pub fn layer(&self, kind: LayerKind) -> &Overlay {
        match kind {
            LayerKind::Base => &self.base,
            LayerKind::Branch => &self.branch,
            LayerKind::Working => &self.working,
            LayerKind::AI => &self.ai,
        }
    }

    /// Get a mutable reference to a specific layer
    pub fn layer_mut(&mut self, kind: LayerKind) -> &mut Overlay {
        match kind {
            LayerKind::Base => &mut self.base,
            LayerKind::Branch => &mut self.branch,
            LayerKind::Working => &mut self.working,
            LayerKind::AI => &mut self.ai,
        }
    }

    /// Resolve a symbol by hash across all layers
    ///
    /// Checks layers from highest (AI) to lowest (Base) priority.
    /// Returns None if the symbol is deleted or not found.
    pub fn resolve_symbol(&self, hash: &str) -> Option<&SymbolInfo> {
        for kind in LayerKind::all_descending() {
            let layer = self.layer(kind);

            // Check if deleted in this layer
            if layer.is_deleted(hash) {
                return None;
            }

            // Check if exists in this layer
            if let Some(state) = layer.get(hash) {
                return state.as_symbol();
            }
        }
        None
    }

    /// Check if a symbol exists (not deleted) in any layer
    pub fn symbol_exists(&self, hash: &str) -> bool {
        self.resolve_symbol(hash).is_some()
    }

    /// Get all active symbol hashes across all layers
    pub fn all_symbol_hashes(&self) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut deleted = HashSet::new();

        // Collect from highest to lowest priority
        for kind in LayerKind::all_descending() {
            let layer = self.layer(kind);

            // Track deletions
            deleted.extend(layer.deleted.iter().cloned());

            // Add active symbols not yet deleted
            for (hash, state) in &layer.symbols {
                if state.is_active() && !deleted.contains(hash) {
                    result.insert(hash.clone());
                }
            }
        }

        result
    }

    /// Resolve a file path through all move histories
    pub fn resolve_path(&self, path: &PathBuf) -> PathBuf {
        let mut current = path.clone();

        // Apply moves from base to AI (chronological order)
        for kind in LayerKind::all_ascending() {
            current = self.layer(kind).resolve_path(&current);
        }

        current
    }

    /// Clear a specific layer
    pub fn clear_layer(&mut self, kind: LayerKind) {
        *self.layer_mut(kind) = Overlay::new(kind);
    }

    /// Get statistics about the layered index
    pub fn stats(&self) -> LayeredIndexStats {
        LayeredIndexStats {
            base_symbols: self.base.active_count(),
            branch_symbols: self.branch.active_count(),
            working_symbols: self.working.active_count(),
            ai_symbols: self.ai.active_count(),
            total_deleted: self.base.deleted_count()
                + self.branch.deleted_count()
                + self.working.deleted_count()
                + self.ai.deleted_count(),
            total_moves: self.base.moves.len()
                + self.branch.moves.len()
                + self.working.moves.len()
                + self.ai.moves.len(),
        }
    }
}

/// Statistics about a layered index
#[derive(Debug, Clone, Default)]
pub struct LayeredIndexStats {
    /// Number of active symbols in base layer
    pub base_symbols: usize,
    /// Number of active symbols in branch layer
    pub branch_symbols: usize,
    /// Number of active symbols in working layer
    pub working_symbols: usize,
    /// Number of active symbols in AI layer
    pub ai_symbols: usize,
    /// Total number of deleted symbols across all layers
    pub total_deleted: usize,
    /// Total number of file moves tracked
    pub total_moves: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Argument, RiskLevel, SymbolKind};

    // ------------------------------------------------------------------------
    // LayerKind Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_layer_kind_ordering() {
        assert!(LayerKind::Base < LayerKind::Branch);
        assert!(LayerKind::Branch < LayerKind::Working);
        assert!(LayerKind::Working < LayerKind::AI);
        assert!(LayerKind::Base < LayerKind::AI);
    }

    #[test]
    fn test_layer_kind_as_str() {
        assert_eq!(LayerKind::Base.as_str(), "base");
        assert_eq!(LayerKind::Branch.as_str(), "branch");
        assert_eq!(LayerKind::Working.as_str(), "working");
        assert_eq!(LayerKind::AI.as_str(), "ai");
    }

    #[test]
    fn test_layer_kind_all_descending() {
        let layers = LayerKind::all_descending();
        assert_eq!(layers[0], LayerKind::AI);
        assert_eq!(layers[1], LayerKind::Working);
        assert_eq!(layers[2], LayerKind::Branch);
        assert_eq!(layers[3], LayerKind::Base);
    }

    #[test]
    fn test_layer_kind_all_ascending() {
        let layers = LayerKind::all_ascending();
        assert_eq!(layers[0], LayerKind::Base);
        assert_eq!(layers[1], LayerKind::Branch);
        assert_eq!(layers[2], LayerKind::Working);
        assert_eq!(layers[3], LayerKind::AI);
    }

    // ------------------------------------------------------------------------
    // SymbolState Tests
    // ------------------------------------------------------------------------

    fn make_test_symbol(name: &str) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 10,
            is_exported: true,
            behavioral_risk: RiskLevel::Low,
            ..Default::default()
        }
    }

    #[test]
    fn test_symbol_state_active() {
        let symbol = make_test_symbol("test_fn");
        let state = SymbolState::active(symbol.clone());

        assert!(state.is_active());
        assert!(!state.is_deleted());
        assert!(state.as_symbol().is_some());
        assert_eq!(state.as_symbol().unwrap().name, "test_fn");
        assert!(state.base_content_hash().is_none());
    }

    #[test]
    fn test_symbol_state_active_with_base() {
        let symbol = make_test_symbol("test_fn");
        let state = SymbolState::active_with_base(symbol, "abc123".to_string());

        assert!(state.is_active());
        assert_eq!(state.base_content_hash(), Some("abc123"));
    }

    #[test]
    fn test_symbol_state_deleted() {
        let state = SymbolState::deleted("hash123".to_string());

        assert!(state.is_deleted());
        assert!(!state.is_active());
        assert!(state.as_symbol().is_none());
    }

    #[test]
    fn test_symbol_state_transitions() {
        // Start with active
        let symbol = make_test_symbol("my_func");
        let active = SymbolState::active(symbol);
        assert!(active.is_active());

        // Transition to deleted
        let deleted = SymbolState::deleted("hash".to_string());
        assert!(deleted.is_deleted());

        // Can create new active to "undelete"
        let symbol2 = make_test_symbol("my_func");
        let reactivated = SymbolState::active(symbol2);
        assert!(reactivated.is_active());
    }

    // ------------------------------------------------------------------------
    // FileMove Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_file_move_creation() {
        let mv = FileMove::new(
            PathBuf::from("src/old.rs"),
            PathBuf::from("src/new.rs"),
        );

        assert_eq!(mv.from_path, PathBuf::from("src/old.rs"));
        assert_eq!(mv.to_path, PathBuf::from("src/new.rs"));
        assert!(mv.commit_sha.is_none());
        assert!(mv.moved_at > 0);
    }

    #[test]
    fn test_file_move_with_commit() {
        let mv = FileMove::with_commit(
            PathBuf::from("src/old.rs"),
            PathBuf::from("src/new.rs"),
            "abc123".to_string(),
        );

        assert_eq!(mv.commit_sha, Some("abc123".to_string()));
    }

    // ------------------------------------------------------------------------
    // Overlay Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_overlay_create_empty() {
        let overlay = Overlay::new(LayerKind::Working);

        assert!(overlay.is_empty());
        assert_eq!(overlay.kind(), Some(LayerKind::Working));
        assert_eq!(overlay.active_count(), 0);
        assert_eq!(overlay.deleted_count(), 0);
    }

    #[test]
    fn test_overlay_upsert_symbol() {
        let mut overlay = Overlay::new(LayerKind::Working);
        let symbol = make_test_symbol("test_fn");
        let hash = "hash123".to_string();

        let prev = overlay.upsert(hash.clone(), SymbolState::active(symbol));

        assert!(prev.is_none()); // First insert
        assert_eq!(overlay.active_count(), 1);
        assert!(overlay.get(&hash).is_some());
    }

    #[test]
    fn test_overlay_upsert_updates_existing() {
        let mut overlay = Overlay::new(LayerKind::Working);
        let hash = "hash123".to_string();

        // First insert
        let symbol1 = make_test_symbol("test_fn_v1");
        overlay.upsert(hash.clone(), SymbolState::active(symbol1));

        // Update
        let symbol2 = make_test_symbol("test_fn_v2");
        let prev = overlay.upsert(hash.clone(), SymbolState::active(symbol2));

        assert!(prev.is_some());
        assert_eq!(prev.unwrap().as_symbol().unwrap().name, "test_fn_v1");
        assert_eq!(overlay.get(&hash).unwrap().as_symbol().unwrap().name, "test_fn_v2");
    }

    #[test]
    fn test_overlay_delete_symbol() {
        let mut overlay = Overlay::new(LayerKind::Working);
        let symbol = make_test_symbol("test_fn");
        let hash = "hash123".to_string();

        overlay.upsert(hash.clone(), SymbolState::active(symbol));
        assert!(!overlay.is_deleted(&hash));

        overlay.delete(&hash);

        assert!(overlay.is_deleted(&hash));
        assert!(overlay.get(&hash).unwrap().is_deleted());
    }

    #[test]
    fn test_overlay_file_move_tracking() {
        let mut overlay = Overlay::new(LayerKind::Branch);

        overlay.record_move(
            PathBuf::from("src/old.rs"),
            PathBuf::from("src/new.rs"),
        );

        assert_eq!(overlay.moves.len(), 1);

        // Resolve old path to new path
        let resolved = overlay.resolve_path(&PathBuf::from("src/old.rs"));
        assert_eq!(resolved, PathBuf::from("src/new.rs"));

        // Unaffected path stays the same
        let other = overlay.resolve_path(&PathBuf::from("src/other.rs"));
        assert_eq!(other, PathBuf::from("src/other.rs"));
    }

    #[test]
    fn test_overlay_chained_moves() {
        let mut overlay = Overlay::new(LayerKind::Branch);

        // Move A -> B -> C
        overlay.record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        overlay.record_move(PathBuf::from("b.rs"), PathBuf::from("c.rs"));

        let resolved = overlay.resolve_path(&PathBuf::from("a.rs"));
        assert_eq!(resolved, PathBuf::from("c.rs"));
    }

    #[test]
    fn test_overlay_serialize_deserialize() {
        let mut overlay = Overlay::new(LayerKind::Working);
        let symbol = make_test_symbol("test_fn");
        overlay.upsert("hash123".to_string(), SymbolState::active(symbol));
        overlay.delete("hash456");
        overlay.record_move(PathBuf::from("old.rs"), PathBuf::from("new.rs"));

        // Serialize
        let json = serde_json::to_string(&overlay).expect("serialize");

        // Deserialize
        let restored: Overlay = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.active_count(), 1);
        assert!(restored.is_deleted("hash456"));
        assert_eq!(restored.moves.len(), 1);
    }

    // ------------------------------------------------------------------------
    // LayeredIndex Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_layered_index_creation() {
        let index = LayeredIndex::new();

        assert_eq!(index.layer(LayerKind::Base).kind(), Some(LayerKind::Base));
        assert_eq!(index.layer(LayerKind::Branch).kind(), Some(LayerKind::Branch));
        assert_eq!(index.layer(LayerKind::Working).kind(), Some(LayerKind::Working));
        assert_eq!(index.layer(LayerKind::AI).kind(), Some(LayerKind::AI));
    }

    #[test]
    fn test_layered_index_resolve_symbol_from_base() {
        let mut index = LayeredIndex::new();
        let symbol = make_test_symbol("base_fn");
        let hash = "hash_base".to_string();

        index.base.upsert(hash.clone(), SymbolState::active(symbol));

        let resolved = index.resolve_symbol(&hash);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name, "base_fn");
    }

    #[test]
    fn test_layered_index_ai_shadows_base() {
        let mut index = LayeredIndex::new();
        let hash = "shared_hash".to_string();

        // Add to base
        let base_symbol = make_test_symbol("base_version");
        index.base.upsert(hash.clone(), SymbolState::active(base_symbol));

        // Add to AI (should shadow base)
        let ai_symbol = make_test_symbol("ai_version");
        index.ai.upsert(hash.clone(), SymbolState::active(ai_symbol));

        let resolved = index.resolve_symbol(&hash);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name, "ai_version");
    }

    #[test]
    fn test_layered_index_working_shadows_branch() {
        let mut index = LayeredIndex::new();
        let hash = "shared_hash".to_string();

        // Add to branch
        let branch_symbol = make_test_symbol("branch_version");
        index.branch.upsert(hash.clone(), SymbolState::active(branch_symbol));

        // Add to working (should shadow branch)
        let working_symbol = make_test_symbol("working_version");
        index.working.upsert(hash.clone(), SymbolState::active(working_symbol));

        let resolved = index.resolve_symbol(&hash);
        assert_eq!(resolved.unwrap().name, "working_version");
    }

    #[test]
    fn test_layered_index_deleted_in_higher_layer_hides_lower() {
        let mut index = LayeredIndex::new();
        let hash = "to_delete".to_string();

        // Add to base
        let symbol = make_test_symbol("base_fn");
        index.base.upsert(hash.clone(), SymbolState::active(symbol));

        // Verify it exists
        assert!(index.resolve_symbol(&hash).is_some());

        // Delete in working layer
        index.working.delete(&hash);

        // Should now be hidden
        assert!(index.resolve_symbol(&hash).is_none());
    }

    #[test]
    fn test_layered_index_symbol_exists() {
        let mut index = LayeredIndex::new();
        let hash = "test_hash".to_string();

        assert!(!index.symbol_exists(&hash));

        let symbol = make_test_symbol("test_fn");
        index.base.upsert(hash.clone(), SymbolState::active(symbol));

        assert!(index.symbol_exists(&hash));
    }

    #[test]
    fn test_layered_index_all_symbol_hashes() {
        let mut index = LayeredIndex::new();

        // Add symbols to different layers
        index.base.upsert("base1".to_string(), SymbolState::active(make_test_symbol("fn1")));
        index.base.upsert("base2".to_string(), SymbolState::active(make_test_symbol("fn2")));
        index.branch.upsert("branch1".to_string(), SymbolState::active(make_test_symbol("fn3")));
        index.working.upsert("working1".to_string(), SymbolState::active(make_test_symbol("fn4")));

        // Delete one
        index.ai.delete("base2");

        let hashes = index.all_symbol_hashes();

        assert!(hashes.contains("base1"));
        assert!(!hashes.contains("base2")); // Deleted
        assert!(hashes.contains("branch1"));
        assert!(hashes.contains("working1"));
        assert_eq!(hashes.len(), 3);
    }

    #[test]
    fn test_layered_index_resolve_path_through_layers() {
        let mut index = LayeredIndex::new();

        // Move in branch layer
        index.branch.record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));

        // Move in working layer
        index.working.record_move(PathBuf::from("b.rs"), PathBuf::from("c.rs"));

        // Resolve through all layers
        let resolved = index.resolve_path(&PathBuf::from("a.rs"));
        assert_eq!(resolved, PathBuf::from("c.rs"));
    }

    #[test]
    fn test_layered_index_clear_layer() {
        let mut index = LayeredIndex::new();

        index.working.upsert("hash1".to_string(), SymbolState::active(make_test_symbol("fn1")));
        assert_eq!(index.working.active_count(), 1);

        index.clear_layer(LayerKind::Working);

        assert_eq!(index.working.active_count(), 0);
        assert!(index.working.is_empty());
    }

    #[test]
    fn test_layered_index_stats() {
        let mut index = LayeredIndex::new();

        index.base.upsert("b1".to_string(), SymbolState::active(make_test_symbol("fn1")));
        index.base.upsert("b2".to_string(), SymbolState::active(make_test_symbol("fn2")));
        index.branch.upsert("br1".to_string(), SymbolState::active(make_test_symbol("fn3")));
        index.working.delete("b1");
        index.ai.record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));

        let stats = index.stats();

        assert_eq!(stats.base_symbols, 2);
        assert_eq!(stats.branch_symbols, 1);
        assert_eq!(stats.working_symbols, 0);
        assert_eq!(stats.ai_symbols, 0);
        assert_eq!(stats.total_deleted, 1);
        assert_eq!(stats.total_moves, 1);
    }

    // ------------------------------------------------------------------------
    // Content Hash Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_compute_symbol_hash_deterministic() {
        let symbol = make_test_symbol("test_fn");

        let hash1 = compute_symbol_hash(&symbol, "src/lib.rs");
        let hash2 = compute_symbol_hash(&symbol, "src/lib.rs");

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_symbol_hash_different_files_same_namespace() {
        let symbol = make_test_symbol("test_fn");

        // Same namespace (both under components)
        let hash1 = compute_symbol_hash(&symbol, "src/components/a.rs");
        let hash2 = compute_symbol_hash(&symbol, "lib/components/b.rs");

        // Should be the same since namespace is "components" for both
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_changes_with_content() {
        let symbol1 = SymbolInfo {
            name: "test_fn".to_string(),
            kind: SymbolKind::Function,
            arguments: vec![Argument {
                name: "x".to_string(),
                arg_type: Some("i32".to_string()),
                default_value: None,
            }],
            ..Default::default()
        };

        let symbol2 = SymbolInfo {
            name: "test_fn".to_string(),
            kind: SymbolKind::Function,
            arguments: vec![
                Argument {
                    name: "x".to_string(),
                    arg_type: Some("i32".to_string()),
                    default_value: None,
                },
                Argument {
                    name: "y".to_string(),
                    arg_type: Some("i32".to_string()),
                    default_value: None,
                },
            ],
            ..Default::default()
        };

        let hash1 = compute_content_hash(&symbol1);
        let hash2 = compute_content_hash(&symbol2);

        assert_ne!(hash1, hash2, "Content hash should change when arguments change");
    }

    // ------------------------------------------------------------------------
    // LayerMeta Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_layer_meta_creation() {
        let meta = LayerMeta::new(LayerKind::Working);

        assert_eq!(meta.kind, Some(LayerKind::Working));
        assert!(meta.created_at > 0);
        assert!(meta.updated_at > 0);
        assert_eq!(meta.symbol_count, 0);
    }

    #[test]
    fn test_layer_meta_touch() {
        let mut meta = LayerMeta::new(LayerKind::Working);
        let original = meta.updated_at;

        // Sleep briefly to ensure time advances (in real tests)
        // For unit tests, we just verify the method works
        meta.touch();

        assert!(meta.updated_at >= original);
    }

    // ========================================================================
    // BUG DETECTION TESTS - These tests expose bugs in the current implementation
    // ========================================================================

    // ------------------------------------------------------------------------
    // Bug #1: File index uses symbol.name instead of actual file path
    // The symbols_by_file index incorrectly uses symbol.name (e.g., "my_function")
    // as the key instead of the actual file path (e.g., "src/lib.rs")
    // ------------------------------------------------------------------------

    #[test]
    fn test_file_index_uses_correct_path() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Create a symbol named "calculate_total" that lives in "src/math.rs"
        let symbol = SymbolInfo {
            name: "calculate_total".to_string(),
            kind: SymbolKind::Function,
            start_line: 10,
            end_line: 20,
            is_exported: true,
            behavioral_risk: RiskLevel::Low,
            ..Default::default()
        };

        let hash = "hash_calc".to_string();
        let file_path = PathBuf::from("src/math.rs");
        // Use active_at() to specify the file path
        overlay.upsert(hash.clone(), SymbolState::active_at(symbol, file_path.clone()));

        // Now we can look up symbols by their actual file path
        let symbols = overlay.get_file_symbols(&file_path);

        assert!(
            !symbols.is_empty(),
            "get_file_symbols() should find symbols by file path when using active_at()"
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].as_symbol().unwrap().name, "calculate_total");
    }

    #[test]
    fn test_file_index_correct_after_rebuild() {
        let mut overlay = Overlay::new(LayerKind::Working);

        let symbol = SymbolInfo {
            name: "process_data".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 50,
            is_exported: true,
            behavioral_risk: RiskLevel::Medium,
            ..Default::default()
        };

        let file_path = PathBuf::from("src/processor.rs");
        overlay.upsert(
            "hash_process".to_string(),
            SymbolState::active_at(symbol, file_path.clone()),
        );

        // Rebuild the file index
        overlay.rebuild_file_index();

        // After rebuild, looking up by actual file path should work
        let symbols = overlay.get_file_symbols(&file_path);

        assert!(
            !symbols.is_empty(),
            "rebuild_file_index() should correctly rebuild the file index using stored file paths"
        );
    }

    // ------------------------------------------------------------------------
    // Bug #3 (FIXED): delete() now correctly returns whether symbol existed
    // ------------------------------------------------------------------------

    #[test]
    fn test_delete_returns_false_for_nonexistent() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Try to delete a symbol that was never added
        let result = overlay.delete("nonexistent_hash_12345");

        // delete() should return false when the symbol didn't exist
        assert!(
            !result,
            "delete() should return false when deleting a non-existent symbol"
        );
    }

    #[test]
    fn test_delete_returns_true_for_existing() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add a symbol first
        let symbol = make_test_symbol("my_func");
        overlay.upsert("hash123".to_string(), SymbolState::active(symbol));

        // Now delete it
        let result = overlay.delete("hash123");

        // delete() should return true when the symbol existed
        assert!(
            result,
            "delete() should return true when deleting an existing symbol"
        );
    }

    // ------------------------------------------------------------------------
    // Bug #7 (FIXED): symbols_by_file is now auto-rebuilt after deserialization
    // ------------------------------------------------------------------------

    #[test]
    fn test_symbols_by_file_rebuilt_after_deserialize() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add a symbol with a file path
        let symbol = make_test_symbol("my_func");
        let file_path = PathBuf::from("src/lib.rs");
        overlay.upsert(
            "hash123".to_string(),
            SymbolState::active_at(symbol, file_path.clone()),
        );

        // Also add a deletion and a move
        overlay.delete("some_other_hash");
        overlay.record_move(PathBuf::from("old.rs"), PathBuf::from("new.rs"));

        // Verify the index has something before serialization
        assert!(
            !overlay.symbols_by_file.is_empty(),
            "Precondition: symbols_by_file should have entries before serialize"
        );

        // Serialize and deserialize
        let json = serde_json::to_string(&overlay).expect("serialize");
        let restored: Overlay = serde_json::from_str(&json).expect("deserialize");

        // After deserialization, symbols_by_file should be automatically rebuilt
        assert!(
            !restored.symbols_by_file.is_empty(),
            "symbols_by_file should be automatically rebuilt after deserialization"
        );

        // Verify we can look up symbols by file path
        let symbols = restored.get_file_symbols(&file_path);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].as_symbol().unwrap().name, "my_func");
    }

    // ------------------------------------------------------------------------
    // Bug #12 (FIXED): 'deleted' and 'moves' fields now have serde(default)
    // ------------------------------------------------------------------------

    #[test]
    fn test_deserialize_with_missing_deleted_field() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add only an active symbol (no deletions) and a move
        let symbol = make_test_symbol("my_func");
        overlay.upsert("hash123".to_string(), SymbolState::active(symbol));
        overlay.record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));

        // Serialize - the 'deleted' field will be omitted because it's empty
        let json = serde_json::to_string(&overlay).expect("serialize");

        // Verify 'deleted' is not in the JSON
        assert!(
            !json.contains("\"deleted\""),
            "Precondition: 'deleted' should not be in JSON when empty"
        );

        // Deserialization should work even when 'deleted' is missing
        let result: Result<Overlay, _> = serde_json::from_str(&json);
        assert!(
            result.is_ok(),
            "Deserialization should work when 'deleted' field is missing. Error: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_deserialize_with_missing_moves_field() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add a symbol and a deletion (no moves)
        let symbol = make_test_symbol("my_func");
        overlay.upsert("hash123".to_string(), SymbolState::active(symbol));
        overlay.delete("some_hash");

        // Serialize - the 'moves' field will be omitted because it's empty
        let json = serde_json::to_string(&overlay).expect("serialize");

        // Verify 'moves' is not in the JSON
        assert!(
            !json.contains("\"moves\""),
            "Precondition: 'moves' should not be in JSON when empty"
        );

        // Deserialization should work even when 'moves' is missing
        let result: Result<Overlay, _> = serde_json::from_str(&json);
        assert!(
            result.is_ok(),
            "Deserialization should work when 'moves' field is missing. Error: {:?}",
            result.err()
        );
    }
}
