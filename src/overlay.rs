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
        [
            LayerKind::AI,
            LayerKind::Working,
            LayerKind::Branch,
            LayerKind::Base,
        ]
    }

    /// Get all layer kinds in order from lowest to highest priority
    pub fn all_ascending() -> [LayerKind; 4] {
        [
            LayerKind::Base,
            LayerKind::Branch,
            LayerKind::Working,
            LayerKind::AI,
        ]
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
    pub fn active_at_with_base(
        symbol: SymbolInfo,
        file_path: PathBuf,
        base_content_hash: String,
    ) -> Self {
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
            Self::Active {
                base_content_hash, ..
            } => base_content_hash.as_deref(),
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
        // Clean up old file index entry if updating an existing symbol
        if let Some(old_state) = self.symbols.get(&hash) {
            if let Some(old_path) = old_state.file_path() {
                // Remove hash from old file path's entry
                if let Some(hashes) = self.symbols_by_file.get_mut(old_path) {
                    hashes.retain(|h| h != &hash);
                    // Clean up empty entries
                    if hashes.is_empty() {
                        self.symbols_by_file.remove(old_path);
                    }
                }
            }
        }

        // Add to new file index if this is an active symbol with a file path
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
            .map(|hashes| hashes.iter().filter_map(|h| self.symbols.get(h)).collect())
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

/// Compute a two-part hash for a symbol
///
/// Returns format: `{file_hash}:{semantic_hash}` (25 chars)
/// - file_hash (8 chars): Hash of file path for uniqueness
/// - semantic_hash (16 chars): Hash of namespace:name:kind:arity for move detection
pub fn compute_symbol_hash(symbol: &SymbolInfo, file_path: &str) -> String {
    // Semantic hash (for move detection and duplicate finding)
    let namespace = crate::schema::SymbolId::namespace_from_path(file_path);
    let semantic_input = format!(
        "{}:{}:{}:{}",
        namespace,
        symbol.name,
        symbol.kind.as_str(),
        symbol.arguments.len() + symbol.props.len()
    );
    let semantic_hash = format!("{:016x}", fnv1a_hash(&semantic_input));

    // File hash (for uniqueness across different files)
    // Truncate to 32 bits (8 hex chars) for compactness
    let file_hash = format!("{:08x}", fnv1a_hash(file_path) as u32);

    // Combined hash: file_hash:semantic_hash
    format!("{}:{}", file_hash, semantic_hash)
}

/// Extract the semantic hash from a full two-part hash
///
/// Given "file_hash:semantic_hash", returns "semantic_hash".
/// For backward compatibility, returns the full hash if no colon is found.
pub fn extract_semantic_hash(full_hash: &str) -> &str {
    full_hash.split(':').nth(1).unwrap_or(full_hash)
}

/// Extract the file hash from a full two-part hash
///
/// Given "file_hash:semantic_hash", returns "file_hash".
/// For backward compatibility, returns empty string if no colon is found.
pub fn extract_file_hash(full_hash: &str) -> &str {
    full_hash
        .split(':')
        .next()
        .filter(|_| full_hash.contains(':'))
        .unwrap_or("")
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
// Layered Query Resolution (SEM-53)
// ============================================================================

/// Options for searching symbols across layers
///
/// All filters are optional. When not specified, all symbols matching
/// the query are returned.
#[derive(Debug, Clone, Default)]
pub struct LayeredSearchOptions {
    /// Filter by symbol kind (e.g., "function", "struct", "component")
    pub kind: Option<String>,
    /// Filter by risk level (e.g., "high", "medium", "low")
    pub risk: Option<String>,
    /// Maximum results to return (default: no limit)
    pub limit: Option<usize>,
    /// Only search in specific layers (default: all layers)
    pub layers: Option<Vec<LayerKind>>,
    /// Case-insensitive search (default: true)
    pub case_insensitive: bool,
}

impl LayeredSearchOptions {
    /// Create new search options with defaults
    #[must_use]
    pub fn new() -> Self {
        Self {
            case_insensitive: true,
            ..Default::default()
        }
    }

    /// Set kind filter
    #[must_use]
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    /// Set risk filter
    #[must_use]
    pub fn with_risk(mut self, risk: impl Into<String>) -> Self {
        self.risk = Some(risk.into());
        self
    }

    /// Set result limit
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set layers to search
    #[must_use]
    pub fn with_layers(mut self, layers: Vec<LayerKind>) -> Self {
        self.layers = Some(layers);
        self
    }

    /// Set case sensitivity
    #[must_use]
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_insensitive = !sensitive;
        self
    }

    /// Check if a layer should be searched
    fn should_search_layer(&self, kind: LayerKind) -> bool {
        match &self.layers {
            Some(layers) => layers.contains(&kind),
            None => true,
        }
    }
}

/// Result from a layered symbol search
///
/// Contains the symbol information along with metadata about
/// which layer it was found in.
#[derive(Debug, Clone)]
pub struct LayeredSearchResult {
    /// Symbol hash (content-addressable identifier)
    pub hash: String,
    /// Full symbol information
    pub symbol: SymbolInfo,
    /// Which layer this symbol was found in
    pub layer: LayerKind,
    /// File path where the symbol is located
    pub file_path: Option<PathBuf>,
}

impl LayeredSearchResult {
    /// Create a new search result
    #[must_use]
    pub fn new(
        hash: String,
        symbol: SymbolInfo,
        layer: LayerKind,
        file_path: Option<PathBuf>,
    ) -> Self {
        Self {
            hash,
            symbol,
            layer,
            file_path,
        }
    }

    /// Get the symbol name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.symbol.name
    }

    /// Get the symbol kind as a string
    #[must_use]
    pub fn kind(&self) -> &str {
        self.symbol.kind.as_str()
    }

    /// Get the risk level as a string
    #[must_use]
    pub fn risk(&self) -> &str {
        self.symbol.behavioral_risk.as_str()
    }

    /// Get the line range as a string (e.g., "45-89")
    #[must_use]
    pub fn lines(&self) -> String {
        format!("{}-{}", self.symbol.start_line, self.symbol.end_line)
    }
}

impl LayeredIndex {
    /// Search for symbols by name across all layers
    ///
    /// Searches for symbols whose names contain the query string.
    /// Results are deduplicated: if the same symbol hash exists in multiple
    /// layers, only the version from the highest-priority layer is returned.
    /// Symbols that are deleted in any higher layer are excluded.
    ///
    /// # Arguments
    /// * `query` - The search query (matched against symbol names)
    /// * `opts` - Search options including filters and limits
    ///
    /// # Returns
    /// A vector of search results, ordered by layer priority (highest first)
    ///
    /// # Example
    /// ```ignore
    /// let index = LayeredIndex::new();
    /// let opts = LayeredSearchOptions::new().with_kind("function").with_limit(10);
    /// let results = index.search_symbols("validate", &opts);
    /// ```
    #[must_use]
    pub fn search_symbols(
        &self,
        query: &str,
        opts: &LayeredSearchOptions,
    ) -> Vec<LayeredSearchResult> {
        let mut results: Vec<LayeredSearchResult> = Vec::new();
        let mut seen_hashes: HashSet<String> = HashSet::new();
        let mut deleted_hashes: HashSet<String> = HashSet::new();

        let query_normalized = if opts.case_insensitive {
            query.to_lowercase()
        } else {
            query.to_string()
        };

        // Iterate layers from highest to lowest priority
        for kind in LayerKind::all_descending() {
            if !opts.should_search_layer(kind) {
                continue;
            }

            let layer = self.layer(kind);

            // First, collect all deleted hashes from this layer
            deleted_hashes.extend(layer.deleted.iter().cloned());

            // Then search for symbols
            for (hash, state) in &layer.symbols {
                // Skip if already seen (higher layer takes precedence)
                if seen_hashes.contains(hash) {
                    continue;
                }

                // Skip if deleted in any higher layer
                if deleted_hashes.contains(hash) {
                    continue;
                }

                // Get the symbol info
                let symbol = match state.as_symbol() {
                    Some(s) => s,
                    None => continue, // Skip deleted entries
                };

                // Match query against symbol name
                let name_normalized = if opts.case_insensitive {
                    symbol.name.to_lowercase()
                } else {
                    symbol.name.clone()
                };

                if !name_normalized.contains(&query_normalized) {
                    continue;
                }

                // Apply kind filter
                if let Some(ref kind_filter) = opts.kind {
                    if symbol.kind.as_str() != kind_filter {
                        continue;
                    }
                }

                // Apply risk filter
                if let Some(ref risk_filter) = opts.risk {
                    if symbol.behavioral_risk.as_str() != risk_filter {
                        continue;
                    }
                }

                // Add to results
                seen_hashes.insert(hash.clone());
                results.push(LayeredSearchResult::new(
                    hash.clone(),
                    symbol.clone(),
                    kind,
                    state.file_path().cloned(),
                ));

                // Check limit
                if let Some(limit) = opts.limit {
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Get all symbols for a file path across all layers
    ///
    /// Resolves the file path through any moves, then collects all symbols
    /// associated with that file. Results are deduplicated: if the same
    /// symbol hash exists in multiple layers, only the highest-priority
    /// version is returned. Deleted symbols are excluded.
    ///
    /// # Arguments
    /// * `path` - The file path to look up (can be original or current path)
    ///
    /// # Returns
    /// A vector of search results for symbols in that file
    ///
    /// # Example
    /// ```ignore
    /// let index = LayeredIndex::new();
    /// let results = index.get_file_symbols(Path::new("src/auth.rs"));
    /// ```
    #[must_use]
    pub fn get_file_symbols(&self, path: &std::path::Path) -> Vec<LayeredSearchResult> {
        let mut results: Vec<LayeredSearchResult> = Vec::new();
        let mut seen_hashes: HashSet<String> = HashSet::new();
        let mut deleted_hashes: HashSet<String> = HashSet::new();

        // Resolve the path through all moves
        let resolved_path = self.resolve_path(&path.to_path_buf());

        // We need to check both the original path and the resolved path
        // in case some layers have old paths and some have new paths
        let paths_to_check: Vec<PathBuf> = if resolved_path == path.to_path_buf() {
            vec![resolved_path]
        } else {
            vec![path.to_path_buf(), resolved_path]
        };

        // Iterate layers from highest to lowest priority
        for kind in LayerKind::all_descending() {
            let layer = self.layer(kind);

            // Collect deleted hashes from this layer
            deleted_hashes.extend(layer.deleted.iter().cloned());

            // Check each path variant
            for check_path in &paths_to_check {
                // Get symbols for this file from the symbols_by_file index
                if let Some(hashes) = layer.symbols_by_file.get(check_path) {
                    for hash in hashes {
                        // Skip if already seen
                        if seen_hashes.contains(hash) {
                            continue;
                        }

                        // Skip if deleted
                        if deleted_hashes.contains(hash) {
                            continue;
                        }

                        // Get the symbol state
                        if let Some(state) = layer.symbols.get(hash) {
                            if let Some(symbol) = state.as_symbol() {
                                seen_hashes.insert(hash.clone());
                                results.push(LayeredSearchResult::new(
                                    hash.clone(),
                                    symbol.clone(),
                                    kind,
                                    state.file_path().cloned(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Resolve a symbol by hash and return with layer info
    ///
    /// Similar to `resolve_symbol` but returns the full result with
    /// layer information.
    ///
    /// # Arguments
    /// * `hash` - The symbol hash to look up
    ///
    /// # Returns
    /// The search result if found, None if not found or deleted
    #[must_use]
    pub fn resolve_symbol_with_layer(&self, hash: &str) -> Option<LayeredSearchResult> {
        for kind in LayerKind::all_descending() {
            let layer = self.layer(kind);

            // Check if deleted in this layer
            if layer.is_deleted(hash) {
                return None;
            }

            // Check if exists in this layer
            if let Some(state) = layer.get(hash) {
                if let Some(symbol) = state.as_symbol() {
                    return Some(LayeredSearchResult::new(
                        hash.to_string(),
                        symbol.clone(),
                        kind,
                        state.file_path().cloned(),
                    ));
                }
            }
        }
        None
    }

    /// Count total active symbols across all layers (with deduplication)
    ///
    /// This is more accurate than summing individual layer counts because
    /// it accounts for shadowing and deletions.
    #[must_use]
    pub fn total_active_symbols(&self) -> usize {
        self.all_symbol_hashes().len()
    }
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
        let mv = FileMove::new(PathBuf::from("src/old.rs"), PathBuf::from("src/new.rs"));

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
        assert_eq!(
            overlay.get(&hash).unwrap().as_symbol().unwrap().name,
            "test_fn_v2"
        );
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

        overlay.record_move(PathBuf::from("src/old.rs"), PathBuf::from("src/new.rs"));

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
        assert_eq!(
            index.layer(LayerKind::Branch).kind(),
            Some(LayerKind::Branch)
        );
        assert_eq!(
            index.layer(LayerKind::Working).kind(),
            Some(LayerKind::Working)
        );
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
        index
            .base
            .upsert(hash.clone(), SymbolState::active(base_symbol));

        // Add to AI (should shadow base)
        let ai_symbol = make_test_symbol("ai_version");
        index
            .ai
            .upsert(hash.clone(), SymbolState::active(ai_symbol));

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
        index
            .branch
            .upsert(hash.clone(), SymbolState::active(branch_symbol));

        // Add to working (should shadow branch)
        let working_symbol = make_test_symbol("working_version");
        index
            .working
            .upsert(hash.clone(), SymbolState::active(working_symbol));

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
        index.base.upsert(
            "base1".to_string(),
            SymbolState::active(make_test_symbol("fn1")),
        );
        index.base.upsert(
            "base2".to_string(),
            SymbolState::active(make_test_symbol("fn2")),
        );
        index.branch.upsert(
            "branch1".to_string(),
            SymbolState::active(make_test_symbol("fn3")),
        );
        index.working.upsert(
            "working1".to_string(),
            SymbolState::active(make_test_symbol("fn4")),
        );

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
        index
            .branch
            .record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));

        // Move in working layer
        index
            .working
            .record_move(PathBuf::from("b.rs"), PathBuf::from("c.rs"));

        // Resolve through all layers
        let resolved = index.resolve_path(&PathBuf::from("a.rs"));
        assert_eq!(resolved, PathBuf::from("c.rs"));
    }

    #[test]
    fn test_layered_index_clear_layer() {
        let mut index = LayeredIndex::new();

        index.working.upsert(
            "hash1".to_string(),
            SymbolState::active(make_test_symbol("fn1")),
        );
        assert_eq!(index.working.active_count(), 1);

        index.clear_layer(LayerKind::Working);

        assert_eq!(index.working.active_count(), 0);
        assert!(index.working.is_empty());
    }

    #[test]
    fn test_layered_index_stats() {
        let mut index = LayeredIndex::new();

        index.base.upsert(
            "b1".to_string(),
            SymbolState::active(make_test_symbol("fn1")),
        );
        index.base.upsert(
            "b2".to_string(),
            SymbolState::active(make_test_symbol("fn2")),
        );
        index.branch.upsert(
            "br1".to_string(),
            SymbolState::active(make_test_symbol("fn3")),
        );
        index.working.delete("b1");
        index
            .ai
            .record_move(PathBuf::from("a.rs"), PathBuf::from("b.rs"));

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

        // Full hashes should be DIFFERENT (different files)
        assert_ne!(hash1, hash2);

        // Semantic hashes should be the SAME (same signature)
        assert_eq!(extract_semantic_hash(&hash1), extract_semantic_hash(&hash2));
    }

    #[test]
    fn test_compute_symbol_hash_unique_per_file_in_same_dir() {
        let symbol = make_test_symbol("enhance");

        // Same directory, same symbol name - the original bug scenario
        let hash1 = compute_symbol_hash(&symbol, "src/frameworks/nextjs.rs");
        let hash2 = compute_symbol_hash(&symbol, "src/frameworks/react.rs");

        // Full hashes should be DIFFERENT (different files)
        assert_ne!(hash1, hash2);

        // Semantic hashes should be the SAME (same signature, enables duplicate detection)
        assert_eq!(extract_semantic_hash(&hash1), extract_semantic_hash(&hash2));
    }

    #[test]
    fn test_compute_symbol_hash_two_part_format() {
        let symbol = make_test_symbol("test_fn");
        let hash = compute_symbol_hash(&symbol, "src/lib.rs");

        // Hash should have format "file_hash:semantic_hash"
        assert!(hash.contains(':'), "Hash should contain colon separator");

        let parts: Vec<&str> = hash.split(':').collect();
        assert_eq!(parts.len(), 2, "Hash should have exactly 2 parts");
        assert_eq!(parts[0].len(), 8, "File hash should be 8 chars");
        assert_eq!(parts[1].len(), 16, "Semantic hash should be 16 chars");
    }

    #[test]
    fn test_extract_semantic_hash() {
        let full_hash = "a1b2c3d4:1234567890abcdef";
        assert_eq!(extract_semantic_hash(full_hash), "1234567890abcdef");

        // Backward compatibility with old 16-char hashes
        let old_hash = "1234567890abcdef";
        assert_eq!(extract_semantic_hash(old_hash), "1234567890abcdef");
    }

    #[test]
    fn test_extract_file_hash() {
        let full_hash = "a1b2c3d4:1234567890abcdef";
        assert_eq!(extract_file_hash(full_hash), "a1b2c3d4");

        // Backward compatibility with old 16-char hashes
        let old_hash = "1234567890abcdef";
        assert_eq!(extract_file_hash(old_hash), "");
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

        assert_ne!(
            hash1, hash2,
            "Content hash should change when arguments change"
        );
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
        overlay.upsert(
            hash.clone(),
            SymbolState::active_at(symbol, file_path.clone()),
        );

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

    // ------------------------------------------------------------------------
    // Copilot Review Fix: upsert() cleans up stale file path entries
    // ------------------------------------------------------------------------

    #[test]
    fn test_upsert_cleans_up_stale_file_path_on_move() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add a symbol at src/old.rs
        let symbol = make_test_symbol("my_func");
        let old_path = PathBuf::from("src/old.rs");
        overlay.upsert(
            "hash123".to_string(),
            SymbolState::active_at(symbol.clone(), old_path.clone()),
        );

        // Verify it's indexed at old path
        assert_eq!(
            overlay.get_file_symbols(&old_path).len(),
            1,
            "Symbol should be indexed at old path"
        );

        // Now "move" the symbol to a new file by upserting with same hash but different path
        let new_path = PathBuf::from("src/new.rs");
        overlay.upsert(
            "hash123".to_string(),
            SymbolState::active_at(symbol, new_path.clone()),
        );

        // The old file path should no longer have any symbols
        let old_symbols = overlay.get_file_symbols(&old_path);
        assert!(
            old_symbols.is_empty(),
            "Old file path should have no symbols after move. Found: {:?}",
            old_symbols
                .iter()
                .map(|s| s.as_symbol().map(|sym| &sym.name))
                .collect::<Vec<_>>()
        );

        // The new file path should have the symbol
        let new_symbols = overlay.get_file_symbols(&new_path);
        assert_eq!(
            new_symbols.len(),
            1,
            "New file path should have exactly one symbol"
        );
        assert_eq!(new_symbols[0].as_symbol().unwrap().name, "my_func");

        // The old path entry should be completely removed from the index (not just empty)
        assert!(
            !overlay.symbols_by_file.contains_key(&old_path),
            "Old file path should be completely removed from symbols_by_file"
        );
    }

    #[test]
    fn test_upsert_does_not_duplicate_on_same_file() {
        let mut overlay = Overlay::new(LayerKind::Working);

        // Add a symbol at src/lib.rs
        let symbol = make_test_symbol("my_func");
        let path = PathBuf::from("src/lib.rs");
        overlay.upsert(
            "hash123".to_string(),
            SymbolState::active_at(symbol.clone(), path.clone()),
        );

        // Update the same symbol at the same path (e.g., changed content)
        let updated_symbol = SymbolInfo {
            name: "my_func".to_string(),
            is_exported: true, // Changed from default false
            ..symbol
        };
        overlay.upsert(
            "hash123".to_string(),
            SymbolState::active_at(updated_symbol, path.clone()),
        );

        // Should still only have one entry, not duplicates
        let symbols = overlay.get_file_symbols(&path);
        assert_eq!(
            symbols.len(),
            1,
            "Should have exactly one symbol, not duplicates. Found: {}",
            symbols.len()
        );
    }

    // ------------------------------------------------------------------------
    // SEM-53: Layered Query Resolution Tests
    // ------------------------------------------------------------------------

    /// Helper to create a test symbol with specific properties
    fn make_test_symbol_with_kind(name: &str, kind: SymbolKind) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind,
            start_line: 1,
            end_line: 10,
            is_exported: true,
            behavioral_risk: RiskLevel::Low,
            ..Default::default()
        }
    }

    /// Helper to create a test symbol with specific risk level
    fn make_test_symbol_with_risk(name: &str, risk: RiskLevel) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 10,
            is_exported: true,
            behavioral_risk: risk,
            ..Default::default()
        }
    }

    // ========================================================================
    // TDD Required Tests (from SEM-53 ticket)
    // ========================================================================

    #[test]
    fn test_ai_layer_shadows_base_in_search() {
        // SEM-53 TDD: test_ai_layer_shadows_base
        let mut index = LayeredIndex::new();
        let hash = "shared_hash".to_string();

        // Add symbol to base layer
        let base_symbol = SymbolInfo {
            name: "validateUser".to_string(),
            kind: SymbolKind::Function,
            start_line: 10,
            end_line: 20,
            ..Default::default()
        };
        index
            .base
            .upsert(hash.clone(), SymbolState::active(base_symbol));

        // Add modified version to AI layer (same hash, different content)
        let ai_symbol = SymbolInfo {
            name: "validateUser".to_string(), // Same name
            kind: SymbolKind::Function,
            start_line: 10,
            end_line: 50, // Different end line (modified)
            ..Default::default()
        };
        index
            .ai
            .upsert(hash.clone(), SymbolState::active(ai_symbol));

        // Search should return AI version, not base
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("validateUser", &opts);

        assert_eq!(
            results.len(),
            1,
            "Should find exactly one result (deduplicated)"
        );
        assert_eq!(results[0].layer, LayerKind::AI, "Should be from AI layer");
        assert_eq!(
            results[0].symbol.end_line, 50,
            "Should have AI layer's content"
        );
    }

    #[test]
    fn test_deleted_marker_stops_search() {
        // SEM-53 TDD: test_deleted_marker_stops_search
        let mut index = LayeredIndex::new();
        let hash = "to_delete".to_string();

        // Add symbol to base layer
        let symbol = make_test_symbol("deletedFunction");
        index.base.upsert(hash.clone(), SymbolState::active(symbol));

        // Verify it exists
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("deletedFunction", &opts);
        assert_eq!(results.len(), 1, "Should find the symbol before deletion");

        // Delete in working layer
        index.working.delete(&hash);

        // Search should now return nothing
        let results = index.search_symbols("deletedFunction", &opts);
        assert!(
            results.is_empty(),
            "Deleted symbol should not appear in search results"
        );
    }

    #[test]
    fn test_file_move_resolves_path_in_get_file_symbols() {
        // SEM-53 TDD: test_file_move_resolves_path
        let mut index = LayeredIndex::new();
        let hash = "moved_symbol".to_string();
        let old_path = PathBuf::from("src/old_auth.rs");
        let new_path = PathBuf::from("src/auth/validator.rs");

        // Add symbol at old path in base layer
        let symbol = make_test_symbol("validateToken");
        index.base.upsert(
            hash.clone(),
            SymbolState::active_at(symbol.clone(), old_path.clone()),
        );

        // Record move in branch layer
        index.branch.record_move(old_path.clone(), new_path.clone());

        // Add updated symbol at new path in working layer
        let updated_symbol = SymbolInfo {
            name: "validateToken".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 30, // Updated
            ..Default::default()
        };
        index.working.upsert(
            hash.clone(),
            SymbolState::active_at(updated_symbol, new_path.clone()),
        );

        // Query by old path should resolve through moves and find the symbol
        let results = index.get_file_symbols(&old_path);
        assert_eq!(
            results.len(),
            1,
            "Should find symbol through move resolution"
        );
        assert_eq!(
            results[0].symbol.end_line, 30,
            "Should return working layer version"
        );
        assert_eq!(results[0].layer, LayerKind::Working);

        // Query by new path should also work
        let results = index.get_file_symbols(&new_path);
        assert_eq!(results.len(), 1, "Should find symbol at new path");
    }

    #[test]
    fn test_search_merges_all_layers() {
        // SEM-53 TDD: test_search_merges_all_layers
        let mut index = LayeredIndex::new();

        // Add different symbols to each layer
        index.base.upsert(
            "base_hash".to_string(),
            SymbolState::active(make_test_symbol("baseFunction")),
        );
        index.branch.upsert(
            "branch_hash".to_string(),
            SymbolState::active(make_test_symbol("branchFunction")),
        );
        index.working.upsert(
            "working_hash".to_string(),
            SymbolState::active(make_test_symbol("workingFunction")),
        );
        index.ai.upsert(
            "ai_hash".to_string(),
            SymbolState::active(make_test_symbol("aiFunction")),
        );

        // Search for "Function" should find all four
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("Function", &opts);

        assert_eq!(results.len(), 4, "Should find symbols from all layers");

        // Verify we got one from each layer
        let layers: Vec<LayerKind> = results.iter().map(|r| r.layer).collect();
        assert!(layers.contains(&LayerKind::Base));
        assert!(layers.contains(&LayerKind::Branch));
        assert!(layers.contains(&LayerKind::Working));
        assert!(layers.contains(&LayerKind::AI));
    }

    #[test]
    fn test_deduplication_across_layers() {
        // SEM-53 TDD: test_deduplication_across_layers
        let mut index = LayeredIndex::new();
        let hash = "shared_hash".to_string();

        // Add same symbol (same hash) to multiple layers with different content
        let base_symbol = SymbolInfo {
            name: "sharedFunc".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 10,
            ..Default::default()
        };
        let branch_symbol = SymbolInfo {
            name: "sharedFunc".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 20, // Modified
            ..Default::default()
        };
        let working_symbol = SymbolInfo {
            name: "sharedFunc".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 30, // Further modified
            ..Default::default()
        };

        index
            .base
            .upsert(hash.clone(), SymbolState::active(base_symbol));
        index
            .branch
            .upsert(hash.clone(), SymbolState::active(branch_symbol));
        index
            .working
            .upsert(hash.clone(), SymbolState::active(working_symbol));

        // Search should return only one result (from highest priority layer)
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("sharedFunc", &opts);

        assert_eq!(results.len(), 1, "Should deduplicate to one result");
        assert_eq!(
            results[0].layer,
            LayerKind::Working,
            "Should be from highest layer"
        );
        assert_eq!(
            results[0].symbol.end_line, 30,
            "Should have working layer content"
        );
    }

    #[test]
    fn test_layer_ordering_correct() {
        // SEM-53 TDD: test_layer_ordering_correct
        // Verify that layers are checked in correct order: AI > Working > Branch > Base

        let mut index = LayeredIndex::new();

        // Add symbols with predictable order markers
        for (i, kind) in LayerKind::all_descending().iter().enumerate() {
            let symbol = SymbolInfo {
                name: format!("func_{}", i),
                kind: SymbolKind::Function,
                start_line: (i + 1) * 100, // Unique marker
                end_line: (i + 1) * 100 + 10,
                ..Default::default()
            };
            index
                .layer_mut(*kind)
                .upsert(format!("hash_{}", i), SymbolState::active(symbol));
        }

        // Search and verify order
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("func", &opts);

        assert_eq!(results.len(), 4);

        // Results should be in layer priority order (AI first, Base last)
        assert_eq!(results[0].layer, LayerKind::AI);
        assert_eq!(results[1].layer, LayerKind::Working);
        assert_eq!(results[2].layer, LayerKind::Branch);
        assert_eq!(results[3].layer, LayerKind::Base);
    }

    // ========================================================================
    // Additional Comprehensive Tests
    // ========================================================================

    #[test]
    fn test_search_empty_query_matches_all() {
        let mut index = LayeredIndex::new();
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("alpha")),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol("beta")),
        );
        index.base.upsert(
            "h3".to_string(),
            SymbolState::active(make_test_symbol("gamma")),
        );

        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("", &opts);

        assert_eq!(results.len(), 3, "Empty query should match all symbols");
    }

    #[test]
    fn test_search_no_matches_returns_empty() {
        let mut index = LayeredIndex::new();
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("alpha")),
        );

        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("nonexistent", &opts);

        assert!(results.is_empty(), "Non-matching query should return empty");
    }

    #[test]
    fn test_search_case_insensitive_by_default() {
        let mut index = LayeredIndex::new();
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("ValidateUser")),
        );

        let opts = LayeredSearchOptions::new();

        // Should match regardless of case
        assert_eq!(index.search_symbols("validateuser", &opts).len(), 1);
        assert_eq!(index.search_symbols("VALIDATEUSER", &opts).len(), 1);
        assert_eq!(index.search_symbols("ValidateUser", &opts).len(), 1);
    }

    #[test]
    fn test_search_case_sensitive_when_requested() {
        let mut index = LayeredIndex::new();
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("ValidateUser")),
        );

        let opts = LayeredSearchOptions::new().case_sensitive(true);

        assert_eq!(index.search_symbols("ValidateUser", &opts).len(), 1);
        assert_eq!(index.search_symbols("validateuser", &opts).len(), 0);
    }

    #[test]
    fn test_search_with_kind_filter() {
        let mut index = LayeredIndex::new();

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol_with_kind(
                "myFunction",
                SymbolKind::Function,
            )),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol_with_kind("MyStruct", SymbolKind::Struct)),
        );
        index.base.upsert(
            "h3".to_string(),
            SymbolState::active(make_test_symbol_with_kind(
                "MyComponent",
                SymbolKind::Component,
            )),
        );

        // Filter by function
        let opts = LayeredSearchOptions::new().with_kind("function");
        let results = index.search_symbols("my", &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "myFunction");

        // Filter by struct
        let opts = LayeredSearchOptions::new().with_kind("struct");
        let results = index.search_symbols("My", &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "MyStruct");
    }

    #[test]
    fn test_search_with_risk_filter() {
        let mut index = LayeredIndex::new();

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol_with_risk("lowRiskFn", RiskLevel::Low)),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol_with_risk("highRiskFn", RiskLevel::High)),
        );

        // Filter by high risk
        let opts = LayeredSearchOptions::new().with_risk("high");
        let results = index.search_symbols("Fn", &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "highRiskFn");

        // Filter by low risk
        let opts = LayeredSearchOptions::new().with_risk("low");
        let results = index.search_symbols("Fn", &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "lowRiskFn");
    }

    #[test]
    fn test_search_with_limit() {
        let mut index = LayeredIndex::new();

        for i in 0..10 {
            index.base.upsert(
                format!("hash_{}", i),
                SymbolState::active(make_test_symbol(&format!("function_{}", i))),
            );
        }

        let opts = LayeredSearchOptions::new().with_limit(3);
        let results = index.search_symbols("function", &opts);

        assert_eq!(results.len(), 3, "Should respect limit");
    }

    #[test]
    fn test_search_with_layer_filter() {
        let mut index = LayeredIndex::new();

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("baseFunc")),
        );
        index.branch.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol("branchFunc")),
        );
        index.working.upsert(
            "h3".to_string(),
            SymbolState::active(make_test_symbol("workingFunc")),
        );

        // Only search base and branch
        let opts =
            LayeredSearchOptions::new().with_layers(vec![LayerKind::Base, LayerKind::Branch]);
        let results = index.search_symbols("Func", &opts);

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"baseFunc"));
        assert!(names.contains(&"branchFunc"));
        assert!(!names.contains(&"workingFunc"));
    }

    #[test]
    fn test_get_file_symbols_basic() {
        let mut index = LayeredIndex::new();
        let path = PathBuf::from("src/auth.rs");

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active_at(make_test_symbol("validateToken"), path.clone()),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active_at(make_test_symbol("refreshToken"), path.clone()),
        );

        let results = index.get_file_symbols(&path);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_file_symbols_with_deletions() {
        let mut index = LayeredIndex::new();
        let path = PathBuf::from("src/auth.rs");

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active_at(make_test_symbol("keepMe"), path.clone()),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active_at(make_test_symbol("deleteMe"), path.clone()),
        );

        // Delete one in working layer
        index.working.delete("h2");

        let results = index.get_file_symbols(&path);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "keepMe");
    }

    #[test]
    fn test_get_file_symbols_with_shadowing() {
        let mut index = LayeredIndex::new();
        let path = PathBuf::from("src/auth.rs");
        let hash = "shared".to_string();

        // Base version
        let base_symbol = SymbolInfo {
            name: "sharedSymbol".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 10,
            ..Default::default()
        };
        index.base.upsert(
            hash.clone(),
            SymbolState::active_at(base_symbol, path.clone()),
        );

        // Working version (shadows base)
        let working_symbol = SymbolInfo {
            name: "sharedSymbol".to_string(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 50, // Modified
            ..Default::default()
        };
        index.working.upsert(
            hash.clone(),
            SymbolState::active_at(working_symbol, path.clone()),
        );

        let results = index.get_file_symbols(&path);
        assert_eq!(results.len(), 1, "Should deduplicate");
        assert_eq!(results[0].layer, LayerKind::Working);
        assert_eq!(results[0].symbol.end_line, 50);
    }

    #[test]
    fn test_get_file_symbols_empty_file() {
        let index = LayeredIndex::new();
        let path = PathBuf::from("src/nonexistent.rs");

        let results = index.get_file_symbols(&path);
        assert!(results.is_empty());
    }

    #[test]
    fn test_resolve_symbol_with_layer_basic() {
        let mut index = LayeredIndex::new();
        let hash = "test_hash";

        index.base.upsert(
            hash.to_string(),
            SymbolState::active(make_test_symbol("testFunc")),
        );

        let result = index.resolve_symbol_with_layer(hash);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.hash, hash);
        assert_eq!(result.layer, LayerKind::Base);
        assert_eq!(result.symbol.name, "testFunc");
    }

    #[test]
    fn test_resolve_symbol_with_layer_shadowing() {
        let mut index = LayeredIndex::new();
        let hash = "test_hash";

        index.base.upsert(
            hash.to_string(),
            SymbolState::active(make_test_symbol("baseVersion")),
        );
        index.branch.upsert(
            hash.to_string(),
            SymbolState::active(make_test_symbol("branchVersion")),
        );

        let result = index.resolve_symbol_with_layer(hash).unwrap();
        assert_eq!(result.layer, LayerKind::Branch);
        assert_eq!(result.symbol.name, "branchVersion");
    }

    #[test]
    fn test_resolve_symbol_with_layer_deleted() {
        let mut index = LayeredIndex::new();
        let hash = "test_hash";

        index.base.upsert(
            hash.to_string(),
            SymbolState::active(make_test_symbol("deletedFunc")),
        );
        index.working.delete(hash);

        let result = index.resolve_symbol_with_layer(hash);
        assert!(result.is_none(), "Deleted symbol should return None");
    }

    #[test]
    fn test_total_active_symbols() {
        let mut index = LayeredIndex::new();

        // Add 3 unique symbols
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("fn1")),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol("fn2")),
        );
        index.branch.upsert(
            "h3".to_string(),
            SymbolState::active(make_test_symbol("fn3")),
        );

        // Shadow one (same hash, different layer)
        index.working.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("fn1_modified")),
        );

        // Delete one
        index.ai.delete("h2");

        // Should have 2 active: h1 (shadowed) and h3
        assert_eq!(index.total_active_symbols(), 2);
    }

    #[test]
    fn test_search_partial_match() {
        let mut index = LayeredIndex::new();

        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(make_test_symbol("validateUserInput")),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(make_test_symbol("validateEmail")),
        );
        index.base.upsert(
            "h3".to_string(),
            SymbolState::active(make_test_symbol("processPayment")),
        );

        let opts = LayeredSearchOptions::new();

        // Partial match at start
        let results = index.search_symbols("validate", &opts);
        assert_eq!(results.len(), 2);

        // Partial match in middle
        let results = index.search_symbols("User", &opts);
        assert_eq!(results.len(), 1);

        // Partial match at end
        let results = index.search_symbols("Input", &opts);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_options_builder_pattern() {
        let opts = LayeredSearchOptions::new()
            .with_kind("function")
            .with_risk("high")
            .with_limit(10)
            .with_layers(vec![LayerKind::Base, LayerKind::Branch])
            .case_sensitive(true);

        assert_eq!(opts.kind, Some("function".to_string()));
        assert_eq!(opts.risk, Some("high".to_string()));
        assert_eq!(opts.limit, Some(10));
        assert_eq!(opts.layers, Some(vec![LayerKind::Base, LayerKind::Branch]));
        assert!(!opts.case_insensitive);
    }

    #[test]
    fn test_layered_search_result_accessors() {
        let symbol = SymbolInfo {
            name: "testFunc".to_string(),
            kind: SymbolKind::Function,
            start_line: 10,
            end_line: 50,
            behavioral_risk: RiskLevel::High,
            ..Default::default()
        };

        let result = LayeredSearchResult::new(
            "hash123".to_string(),
            symbol,
            LayerKind::Working,
            Some(PathBuf::from("src/test.rs")),
        );

        assert_eq!(result.name(), "testFunc");
        assert_eq!(result.kind(), "function");
        assert_eq!(result.risk(), "high");
        assert_eq!(result.lines(), "10-50");
    }

    #[test]
    fn test_multiple_files_same_symbol_name() {
        let mut index = LayeredIndex::new();
        let path1 = PathBuf::from("src/auth/user.rs");
        let path2 = PathBuf::from("src/auth/admin.rs");

        // Same function name in different files (different hashes)
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active_at(make_test_symbol("validate"), path1.clone()),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active_at(make_test_symbol("validate"), path2.clone()),
        );

        // Search should find both
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("validate", &opts);
        assert_eq!(results.len(), 2);

        // File-specific query should find only one
        let results = index.get_file_symbols(&path1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, Some(path1));
    }

    #[test]
    fn test_chained_file_moves() {
        let mut index = LayeredIndex::new();
        let hash = "moved_symbol".to_string();

        // Original location
        let path_a = PathBuf::from("src/a.rs");
        let path_b = PathBuf::from("src/b.rs");
        let path_c = PathBuf::from("src/c.rs");

        // Symbol starts at path_a
        index.base.upsert(
            hash.clone(),
            SymbolState::active_at(make_test_symbol("movedFunc"), path_a.clone()),
        );

        // Move a -> b in branch
        index.branch.record_move(path_a.clone(), path_b.clone());

        // Move b -> c in working
        index.working.record_move(path_b.clone(), path_c.clone());

        // Add symbol at final location in AI
        index.ai.upsert(
            hash.clone(),
            SymbolState::active_at(make_test_symbol("movedFunc"), path_c.clone()),
        );

        // Query by original path should resolve through chain
        let results = index.get_file_symbols(&path_a);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].layer, LayerKind::AI);
    }

    #[test]
    fn test_deletion_in_middle_layer() {
        let mut index = LayeredIndex::new();
        let hash = "test_hash".to_string();

        // Add in base
        index
            .base
            .upsert(hash.clone(), SymbolState::active(make_test_symbol("fn1")));

        // Delete in branch
        index.branch.delete(&hash);

        // Re-add in working (resurrection)
        index.working.upsert(
            hash.clone(),
            SymbolState::active(make_test_symbol("fn1_resurrected")),
        );

        // Should find the working version (resurrection works)
        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("fn1", &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].layer, LayerKind::Working);
        assert_eq!(results[0].symbol.name, "fn1_resurrected");
    }

    #[test]
    fn test_empty_index_search() {
        let index = LayeredIndex::new();

        let opts = LayeredSearchOptions::new();
        let results = index.search_symbols("anything", &opts);

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_respects_all_filters_together() {
        let mut index = LayeredIndex::new();

        // Add various symbols
        index.base.upsert(
            "h1".to_string(),
            SymbolState::active(SymbolInfo {
                name: "validateUser".to_string(),
                kind: SymbolKind::Function,
                behavioral_risk: RiskLevel::High,
                ..Default::default()
            }),
        );
        index.base.upsert(
            "h2".to_string(),
            SymbolState::active(SymbolInfo {
                name: "validateInput".to_string(),
                kind: SymbolKind::Function,
                behavioral_risk: RiskLevel::Low,
                ..Default::default()
            }),
        );
        index.base.upsert(
            "h3".to_string(),
            SymbolState::active(SymbolInfo {
                name: "ValidateConfig".to_string(),
                kind: SymbolKind::Struct,
                behavioral_risk: RiskLevel::High,
                ..Default::default()
            }),
        );

        // Search with all filters: name contains "validate", kind is function, risk is high
        let opts = LayeredSearchOptions::new()
            .with_kind("function")
            .with_risk("high");
        let results = index.search_symbols("validate", &opts);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "validateUser");
    }
}
