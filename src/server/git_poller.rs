//! Git state polling for Base/Branch layer updates (SEM-102)
//!
//! Background polling tasks to detect git state changes and trigger
//! layer updates when the repository state changes.
//!
//! # Polling Intervals
//!
//! - Base layer: every 5s (origin/main changes after fetch/pull)
//! - Branch layer: every 1s (local commits, rebases, checkouts)
//!
//! # Detection Methods
//!
//! - Base layer stale: origin/main HEAD != indexed SHA
//! - Branch layer stale: HEAD != indexed SHA
//! - Rebase detected: merge-base change between base and HEAD

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use crate::drift::UpdateStrategy;
use crate::error::Result;
use crate::overlay::LayerKind;

use super::state::ServerState;
use super::sync::LayerSynchronizer;

/// Configuration for git polling
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// Base layer poll interval (default: 5s)
    pub base_interval: Duration,
    /// Branch layer poll interval (default: 1s)
    pub branch_interval: Duration,
    /// Whether to auto-update on changes
    pub auto_update: bool,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            base_interval: Duration::from_secs(5),
            branch_interval: Duration::from_secs(1),
            auto_update: true,
        }
    }
}

/// Git state for comparison
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitState {
    /// HEAD commit SHA
    head_sha: Option<String>,
    /// Base branch SHA (origin/main)
    base_sha: Option<String>,
    /// Current branch name
    branch_name: Option<String>,
    /// Merge base between HEAD and base branch
    merge_base: Option<String>,
}

impl GitState {
    /// Read current git state from repository
    fn read(repo_root: &PathBuf) -> Self {
        let cwd = Some(repo_root.as_path());
        let head_sha = get_head_sha(repo_root).ok();
        let base_sha = get_ref_sha(repo_root, "origin/main")
            .or_else(|_| get_ref_sha(repo_root, "origin/master"))
            .ok();
        let branch_name = crate::git::get_current_branch(cwd).ok();
        let merge_base = base_sha
            .as_ref()
            .and_then(|base| crate::git::get_merge_base("HEAD", base, cwd).ok());

        Self {
            head_sha,
            base_sha,
            branch_name,
            merge_base,
        }
    }

    /// Check if base layer is stale
    fn is_base_stale(&self, indexed_base_sha: &Option<String>) -> bool {
        match (&self.base_sha, indexed_base_sha) {
            (Some(current), Some(indexed)) => current != indexed,
            (Some(_), None) => true, // Have base now but didn't before
            _ => false,
        }
    }

    /// Check if branch layer is stale
    fn is_branch_stale(&self, indexed_head_sha: &Option<String>) -> bool {
        match (&self.head_sha, indexed_head_sha) {
            (Some(current), Some(indexed)) => current != indexed,
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Check if a rebase occurred
    fn is_rebase(&self, indexed_merge_base: &Option<String>) -> bool {
        match (&self.merge_base, indexed_merge_base) {
            (Some(current), Some(indexed)) => current != indexed,
            _ => false,
        }
    }
}

/// Git state poller for automatic layer updates
pub struct GitPoller {
    /// Repository root path
    repo_root: PathBuf,
    /// Poller configuration
    config: PollerConfig,
    /// Whether the poller is running
    running: Arc<AtomicBool>,
    /// Last known git state
    last_state: Arc<Mutex<GitState>>,
}

impl GitPoller {
    /// Create a new git poller for a repository
    pub fn new(repo_root: PathBuf) -> Self {
        let initial_state = GitState::read(&repo_root);
        Self {
            repo_root,
            config: PollerConfig::default(),
            running: Arc::new(AtomicBool::new(false)),
            last_state: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// Create with custom configuration
    pub fn with_config(repo_root: PathBuf, config: PollerConfig) -> Self {
        let initial_state = GitState::read(&repo_root);
        Self {
            repo_root,
            config,
            running: Arc::new(AtomicBool::new(false)),
            last_state: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// Check if the poller is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start polling for git state changes
    ///
    /// This spawns two background threads:
    /// 1. Base layer poller (every 5s)
    /// 2. Branch layer poller (every 1s)
    pub fn start(&self, state: Arc<ServerState>) -> Result<PollerHandle> {
        if self.running.swap(true, Ordering::SeqCst) {
            // Already running
            return Ok(PollerHandle {
                running: Arc::clone(&self.running),
            });
        }

        let running = Arc::clone(&self.running);
        let last_state = Arc::clone(&self.last_state);
        let repo_root = self.repo_root.clone();
        let config = self.config.clone();

        // Spawn branch layer poller (more frequent)
        let branch_running = Arc::clone(&running);
        let branch_state = Arc::clone(&state);
        let branch_last = Arc::clone(&last_state);
        let branch_root = repo_root.clone();
        let branch_interval = config.branch_interval;
        let auto_update = config.auto_update;

        thread::spawn(move || {
            let synchronizer = LayerSynchronizer::new(branch_root.clone());

            while branch_running.load(Ordering::SeqCst) {
                thread::sleep(branch_interval);

                let current = GitState::read(&branch_root);
                let _last = branch_last.lock().clone();

                // Check for branch changes
                let indexed_sha = branch_state
                    .read(|index| index.layer(LayerKind::Branch).meta.indexed_sha.clone());

                if current.is_branch_stale(&indexed_sha) {
                    tracing::info!(
                        "Branch layer stale: {:?} -> {:?}",
                        indexed_sha,
                        current.head_sha
                    );

                    // Mark layer stale
                    let indexed_merge_base = branch_state
                        .read(|index| index.layer(LayerKind::Branch).meta.merge_base_sha.clone());

                    let strategy = if current.is_rebase(&indexed_merge_base) {
                        UpdateStrategy::Rebase
                    } else {
                        // Get changed files for incremental update
                        let changed = crate::git::get_changed_files(
                            "HEAD~1",
                            "HEAD",
                            Some(branch_root.as_path()),
                        )
                        .unwrap_or_default();
                        if changed.len() < 10 {
                            UpdateStrategy::Incremental(
                                changed.into_iter().map(|c| PathBuf::from(c.path)).collect(),
                            )
                        } else {
                            UpdateStrategy::FullRebuild
                        }
                    };

                    branch_state.mark_layer_stale(LayerKind::Branch, strategy.clone());

                    if auto_update {
                        match synchronizer.update_layer(&branch_state, LayerKind::Branch, strategy)
                        {
                            Ok(stats) => {
                                // Emit event for CLI
                                let event = super::events::LayerUpdatedEvent::from_stats(
                                    LayerKind::Branch,
                                    &stats,
                                );
                                super::events::emit_event(&event);
                            }
                            Err(e) => {
                                tracing::error!("Failed to update branch layer: {}", e);
                            }
                        }
                    }
                }

                // Update last known state
                *branch_last.lock() = current;
            }
        });

        // Spawn base layer poller (less frequent)
        let base_running = Arc::clone(&running);
        let base_state = Arc::clone(&state);
        let base_last = Arc::clone(&last_state);
        let base_root = repo_root;
        let base_interval = config.base_interval;

        thread::spawn(move || {
            let synchronizer = LayerSynchronizer::new(base_root.clone());

            while base_running.load(Ordering::SeqCst) {
                thread::sleep(base_interval);

                let current = GitState::read(&base_root);

                // Check for base changes (origin/main moved)
                let indexed_sha =
                    base_state.read(|index| index.layer(LayerKind::Base).meta.indexed_sha.clone());

                if current.is_base_stale(&indexed_sha) {
                    tracing::info!(
                        "Base layer stale: {:?} -> {:?}",
                        indexed_sha,
                        current.base_sha
                    );

                    // Base layer changes typically require full rebuild
                    let strategy = UpdateStrategy::FullRebuild;
                    base_state.mark_layer_stale(LayerKind::Base, strategy.clone());

                    if auto_update {
                        match synchronizer.update_layer(&base_state, LayerKind::Base, strategy) {
                            Ok(stats) => {
                                // Emit event for CLI
                                let event = super::events::LayerUpdatedEvent::from_stats(
                                    LayerKind::Base,
                                    &stats,
                                );
                                super::events::emit_event(&event);
                            }
                            Err(e) => {
                                tracing::error!("Failed to update base layer: {}", e);
                            }
                        }
                    }
                }

                *base_last.lock() = current;
            }
        });

        Ok(PollerHandle { running })
    }

    /// Stop polling
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Manually poll for changes (for testing)
    pub fn poll_once(&self, state: &ServerState) -> (bool, bool) {
        let current = GitState::read(&self.repo_root);

        let indexed_base =
            state.read(|index| index.layer(LayerKind::Base).meta.indexed_sha.clone());
        let indexed_branch =
            state.read(|index| index.layer(LayerKind::Branch).meta.indexed_sha.clone());

        let base_stale = current.is_base_stale(&indexed_base);
        let branch_stale = current.is_branch_stale(&indexed_branch);

        *self.last_state.lock() = current;

        (base_stale, branch_stale)
    }
}

/// Handle for controlling a running poller
pub struct PollerHandle {
    running: Arc<AtomicBool>,
}

impl PollerHandle {
    /// Stop the poller
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the poller is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for PollerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Git Helper Functions
// ============================================================================

use std::process::Command;

/// Get SHA for a specific ref (branch, tag, etc.)
fn get_ref_sha(repo_root: &PathBuf, ref_name: &str) -> crate::error::Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", ref_name])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(crate::error::McpDiffError::GitError {
            message: format!("Failed to get ref SHA for {}", ref_name),
        })
    }
}

/// Get current HEAD SHA
fn get_head_sha(repo_root: &PathBuf) -> crate::error::Result<String> {
    get_ref_sha(repo_root, "HEAD")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poller_config_default() {
        let config = PollerConfig::default();
        assert_eq!(config.base_interval, Duration::from_secs(5));
        assert_eq!(config.branch_interval, Duration::from_secs(1));
        assert!(config.auto_update);
    }

    #[test]
    fn test_git_state_stale_detection() {
        let state = GitState {
            head_sha: Some("abc123".to_string()),
            base_sha: Some("def456".to_string()),
            branch_name: Some("main".to_string()),
            merge_base: Some("xyz789".to_string()),
        };

        // Same SHA = not stale
        assert!(!state.is_branch_stale(&Some("abc123".to_string())));
        assert!(!state.is_base_stale(&Some("def456".to_string())));

        // Different SHA = stale
        assert!(state.is_branch_stale(&Some("different".to_string())));
        assert!(state.is_base_stale(&Some("different".to_string())));

        // No indexed SHA = stale
        assert!(state.is_branch_stale(&None));
        assert!(state.is_base_stale(&None));
    }

    #[test]
    fn test_rebase_detection() {
        let state = GitState {
            head_sha: Some("abc123".to_string()),
            base_sha: Some("def456".to_string()),
            branch_name: Some("feature".to_string()),
            merge_base: Some("new_merge_base".to_string()),
        };

        // Same merge base = no rebase
        assert!(!state.is_rebase(&Some("new_merge_base".to_string())));

        // Different merge base = rebase occurred
        assert!(state.is_rebase(&Some("old_merge_base".to_string())));
    }

    #[test]
    fn test_poller_creation() {
        let poller = GitPoller::new(PathBuf::from("/tmp/test"));
        assert!(!poller.is_running());
    }
}
