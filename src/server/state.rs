//! Thread-safe server state management (SEM-99)
//!
//! This module provides thread-safe wrappers around `LayeredIndex` using
//! `parking_lot` for faster, more efficient locking than `std::sync`.
//!
//! # Thread Safety
//!
//! - Uses `RwLock` for the index: concurrent reads, exclusive writes
//! - Uses `Mutex` for mutable metadata (cache paths, status)
//! - `parking_lot` provides:
//!   - No poisoning (simpler error handling)
//!   - Faster uncontended locks
//!   - Fair scheduling under contention
//!
//! # Performance Considerations
//!
//! - Read operations don't block each other
//! - Write operations are exclusive but don't starve readers
//! - Lock guards are held for minimal duration
//! - I/O is performed outside lock scope

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::cache::CacheDir;
use crate::drift::{DriftDetector, DriftStatus, UpdateStrategy};
use crate::overlay::{LayerKind, LayeredIndex, LayeredIndexStats, SymbolState};
use crate::schema::SymbolInfo;

/// Status of a specific layer
#[derive(Debug, Clone)]
pub struct LayerStatus {
    /// Layer kind
    pub kind: LayerKind,
    /// Whether the layer is stale
    pub is_stale: bool,
    /// Recommended update strategy
    pub strategy: UpdateStrategy,
    /// Last update timestamp
    pub last_updated: Option<Instant>,
    /// Number of symbols in this layer
    pub symbol_count: usize,
}

impl LayerStatus {
    /// Create a fresh layer status
    pub fn fresh(kind: LayerKind, symbol_count: usize) -> Self {
        Self {
            kind,
            is_stale: false,
            strategy: UpdateStrategy::Fresh,
            last_updated: Some(Instant::now()),
            symbol_count,
        }
    }

    /// Create a stale layer status
    pub fn stale(kind: LayerKind, symbol_count: usize, strategy: UpdateStrategy) -> Self {
        Self {
            kind,
            is_stale: true,
            strategy,
            last_updated: None,
            symbol_count,
        }
    }
}

/// Overall server status
#[derive(Debug, Clone)]
pub struct ServerStatus {
    /// Repository root path
    pub repo_root: PathBuf,
    /// Whether the server is running
    pub is_running: bool,
    /// Layer statuses
    pub layers: [LayerStatus; 4],
    /// Server uptime
    pub uptime: Duration,
    /// Start time
    start_time: Instant,
}

impl ServerStatus {
    /// Create a new server status
    pub fn new(repo_root: PathBuf) -> Self {
        let now = Instant::now();
        Self {
            repo_root,
            is_running: false,
            layers: [
                LayerStatus::fresh(LayerKind::Base, 0),
                LayerStatus::fresh(LayerKind::Branch, 0),
                LayerStatus::fresh(LayerKind::Working, 0),
                LayerStatus::fresh(LayerKind::AI, 0),
            ],
            uptime: Duration::ZERO,
            start_time: now,
        }
    }

    /// Update uptime
    pub fn update_uptime(&mut self) {
        self.uptime = self.start_time.elapsed();
    }

    /// Mark server as running
    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
        if running {
            self.start_time = Instant::now();
        }
    }

    /// Update a layer's status
    pub fn update_layer(&mut self, kind: LayerKind, status: LayerStatus) {
        self.layers[kind as usize] = status;
    }

    /// Get a layer's status
    pub fn layer(&self, kind: LayerKind) -> &LayerStatus {
        &self.layers[kind as usize]
    }
}

/// Thread-safe server state (SEM-99)
///
/// This is the main state container for the persistent semantic index server.
/// It wraps `LayeredIndex` in `Arc<RwLock<>>` for thread-safe concurrent access.
///
/// # Example
///
/// ```ignore
/// use semfora_engine::server::ServerState;
/// use std::path::PathBuf;
///
/// let state = ServerState::new(PathBuf::from("/path/to/repo"));
///
/// // Concurrent reads (don't block each other)
/// let symbol = state.read(|index| {
///     index.resolve_symbol("abc123").cloned()
/// });
///
/// // Exclusive writes
/// state.write(|index| {
///     index.clear_layer(LayerKind::Working);
/// });
/// ```
#[derive(Clone)]
pub struct ServerState {
    /// Thread-safe layered index
    ///
    /// Uses RwLock for concurrent reads, exclusive writes.
    /// LOCKING ORDER: Always acquire this lock first.
    index: Arc<RwLock<LayeredIndex>>,

    /// Cache directory for persistence
    ///
    /// LOCKING ORDER: Acquire after index.
    cache_dir: Arc<Mutex<Option<CacheDir>>>,

    /// Server status
    ///
    /// LOCKING ORDER: Acquire last.
    status: Arc<Mutex<ServerStatus>>,

    /// Repository root path (immutable after creation)
    repo_root: PathBuf,
}

impl ServerState {
    /// Create a new server state for a repository
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            index: Arc::new(RwLock::new(LayeredIndex::new())),
            cache_dir: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(ServerStatus::new(repo_root.clone()))),
            repo_root,
        }
    }

    /// Create with an existing layered index
    pub fn with_index(repo_root: PathBuf, index: LayeredIndex) -> Self {
        let mut status = ServerStatus::new(repo_root.clone());
        // Initialize layer statuses from index stats
        let stats = index.stats();
        status.layers[0].symbol_count = stats.base_symbols;
        status.layers[1].symbol_count = stats.branch_symbols;
        status.layers[2].symbol_count = stats.working_symbols;
        status.layers[3].symbol_count = stats.ai_symbols;

        Self {
            index: Arc::new(RwLock::new(index)),
            cache_dir: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(status)),
            repo_root,
        }
    }

    /// Create with cache directory
    pub fn with_cache_dir(repo_root: PathBuf, cache: CacheDir) -> Self {
        Self {
            index: Arc::new(RwLock::new(LayeredIndex::new())),
            cache_dir: Arc::new(Mutex::new(Some(cache))),
            status: Arc::new(Mutex::new(ServerStatus::new(repo_root.clone()))),
            repo_root,
        }
    }

    /// Get the repository root path
    pub fn repo_root(&self) -> &PathBuf {
        &self.repo_root
    }

    // ========================================================================
    // Read Operations (concurrent access)
    // ========================================================================

    /// Read from the index with a closure
    ///
    /// Multiple readers can access the index concurrently.
    /// This is the preferred way to read from the index.
    pub fn read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&LayeredIndex) -> R,
    {
        let guard = self.index.read();
        f(&guard)
    }

    /// Try to read from the index without blocking
    ///
    /// Returns None if a write lock is held.
    pub fn try_read<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&LayeredIndex) -> R,
    {
        self.index.try_read().map(|guard| f(&guard))
    }

    /// Get a read guard for complex operations
    ///
    /// Prefer `read()` for simple operations.
    /// Use this when you need to hold the lock across multiple method calls.
    pub fn read_guard(&self) -> RwLockReadGuard<'_, LayeredIndex> {
        self.index.read()
    }

    /// Resolve a symbol by hash (convenience method)
    pub fn resolve_symbol(&self, hash: &str) -> Option<SymbolInfo> {
        self.read(|index| index.resolve_symbol(hash).cloned())
    }

    /// Check if a symbol exists (convenience method)
    pub fn symbol_exists(&self, hash: &str) -> bool {
        self.read(|index| index.symbol_exists(hash))
    }

    /// Get index statistics (convenience method)
    pub fn stats(&self) -> LayeredIndexStats {
        self.read(|index| index.stats())
    }

    // ========================================================================
    // Write Operations (exclusive access)
    // ========================================================================

    /// Write to the index with a closure
    ///
    /// Only one writer can access the index at a time.
    /// Writers have exclusive access (no concurrent readers).
    pub fn write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut LayeredIndex) -> R,
    {
        let mut guard = self.index.write();
        let result = f(&mut guard);
        // Update status after write
        drop(guard); // Release index lock before acquiring status lock
        self.update_status_from_index();
        result
    }

    /// Try to write to the index without blocking
    ///
    /// Returns None if any lock (read or write) is held.
    pub fn try_write<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut LayeredIndex) -> R,
    {
        self.index.try_write().map(|mut guard| {
            let result = f(&mut guard);
            drop(guard);
            self.update_status_from_index();
            result
        })
    }

    /// Get a write guard for complex operations
    ///
    /// Prefer `write()` for simple operations.
    /// Use this when you need to hold the lock across multiple method calls.
    ///
    /// WARNING: Remember to call `update_status_from_index()` after dropping
    /// the guard if you modified symbol counts.
    pub fn write_guard(&self) -> RwLockWriteGuard<'_, LayeredIndex> {
        self.index.write()
    }

    /// Clear a specific layer (convenience method)
    pub fn clear_layer(&self, kind: LayerKind) {
        self.write(|index| index.clear_layer(kind));
    }

    /// Upsert a symbol into a layer (convenience method)
    pub fn upsert_symbol(&self, kind: LayerKind, hash: String, state: SymbolState) {
        self.write(|index| {
            index.layer_mut(kind).upsert(hash, state);
        });
    }

    // ========================================================================
    // Status Operations
    // ========================================================================

    /// Get a copy of the current server status
    pub fn status(&self) -> ServerStatus {
        let mut status = self.status.lock().clone();
        status.update_uptime();
        status
    }

    /// Update status from current index state
    fn update_status_from_index(&self) {
        let stats = self.read(|index| index.stats());
        let mut status = self.status.lock();
        status.layers[0].symbol_count = stats.base_symbols;
        status.layers[1].symbol_count = stats.branch_symbols;
        status.layers[2].symbol_count = stats.working_symbols;
        status.layers[3].symbol_count = stats.ai_symbols;
        status.update_uptime();
    }

    /// Mark a layer as stale
    pub fn mark_layer_stale(&self, kind: LayerKind, strategy: UpdateStrategy) {
        let symbol_count = self.read(|index| index.layer(kind).active_count());
        let mut status = self.status.lock();
        status.layers[kind as usize] = LayerStatus::stale(kind, symbol_count, strategy);
    }

    /// Mark a layer as fresh
    pub fn mark_layer_fresh(&self, kind: LayerKind) {
        let symbol_count = self.read(|index| index.layer(kind).active_count());
        let mut status = self.status.lock();
        status.layers[kind as usize] = LayerStatus::fresh(kind, symbol_count);
    }

    /// Set server running state
    pub fn set_running(&self, running: bool) {
        self.status.lock().set_running(running);
    }

    // ========================================================================
    // Cache Operations
    // ========================================================================

    /// Set the cache directory
    pub fn set_cache_dir(&self, cache: CacheDir) {
        *self.cache_dir.lock() = Some(cache);
    }

    /// Get the cache directory path if set
    pub fn cache_path(&self) -> Option<PathBuf> {
        self.cache_dir.lock().as_ref().map(|c| c.root.clone())
    }

    /// Perform a cache operation
    pub fn with_cache<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&CacheDir) -> R,
    {
        self.cache_dir.lock().as_ref().map(f)
    }

    /// Perform a mutable cache operation
    pub fn with_cache_mut<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut CacheDir) -> R,
    {
        self.cache_dir.lock().as_mut().map(f)
    }

    // ========================================================================
    // Drift Detection
    // ========================================================================

    /// Check drift for a specific layer
    ///
    /// This performs I/O (git operations) so it does NOT hold any locks.
    pub fn check_layer_drift(&self, kind: LayerKind) -> Option<DriftStatus> {
        // Read metadata outside lock
        let (indexed_sha, merge_base_sha) = self.read(|index| {
            let layer = index.layer(kind);
            (
                layer.meta.indexed_sha.clone(),
                layer.meta.merge_base_sha.clone(),
            )
        });

        // Get total files for percentage calculation
        let detector = DriftDetector::new(self.repo_root.clone());

        // Perform drift check (I/O operation, no locks held)
        detector
            .check_drift(kind, indexed_sha.as_deref(), merge_base_sha.as_deref())
            .ok()
    }

    /// Check drift for all layers
    pub fn check_all_drift(&self) -> [(LayerKind, Option<DriftStatus>); 4] {
        [
            (LayerKind::Base, self.check_layer_drift(LayerKind::Base)),
            (LayerKind::Branch, self.check_layer_drift(LayerKind::Branch)),
            (
                LayerKind::Working,
                self.check_layer_drift(LayerKind::Working),
            ),
            (LayerKind::AI, self.check_layer_drift(LayerKind::AI)),
        ]
    }
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field("repo_root", &self.repo_root)
            .field("stats", &self.stats())
            .finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_server_state_creation() {
        let state = ServerState::new(PathBuf::from("/tmp/test"));
        assert_eq!(state.repo_root(), &PathBuf::from("/tmp/test"));
        assert_eq!(state.stats().base_symbols, 0);
    }

    #[test]
    fn test_concurrent_reads() {
        let state = Arc::new(ServerState::new(PathBuf::from("/tmp/test")));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    // All reads should succeed concurrently
                    state.read(|index| index.stats())
                })
            })
            .collect();

        for handle in handles {
            let stats = handle.join().unwrap();
            assert_eq!(stats.base_symbols, 0);
        }
    }

    #[test]
    fn test_write_blocks_reads() {
        let state = Arc::new(ServerState::new(PathBuf::from("/tmp/test")));

        // Write operation
        state.write(|index| {
            index.clear_layer(LayerKind::Working);
        });

        // Read should work after write completes
        let stats = state.stats();
        assert_eq!(stats.working_symbols, 0);
    }

    #[test]
    fn test_status_updates() {
        let state = ServerState::new(PathBuf::from("/tmp/test"));

        state.set_running(true);
        assert!(state.status().is_running);

        state.mark_layer_stale(LayerKind::Working, UpdateStrategy::FullRebuild);
        assert!(state.status().layer(LayerKind::Working).is_stale);

        state.mark_layer_fresh(LayerKind::Working);
        assert!(!state.status().layer(LayerKind::Working).is_stale);
    }

    #[test]
    fn test_try_read_non_blocking() {
        let state = ServerState::new(PathBuf::from("/tmp/test"));

        // Should succeed when no write lock is held
        let result = state.try_read(|index| index.stats());
        assert!(result.is_some());
    }

    #[test]
    fn test_layer_status_creation() {
        let fresh = LayerStatus::fresh(LayerKind::Base, 100);
        assert!(!fresh.is_stale);
        assert_eq!(fresh.symbol_count, 100);
        assert!(matches!(fresh.strategy, UpdateStrategy::Fresh));

        let stale = LayerStatus::stale(LayerKind::Working, 50, UpdateStrategy::FullRebuild);
        assert!(stale.is_stale);
        assert_eq!(stale.symbol_count, 50);
        assert!(matches!(stale.strategy, UpdateStrategy::FullRebuild));
    }
}
