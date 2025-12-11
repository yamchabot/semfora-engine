//! Repository registry and context management
//!
//! Manages RepoContext instances for each connected repository.
//! Multiple clients can share the same RepoContext when working on the same repo.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::cache::CacheDir;
use crate::server::watcher::{FileWatcher, WatcherHandle};
use crate::server::ServerState;
use crate::socket_server::protocol::{
    ConnectionInfo, IndexInfo, IndexStatus, WorktreeInfo,
};
use crate::socket_server::worktree::{
    discover_worktrees, get_current_branch, get_default_branch, get_repo_root,
};
use crate::socket_server::indexer::{index_directory, needs_indexing, IndexOptions};

/// Unique identifier for a repository (hash of path or remote URL)
pub type RepoHash = String;

/// Unique identifier for an index
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexId {
    BaseBranch,
    FeatureBranch,
    Working(String), // branch name
    Worktree(PathBuf),
}

impl std::fmt::Display for IndexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexId::BaseBranch => write!(f, "base_branch"),
            IndexId::FeatureBranch => write!(f, "feature_branch"),
            IndexId::Working(branch) => write!(f, "working:{}", branch),
            IndexId::Worktree(path) => write!(f, "worktree:{}", path.display()),
        }
    }
}

/// Event sent through the broadcast channel
#[derive(Debug, Clone)]
pub struct RepoEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

/// Context for a single repository
pub struct RepoContext {
    /// Unique hash for this repo
    pub repo_hash: RepoHash,
    /// Base repository path
    pub base_repo_path: PathBuf,
    /// Default branch (main/master)
    pub base_branch: String,
    /// Current feature branch (if not on main)
    pub feature_branch: Option<String>,
    /// Discovered worktrees
    pub worktrees: RwLock<Vec<WorktreeInfo>>,
    /// Indexes for this repo (indexed by scope)
    pub indexes: RwLock<HashMap<IndexId, Arc<ServerState>>>,
    /// Cache directory for the base repo (used for default queries)
    pub cache_dir: CacheDir,
    /// Per-index cache directories (each index/worktree gets its own cache)
    pub index_caches: RwLock<HashMap<IndexId, CacheDir>>,
    /// Number of connected clients
    pub client_count: AtomicUsize,
    /// Event broadcast channel
    pub event_tx: broadcast::Sender<RepoEvent>,
    /// File watcher handles (one per watched path)
    pub watcher_handles: RwLock<Vec<WatcherHandle>>,
}

impl RepoContext {
    /// Create a new RepoContext for a repository path
    pub async fn new(path: PathBuf) -> anyhow::Result<Self> {
        // Get the actual repo root
        let base_repo_path = get_repo_root(&path).unwrap_or_else(|_| path.clone());

        // Get cache directory for base repo (uses git remote URL hash)
        let cache_dir = CacheDir::for_repo(&base_repo_path)?;
        let repo_hash = cache_dir.repo_hash.clone();

        // Auto-index base repo if no cache exists
        let index_options = IndexOptions::default();
        if needs_indexing(&cache_dir) {
            tracing::info!("No index found for base repo {:?}, triggering auto-index...", base_repo_path);
            match index_directory(&base_repo_path, cache_dir.clone(), &index_options) {
                Ok(result) => {
                    tracing::info!(
                        "Auto-indexed base repo: {} files, {} symbols -> {}",
                        result.files_analyzed,
                        result.symbols_written,
                        result.cache_path.display()
                    );
                }
                Err(e) => {
                    tracing::warn!("Auto-index failed for base repo: {}", e);
                }
            }
        } else {
            tracing::info!("Using existing cache for base repo: {} ({} symbols)",
                cache_dir.repo_hash,
                cache_dir.load_all_symbol_entries().map(|e| e.len()).unwrap_or(0)
            );
        }

        // Determine branches
        let base_branch = get_default_branch(&base_repo_path).unwrap_or_else(|_| "main".to_string());
        let current_branch = get_current_branch(&base_repo_path).unwrap_or_else(|_| base_branch.clone());
        let feature_branch = if current_branch != base_branch {
            Some(current_branch.clone())
        } else {
            None
        };

        // Discover worktrees
        let worktrees = discover_worktrees(&base_repo_path).unwrap_or_default();

        // Create event broadcast channel (capacity 100 events)
        let (event_tx, _) = broadcast::channel(100);

        // Initialize indexes and per-index caches
        let mut indexes = HashMap::new();
        let mut index_caches = HashMap::new();

        // Create the main ServerState for this repo (base branch uses base cache)
        let main_state = ServerState::new(base_repo_path.clone());
        main_state.set_cache_dir(cache_dir.clone());
        indexes.insert(IndexId::BaseBranch, Arc::new(main_state));
        index_caches.insert(IndexId::BaseBranch, cache_dir.clone());

        // If we're on a feature branch, create a feature branch index (uses base cache for now)
        if feature_branch.is_some() {
            let feature_state = ServerState::new(base_repo_path.clone());
            feature_state.set_cache_dir(cache_dir.clone());
            indexes.insert(IndexId::FeatureBranch, Arc::new(feature_state));
            index_caches.insert(IndexId::FeatureBranch, cache_dir.clone());
        }

        // Create working indexes for each worktree - each gets its OWN cache
        for wt in &worktrees {
            // Create a separate cache for this worktree using path-based hash
            let wt_cache = CacheDir::for_worktree(&wt.path)?;

            // Auto-index worktree if no cache exists
            if needs_indexing(&wt_cache) {
                tracing::info!("No index found for worktree {:?}, triggering auto-index...", wt.path);
                match index_directory(&wt.path, wt_cache.clone(), &index_options) {
                    Ok(result) => {
                        tracing::info!(
                            "Auto-indexed worktree: {} files, {} symbols -> {}",
                            result.files_analyzed,
                            result.symbols_written,
                            result.cache_path.display()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Auto-index failed for worktree {:?}: {}", wt.path, e);
                    }
                }
            } else {
                tracing::info!(
                    "Using existing cache for worktree {:?} -> {} ({} symbols)",
                    wt.path,
                    wt_cache.repo_hash,
                    wt_cache.load_all_symbol_entries().map(|e| e.len()).unwrap_or(0)
                );
            }

            let wt_state = ServerState::new(wt.path.clone());
            wt_state.set_cache_dir(wt_cache.clone());

            let index_id = IndexId::Worktree(wt.path.clone());
            indexes.insert(index_id.clone(), Arc::new(wt_state));
            index_caches.insert(index_id, wt_cache);
        }

        // Start file watchers - only for unique paths
        let mut watcher_handles = Vec::new();
        let mut watched_paths = std::collections::HashSet::new();

        // Watch the base repo (with disk cache enabled)
        let base_state = indexes.get(&IndexId::BaseBranch).cloned();
        if let Some(state) = base_state {
            let watcher = FileWatcher::new(base_repo_path.clone());
            match watcher.start_with_cache(state, Some(cache_dir.clone())) {
                Ok(handle) => {
                    tracing::info!("Started file watcher for base repo: {:?}", base_repo_path);
                    watcher_handles.push(handle);
                    watched_paths.insert(base_repo_path.clone());
                }
                Err(e) => {
                    tracing::warn!("Failed to start file watcher for base repo: {}", e);
                }
            }
        }

        // Watch each worktree (skip if already watching this path)
        for wt in &worktrees {
            // Skip if this path is already being watched (e.g., base repo appears in worktree list)
            if watched_paths.contains(&wt.path) {
                tracing::debug!("Skipping duplicate watcher for: {:?}", wt.path);
                continue;
            }

            let index_id = IndexId::Worktree(wt.path.clone());
            let wt_state = indexes.get(&index_id).cloned();
            let wt_cache = index_caches.get(&index_id).cloned();

            if let (Some(state), Some(wt_cache_dir)) = (wt_state, wt_cache) {
                let watcher = FileWatcher::new(wt.path.clone());
                // Each worktree uses its OWN cache
                match watcher.start_with_cache(state, Some(wt_cache_dir)) {
                    Ok(handle) => {
                        tracing::info!("Started file watcher for worktree: {:?}", wt.path);
                        watcher_handles.push(handle);
                        watched_paths.insert(wt.path.clone());
                    }
                    Err(e) => {
                        tracing::warn!("Failed to start file watcher for worktree {:?}: {}", wt.path, e);
                    }
                }
            }
        }

        Ok(Self {
            repo_hash,
            base_repo_path,
            base_branch,
            feature_branch,
            worktrees: RwLock::new(worktrees),
            indexes: RwLock::new(indexes),
            cache_dir,
            index_caches: RwLock::new(index_caches),
            client_count: AtomicUsize::new(0),
            event_tx,
            watcher_handles: RwLock::new(watcher_handles),
        })
    }

    /// Get connection info for a new client
    pub fn connection_info(&self, client_id: String) -> ConnectionInfo {
        let worktrees = self.worktrees.read().clone();
        let indexes = self.get_index_infos();

        ConnectionInfo {
            client_id,
            repo_id: self.repo_hash.clone(),
            base_repo_path: self.base_repo_path.clone(),
            base_branch: self.base_branch.clone(),
            feature_branch: self.feature_branch.clone(),
            worktrees,
            indexes,
        }
    }

    /// Get info about all indexes
    /// Returns info from per-index disk caches
    fn get_index_infos(&self) -> Vec<IndexInfo> {
        let mut infos = Vec::new();

        // Include all indexes with their per-index caches
        let indexes = self.indexes.read();
        let index_caches = self.index_caches.read();

        for (id, state) in indexes.iter() {
            let status = state.status();

            // Get symbol count from this index's specific cache
            let symbol_count = if let Some(cache) = index_caches.get(id) {
                if cache.has_symbol_index() {
                    cache.load_all_symbol_entries()
                        .map(|entries| entries.len())
                        .unwrap_or(0)
                } else {
                    0
                }
            } else {
                // Fallback to base cache for backward compatibility
                if self.cache_dir.has_symbol_index() {
                    self.cache_dir.load_all_symbol_entries()
                        .map(|entries| entries.len())
                        .unwrap_or(0)
                } else {
                    0
                }
            };

            infos.push(IndexInfo {
                id: id.to_string(),
                scope: match id {
                    IndexId::BaseBranch => format!("base_branch ({})", self.base_branch),
                    IndexId::FeatureBranch => {
                        format!("feature_branch ({})", self.feature_branch.as_deref().unwrap_or("unknown"))
                    }
                    IndexId::Working(branch) => format!("working:{}", branch),
                    IndexId::Worktree(path) => {
                        // Get just the directory name for cleaner display
                        let name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("worktree");
                        format!("worktree:{}", name)
                    }
                },
                symbol_count,
                status: if status.is_running {
                    IndexStatus::Updating
                } else {
                    IndexStatus::Ready
                },
            });
        }

        // If no indexes exist but base cache does, show cache info
        if infos.is_empty() && self.cache_dir.exists() && self.cache_dir.has_symbol_index() {
            let symbol_count = self.cache_dir.load_all_symbol_entries()
                .map(|entries| entries.len())
                .unwrap_or(0);

            infos.push(IndexInfo {
                id: "disk_cache".to_string(),
                scope: "base_branch (disk only)".to_string(),
                symbol_count,
                status: IndexStatus::Ready,
            });
        }

        infos
    }

    /// Increment client count
    pub fn add_client(&self) {
        self.client_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement client count, returns true if this was the last client
    pub fn remove_client(&self) -> bool {
        self.client_count.fetch_sub(1, Ordering::SeqCst) == 1
    }

    /// Get current client count
    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::SeqCst)
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<RepoEvent> {
        self.event_tx.subscribe()
    }

    /// Emit an event to all subscribers
    pub fn emit_event(&self, name: String, payload: serde_json::Value) {
        let _ = self.event_tx.send(RepoEvent { name, payload });
    }

    /// Refresh worktree list
    pub fn refresh_worktrees(&self) -> anyhow::Result<()> {
        let worktrees = discover_worktrees(&self.base_repo_path)?;
        *self.worktrees.write() = worktrees;
        Ok(())
    }

    /// Get the cache directory for a specific scope/index (cloned)
    /// Scope can be:
    /// - "base_branch" or "base" -> base repo cache
    /// - "feature_branch" or "feature" -> feature branch cache (same as base for now)
    /// - "worktree:/path/to/worktree" -> specific worktree cache
    /// - Empty or None -> base repo cache (default)
    pub fn get_cache_for_scope(&self, scope: Option<&str>) -> CacheDir {
        let scope_str = scope.unwrap_or("base_branch");

        // Parse the scope to find the matching IndexId
        let index_id = if scope_str.starts_with("worktree:") {
            let path_str = scope_str.strip_prefix("worktree:").unwrap_or("");
            tracing::info!("[get_cache_for_scope] Looking for worktree with path_str={:?}", path_str);

            // Check if it's a full path or just a directory name
            let path = PathBuf::from(path_str);
            if path.is_absolute() {
                // Full path provided
                tracing::info!("[get_cache_for_scope] Using absolute path: {:?}", path);
                Some(IndexId::Worktree(path))
            } else {
                // Just a directory name - search worktrees to find the full path
                let worktrees = self.worktrees.read();
                tracing::info!("[get_cache_for_scope] Searching {} worktrees for match with path_str={:?}", worktrees.len(), path_str);
                for wt in worktrees.iter() {
                    let dir_name = wt.path.file_name().and_then(|n| n.to_str());
                    tracing::info!("[get_cache_for_scope] Worktree: {:?}, dir_name={:?}", wt.path, dir_name);
                }

                // First try exact match
                let found = worktrees.iter().find(|wt| {
                    wt.path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|name| name == path_str)
                        .unwrap_or(false)
                });

                // If no exact match, try partial match (but be careful about order)
                let found = found.or_else(|| {
                    worktrees.iter().find(|wt| {
                        wt.path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|name| path_str.contains(name) || name.contains(path_str))
                            .unwrap_or(false)
                    })
                });

                if let Some(wt) = found {
                    tracing::info!("[get_cache_for_scope] Found matching worktree: {:?}", wt.path);
                } else {
                    tracing::warn!("[get_cache_for_scope] No matching worktree found for {:?}", path_str);
                }
                found.map(|wt| IndexId::Worktree(wt.path.clone()))
            }
        } else if scope_str == "base_branch" || scope_str == "base" {
            Some(IndexId::BaseBranch)
        } else if scope_str == "feature_branch" || scope_str == "feature" {
            Some(IndexId::FeatureBranch)
        } else {
            // Try to match by worktree directory name
            let worktrees = self.worktrees.read();
            let found = worktrees.iter().find(|wt| {
                wt.path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| scope_str.contains(name) || name.contains(scope_str))
                    .unwrap_or(false)
            });
            found.map(|wt| IndexId::Worktree(wt.path.clone()))
        };

        tracing::info!("[get_cache_for_scope] scope={:?} -> index_id={:?}", scope_str, index_id);

        // Look up the cache for this index
        if let Some(ref id) = index_id {
            let index_caches = self.index_caches.read();
            tracing::info!("[get_cache_for_scope] index_caches has {} entries", index_caches.len());
            for (k, v) in index_caches.iter() {
                tracing::info!("[get_cache_for_scope] Cache entry: {:?} -> {:?}", k, v.root);
            }
            if let Some(cache) = index_caches.get(id) {
                tracing::info!("[get_cache_for_scope] Found cache for {:?} at {:?}", id, cache.root);
                cache.clone()
            } else {
                tracing::warn!("[get_cache_for_scope] No cache found for {:?}, falling back to base", id);
                self.cache_dir.clone()
            }
        } else {
            tracing::warn!("[get_cache_for_scope] Could not parse scope {:?}, falling back to base", scope_str);
            self.cache_dir.clone()
        }
    }

    /// Get the cache directory for a specific index ID
    pub fn get_cache_for_index(&self, index_id: &IndexId) -> Option<CacheDir> {
        self.index_caches.read().get(index_id).cloned()
    }
}

/// Global registry of all repo contexts
pub struct RepoRegistry {
    repos: RwLock<HashMap<RepoHash, Arc<RepoContext>>>,
    /// Global event broadcaster for all repos
    event_broadcaster: RwLock<Option<tokio::sync::broadcast::Sender<String>>>,
}

impl RepoRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            repos: RwLock::new(HashMap::new()),
            event_broadcaster: RwLock::new(None),
        }
    }

    /// Set the event broadcaster (called from daemon main)
    pub fn set_event_broadcaster(&self, sender: tokio::sync::broadcast::Sender<String>) {
        *self.event_broadcaster.write() = Some(sender);
    }

    /// Subscribe to events
    pub fn subscribe_events(&self) -> Option<tokio::sync::broadcast::Receiver<String>> {
        self.event_broadcaster.read().as_ref().map(|s| s.subscribe())
    }

    /// Get or create a RepoContext for a path
    pub async fn get_or_create(&self, path: &PathBuf) -> anyhow::Result<Arc<RepoContext>> {
        // First, try to find existing context
        let repo_root = get_repo_root(path).unwrap_or_else(|_| path.clone());
        let cache_dir = CacheDir::for_repo(&repo_root)?;
        let repo_hash = cache_dir.repo_hash.clone();

        // Check if we already have this repo
        {
            let repos = self.repos.read();
            if let Some(ctx) = repos.get(&repo_hash) {
                tracing::info!("Reusing existing RepoContext for {}", repo_hash);
                return Ok(ctx.clone());
            }
        }

        // Create new context
        tracing::info!("Creating new RepoContext for {}", path.display());
        let ctx = Arc::new(RepoContext::new(path.clone()).await?);

        // Store it
        {
            let mut repos = self.repos.write();
            repos.insert(repo_hash.clone(), ctx.clone());
        }

        Ok(ctx)
    }

    /// Remove a repo context if it has no clients
    pub fn maybe_evict(&self, repo_hash: &str) {
        let mut repos = self.repos.write();
        if let Some(ctx) = repos.get(repo_hash) {
            if ctx.client_count() == 0 {
                tracing::info!("Evicting RepoContext for {} (no clients)", repo_hash);
                repos.remove(repo_hash);
            }
        }
    }

    /// Get all active repo hashes
    pub fn active_repos(&self) -> Vec<RepoHash> {
        self.repos.read().keys().cloned().collect()
    }

    /// Get stats about the registry
    pub fn stats(&self) -> RegistryStats {
        let repos = self.repos.read();
        let total_clients: usize = repos.values().map(|r| r.client_count()).sum();
        RegistryStats {
            repo_count: repos.len(),
            total_clients,
        }
    }
}

impl Default for RepoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the registry
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub repo_count: usize,
    pub total_clients: usize,
}
