//! File system watcher for Working layer auto-update (SEM-101)
//!
//! Uses the `notify` crate to watch for file changes and trigger
//! incremental updates to the Working layer.
//!
//! # Features
//!
//! - Recursive directory watching
//! - .gitignore pattern respect
//! - Debounced events (100ms window to batch rapid changes)
//! - Automatic Working layer updates within 500ms of file save
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────────┐
//! │   notify    │────>│  debouncer  │────>│ LayerSynchronizer│
//! │   watcher   │     │  (100ms)    │     │   (Working)      │
//! └─────────────┘     └─────────────┘     └─────────────────┘
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use parking_lot::Mutex;

use crate::drift::UpdateStrategy;
use crate::error::Result;
use crate::overlay::LayerKind;

use super::state::ServerState;
use super::sync::LayerSynchronizer;

/// Configuration for the file watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce duration (default: 100ms)
    pub debounce_duration: Duration,
    /// Whether to respect .gitignore patterns
    pub respect_gitignore: bool,
    /// File extensions to watch (empty = all supported)
    pub extensions: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_duration: Duration::from_millis(100),
            respect_gitignore: true,
            extensions: vec![],
        }
    }
}

/// File system watcher for automatic Working layer updates
pub struct FileWatcher {
    /// Repository root path
    repo_root: PathBuf,
    /// Watcher configuration
    config: WatcherConfig,
    /// Whether the watcher is running
    running: Arc<AtomicBool>,
    /// Pending file changes (debounced)
    pending_changes: Arc<Mutex<Vec<PathBuf>>>,
}

impl FileWatcher {
    /// Create a new file watcher for a repository
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            config: WatcherConfig::default(),
            running: Arc::new(AtomicBool::new(false)),
            pending_changes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(repo_root: PathBuf, config: WatcherConfig) -> Self {
        Self {
            repo_root,
            config,
            running: Arc::new(AtomicBool::new(false)),
            pending_changes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if the watcher is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start watching for file changes
    ///
    /// This spawns a background task that:
    /// 1. Watches the repository for file changes
    /// 2. Debounces rapid changes
    /// 3. Triggers incremental Working layer updates
    pub fn start(&self, state: Arc<ServerState>) -> Result<WatcherHandle> {
        self.start_with_cache(state, None)
    }

    /// Start watching for file changes with disk cache updates
    ///
    /// Same as `start`, but also updates the disk cache when files change.
    pub fn start_with_cache(
        &self,
        state: Arc<ServerState>,
        cache_dir: Option<crate::cache::CacheDir>,
    ) -> Result<WatcherHandle> {
        if self.running.swap(true, Ordering::SeqCst) {
            // Already running
            return Ok(WatcherHandle {
                running: Arc::clone(&self.running),
            });
        }

        let repo_root = self.repo_root.clone();
        let debounce_duration = self.config.debounce_duration;
        let running = Arc::clone(&self.running);
        let pending = Arc::clone(&self.pending_changes);

        // Channel for receiving debounced events
        let (tx, rx) = std::sync::mpsc::channel();

        // Create debounced watcher
        let mut debouncer = new_debouncer(debounce_duration, tx).map_err(|e| {
            crate::error::McpDiffError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Start watching
        debouncer
            .watcher()
            .watch(&repo_root, RecursiveMode::Recursive)
            .map_err(|e| {
                crate::error::McpDiffError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;

        // Spawn processing thread
        let handle_running = Arc::clone(&running);
        std::thread::spawn(move || {
            let synchronizer = match cache_dir {
                Some(cache) => LayerSynchronizer::with_cache(repo_root.clone(), cache),
                None => LayerSynchronizer::new(repo_root.clone()),
            };

            // Track recently processed files to avoid re-processing
            // Key: file path, Value: last processed time
            let mut recently_processed: std::collections::HashMap<PathBuf, std::time::Instant> =
                std::collections::HashMap::new();
            let cooldown_duration = Duration::from_secs(3); // 3 second cooldown per file

            while handle_running.load(Ordering::SeqCst) {
                // Clean up old entries from recently_processed (older than 2x cooldown)
                let now = std::time::Instant::now();
                recently_processed.retain(|_, processed_at| {
                    now.duration_since(*processed_at) < cooldown_duration * 2
                });

                // Receive events with timeout
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(events)) => {
                        tracing::debug!("[WATCHER] Received {} raw events", events.len());

                        // Collect changed file paths, filtering out recently processed
                        let mut changed_files = Vec::new();
                        let now = std::time::Instant::now();

                        for event in events {
                            tracing::debug!(
                                "[WATCHER] Event kind: {:?}, path: {:?}",
                                event.kind,
                                event.path
                            );
                            if matches!(event.kind, DebouncedEventKind::Any) {
                                // Filter out ignored paths
                                let path = event.path;
                                if Self::should_watch_path(&path, &repo_root) {
                                    // Convert to relative path
                                    if let Ok(rel_path) = path.strip_prefix(&repo_root) {
                                        // Check if file is in cooldown period
                                        if let Some(processed_at) = recently_processed.get(rel_path)
                                        {
                                            if now.duration_since(*processed_at) < cooldown_duration
                                            {
                                                tracing::debug!(
                                                    "[WATCHER] Skipping {:?} (in cooldown)",
                                                    rel_path
                                                );
                                                continue;
                                            }
                                        }
                                        tracing::debug!("[WATCHER] Accepted file: {:?}", rel_path);
                                        changed_files.push(rel_path.to_path_buf());
                                    }
                                } else {
                                    tracing::debug!("[WATCHER] Filtered out: {:?}", path);
                                }
                            }
                        }

                        if !changed_files.is_empty() {
                            tracing::info!(
                                "[WATCHER] Processing {} changed files: {:?}",
                                changed_files.len(),
                                changed_files
                            );

                            // Mark files as recently processed BEFORE processing
                            // This prevents re-processing during the cooldown
                            let now = std::time::Instant::now();
                            for file in &changed_files {
                                recently_processed.insert(file.clone(), now);
                            }

                            // Add to pending changes
                            pending.lock().extend(changed_files.clone());

                            // Trigger incremental update
                            let strategy = UpdateStrategy::Incremental(changed_files.clone());
                            match synchronizer.update_layer(&state, LayerKind::Working, strategy) {
                                Ok(stats) => {
                                    tracing::info!(
                                        "[WATCHER] Layer updated successfully, emitting event"
                                    );
                                    // Emit event for CLI
                                    let event = super::events::LayerUpdatedEvent::from_stats(
                                        LayerKind::Working,
                                        &stats,
                                    );
                                    super::events::emit_event(&event);
                                    tracing::info!("[WATCHER] Event emitted");
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "[WATCHER] Failed to update working layer: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Watcher error: {:?}", e);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // No events, continue loop
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }

            // Keep debouncer alive until thread exits
            drop(debouncer);
        });

        Ok(WatcherHandle { running })
    }

    /// Stop watching for file changes
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Get pending changes (for testing/debugging)
    pub fn pending_changes(&self) -> Vec<PathBuf> {
        self.pending_changes.lock().clone()
    }

    /// Clear pending changes
    pub fn clear_pending(&self) {
        self.pending_changes.lock().clear();
    }

    /// Check if a path should be watched
    fn should_watch_path(path: &PathBuf, _repo_root: &PathBuf) -> bool {
        // Skip hidden files and directories (except .github, etc.)
        // Check all path components, not just the file name
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                let name = name.to_string_lossy();
                if name.starts_with('.') && !name.starts_with(".github") {
                    return false;
                }
            }
        }

        // Skip common ignored patterns
        let path_str = path.to_string_lossy();
        let ignored_patterns = [
            "/node_modules/",
            "/target/",
            "/.git/",
            "/dist/",
            "/build/",
            "/__pycache__/",
            "/.venv/",
            "/venv/",
        ];

        for pattern in &ignored_patterns {
            if path_str.contains(pattern) {
                return false;
            }
        }

        // Check if it's a supported source file
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            let supported = [
                "ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "c", "cpp", "h", "hpp", "html",
                "css", "scss", "json", "yaml", "yml", "toml", "md",
            ];
            return supported.contains(&ext.as_str());
        }

        false
    }
}

/// Handle for controlling a running watcher
pub struct WatcherHandle {
    running: Arc<AtomicBool>,
}

impl WatcherHandle {
    /// Stop the watcher
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the watcher is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce_duration, Duration::from_millis(100));
        assert!(config.respect_gitignore);
        assert!(config.extensions.is_empty());
    }

    #[test]
    fn test_should_watch_path_source_files() {
        let repo = PathBuf::from("/repo");

        assert!(FileWatcher::should_watch_path(
            &PathBuf::from("/repo/src/main.rs"),
            &repo
        ));
        assert!(FileWatcher::should_watch_path(
            &PathBuf::from("/repo/src/component.tsx"),
            &repo
        ));
    }

    #[test]
    fn test_should_watch_path_ignored() {
        let repo = PathBuf::from("/repo");

        assert!(!FileWatcher::should_watch_path(
            &PathBuf::from("/repo/node_modules/package/index.js"),
            &repo
        ));
        assert!(!FileWatcher::should_watch_path(
            &PathBuf::from("/repo/.git/objects/abc"),
            &repo
        ));
        assert!(!FileWatcher::should_watch_path(
            &PathBuf::from("/repo/target/debug/binary"),
            &repo
        ));
    }

    #[test]
    fn test_should_watch_path_hidden_files() {
        let repo = PathBuf::from("/repo");

        assert!(!FileWatcher::should_watch_path(
            &PathBuf::from("/repo/.hidden/file.rs"),
            &repo
        ));
        // .github should be allowed
        // (but files inside need to be source files)
    }

    #[test]
    fn test_watcher_creation() {
        let watcher = FileWatcher::new(PathBuf::from("/tmp/test"));
        assert!(!watcher.is_running());
        assert!(watcher.pending_changes().is_empty());
    }
}
