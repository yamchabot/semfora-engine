//! SHA-based drift detection for layered index staleness
//!
//! This module implements SEM-47: SHA-Based Drift Detection.
//!
//! # Key Insight
//!
//! Time-based staleness is meaningless:
//! - Old project with same SHA = FRESH (nothing changed)
//! - 5-minute break with half app rewritten = STALE (everything changed)
//!
//! # Drift Detection Strategy
//!
//! | Drift | Strategy |
//! |-------|----------|
//! | 0 files | No action (Fresh) |
//! | < 10 files | Incremental update |
//! | < 30% of repo | Rebase overlay |
//! | ≥ 30% of repo | Full rebuild |
//!
//! # Layer-Specific Detection
//!
//! - **Base layer**: Compare indexed SHA vs current HEAD of base branch
//! - **Branch layer**: Compare indexed SHA vs current HEAD + check merge-base
//! - **Working layer**: Check uncommitted changes via `git status`/`git diff`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::git;
use crate::overlay::LayerKind;

// ============================================================================
// Drift Status
// ============================================================================

/// Detailed drift status for a layer
///
/// Contains all information needed to decide on an update strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftStatus {
    /// Whether the layer is stale (needs update)
    pub is_stale: bool,

    /// The SHA that was indexed (None if no index exists)
    pub indexed_sha: Option<String>,

    /// The current SHA to compare against
    pub current_sha: Option<String>,

    /// List of files that changed since last index
    pub changed_files: Vec<PathBuf>,

    /// Percentage of repo that has drifted (0.0 - 100.0)
    pub drift_percentage: f64,

    /// For branch layers: the stored merge-base SHA
    pub indexed_merge_base: Option<String>,

    /// For branch layers: the current merge-base SHA
    pub current_merge_base: Option<String>,

    /// Whether the merge-base has changed (indicates rebase/merge)
    pub merge_base_changed: bool,
}

impl DriftStatus {
    /// Create a fresh (not stale) status
    #[must_use]
    pub fn fresh(indexed_sha: String, current_sha: String) -> Self {
        Self {
            is_stale: false,
            indexed_sha: Some(indexed_sha),
            current_sha: Some(current_sha),
            changed_files: Vec::new(),
            drift_percentage: 0.0,
            indexed_merge_base: None,
            current_merge_base: None,
            merge_base_changed: false,
        }
    }

    /// Create a stale status with changed files
    #[must_use]
    pub fn stale(
        indexed_sha: Option<String>,
        current_sha: String,
        changed_files: Vec<PathBuf>,
        total_files: usize,
    ) -> Self {
        let drift_percentage = if total_files > 0 {
            (changed_files.len() as f64 / total_files as f64) * 100.0
        } else {
            0.0
        };

        Self {
            is_stale: true,
            indexed_sha,
            current_sha: Some(current_sha),
            changed_files,
            drift_percentage,
            indexed_merge_base: None,
            current_merge_base: None,
            merge_base_changed: false,
        }
    }

    /// Create a stale status when no index exists
    #[must_use]
    pub fn no_index() -> Self {
        Self {
            is_stale: true,
            indexed_sha: None,
            current_sha: None,
            changed_files: Vec::new(),
            drift_percentage: 100.0, // Everything needs indexing
            indexed_merge_base: None,
            current_merge_base: None,
            merge_base_changed: false,
        }
    }

    /// Add merge-base information (for branch layers)
    #[must_use]
    pub fn with_merge_base(
        mut self,
        indexed: Option<String>,
        current: Option<String>,
    ) -> Self {
        self.merge_base_changed = match (&indexed, &current) {
            (Some(i), Some(c)) => i != c,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        };
        self.indexed_merge_base = indexed;
        self.current_merge_base = current;
        self
    }

    /// Get the recommended update strategy based on drift magnitude
    #[must_use]
    pub fn strategy(&self, total_repo_files: usize) -> UpdateStrategy {
        if !self.is_stale {
            return UpdateStrategy::Fresh;
        }

        // No index at all = full rebuild
        if self.indexed_sha.is_none() {
            return UpdateStrategy::FullRebuild;
        }

        // Merge-base changed = rebase needed
        if self.merge_base_changed {
            return UpdateStrategy::Rebase;
        }

        let changed_count = self.changed_files.len();

        // Calculate thresholds
        let thirty_percent = (total_repo_files as f64 * 0.30).ceil() as usize;

        if changed_count == 0 {
            UpdateStrategy::Fresh
        } else if changed_count < 10 {
            UpdateStrategy::Incremental(self.changed_files.clone())
        } else if changed_count < thirty_percent {
            UpdateStrategy::Rebase
        } else {
            UpdateStrategy::FullRebuild
        }
    }
}

impl Default for DriftStatus {
    fn default() -> Self {
        Self::no_index()
    }
}

// ============================================================================
// Update Strategy
// ============================================================================

/// Strategy for updating a stale layer
///
/// The strategy is selected based on drift magnitude:
/// - Fresh: No update needed
/// - Incremental: Update only changed files (< 10 files)
/// - Rebase: Reconcile overlay with new base (< 30% changed)
/// - FullRebuild: Discard and recreate (≥ 30% changed)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateStrategy {
    /// No update needed - layer is fresh
    Fresh,

    /// Incremental update - reparse only changed files
    Incremental(Vec<PathBuf>),

    /// Rebase overlay - reconcile with new base
    Rebase,

    /// Full rebuild - discard and recreate
    FullRebuild,
}

impl UpdateStrategy {
    /// Check if any update is needed
    #[must_use]
    pub fn needs_update(&self) -> bool {
        !matches!(self, Self::Fresh)
    }

    /// Get a human-readable description
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Fresh => "No update needed".to_string(),
            Self::Incremental(files) => {
                let file_word = if files.len() == 1 { "file" } else { "files" };
                format!("Incremental update ({} {})", files.len(), file_word)
            }
            Self::Rebase => "Rebase overlay".to_string(),
            Self::FullRebuild => "Full rebuild required".to_string(),
        }
    }
}

// ============================================================================
// Drift Detector
// ============================================================================

/// Drift detector for a repository
///
/// Checks layer staleness using git SHA comparison instead of timestamps.
pub struct DriftDetector {
    /// Repository root path
    repo_root: PathBuf,

    /// Total number of tracked files in the repository (for percentage calculation)
    total_files: usize,
}

impl DriftDetector {
    /// Create a new drift detector for a repository
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            total_files: 0,
        }
    }

    /// Create with known file count (for accurate percentage calculation)
    pub fn with_file_count(repo_root: PathBuf, total_files: usize) -> Self {
        Self {
            repo_root,
            total_files,
        }
    }

    /// Set the total file count (for accurate percentage calculation)
    pub fn set_total_files(&mut self, count: usize) {
        self.total_files = count;
    }

    /// Check drift for a specific layer
    ///
    /// # Arguments
    /// * `layer` - The layer kind to check
    /// * `indexed_sha` - The SHA that was indexed (from LayerMeta)
    /// * `merge_base_sha` - The merge-base SHA (for branch layers)
    pub fn check_drift(
        &self,
        layer: LayerKind,
        indexed_sha: Option<&str>,
        merge_base_sha: Option<&str>,
    ) -> Result<DriftStatus> {
        match layer {
            LayerKind::Base => self.check_base_drift(indexed_sha),
            LayerKind::Branch => self.check_branch_drift(indexed_sha, merge_base_sha),
            LayerKind::Working => self.check_working_drift(indexed_sha),
            LayerKind::AI => {
                // AI layer is ephemeral, always considered fresh (managed in-memory)
                Ok(DriftStatus {
                    is_stale: false,
                    indexed_sha: None,
                    current_sha: None,
                    changed_files: Vec::new(),
                    drift_percentage: 0.0,
                    indexed_merge_base: None,
                    current_merge_base: None,
                    merge_base_changed: false,
                })
            }
        }
    }

    /// Check drift for base layer
    ///
    /// Compares indexed SHA vs current HEAD of the base branch (main/master).
    fn check_base_drift(&self, indexed_sha: Option<&str>) -> Result<DriftStatus> {
        let indexed_sha = match indexed_sha {
            Some(sha) => sha,
            None => return Ok(DriftStatus::no_index()),
        };

        // Get current HEAD of base branch
        let base_branch = git::detect_base_branch(Some(&self.repo_root))?;
        let current_sha = git::git_command(&["rev-parse", &base_branch], Some(&self.repo_root))?;

        // Same SHA = fresh
        if indexed_sha == current_sha {
            return Ok(DriftStatus::fresh(indexed_sha.to_string(), current_sha));
        }

        // Get changed files between indexed SHA and current
        let changed = git::get_changed_files(indexed_sha, &current_sha, Some(&self.repo_root))?;
        let changed_paths: Vec<PathBuf> = changed.iter().map(|f| PathBuf::from(&f.path)).collect();

        Ok(DriftStatus::stale(
            Some(indexed_sha.to_string()),
            current_sha,
            changed_paths,
            self.total_files,
        ))
    }

    /// Check drift for branch layer
    ///
    /// Compares indexed SHA vs current HEAD, and also checks if merge-base changed
    /// (which indicates a rebase or merge from upstream).
    fn check_branch_drift(
        &self,
        indexed_sha: Option<&str>,
        stored_merge_base: Option<&str>,
    ) -> Result<DriftStatus> {
        let indexed_sha = match indexed_sha {
            Some(sha) => sha,
            None => return Ok(DriftStatus::no_index()),
        };

        // Get current HEAD
        let current_sha = git::git_command(&["rev-parse", "HEAD"], Some(&self.repo_root))?;

        // Get current merge-base
        let base_branch = git::detect_base_branch(Some(&self.repo_root))?;
        let current_merge_base =
            git::get_merge_base("HEAD", &base_branch, Some(&self.repo_root)).ok();

        // Check if merge-base changed (indicates rebase)
        let merge_base_changed = match (stored_merge_base, &current_merge_base) {
            (Some(stored), Some(current)) => stored != current,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        };

        // Same SHA and merge-base = fresh
        if indexed_sha == current_sha && !merge_base_changed {
            return Ok(DriftStatus::fresh(indexed_sha.to_string(), current_sha).with_merge_base(
                stored_merge_base.map(String::from),
                current_merge_base,
            ));
        }

        // Get changed files
        let changed = if indexed_sha != current_sha {
            git::get_changed_files(indexed_sha, &current_sha, Some(&self.repo_root))?
        } else {
            Vec::new()
        };
        let changed_paths: Vec<PathBuf> = changed.iter().map(|f| PathBuf::from(&f.path)).collect();

        Ok(DriftStatus::stale(
            Some(indexed_sha.to_string()),
            current_sha,
            changed_paths,
            self.total_files,
        )
        .with_merge_base(stored_merge_base.map(String::from), current_merge_base))
    }

    /// Check drift for working layer
    ///
    /// Checks for uncommitted changes via `git diff` and `git status`.
    fn check_working_drift(&self, indexed_sha: Option<&str>) -> Result<DriftStatus> {
        // For working layer, we compare against HEAD (what's committed)
        let current_sha = git::git_command(&["rev-parse", "HEAD"], Some(&self.repo_root))?;

        // Get uncommitted changes (staged + unstaged)
        let uncommitted = git::get_uncommitted_changes("HEAD", Some(&self.repo_root))?;
        let changed_paths: Vec<PathBuf> =
            uncommitted.iter().map(|f| PathBuf::from(&f.path)).collect();

        if changed_paths.is_empty() {
            // No uncommitted changes
            return Ok(DriftStatus::fresh(
                indexed_sha.unwrap_or(&current_sha).to_string(),
                current_sha,
            ));
        }

        // Has uncommitted changes = stale
        Ok(DriftStatus::stale(
            indexed_sha.map(String::from),
            current_sha,
            changed_paths,
            self.total_files,
        ))
    }

    /// Get the recommended update strategy for a layer
    pub fn get_strategy(
        &self,
        layer: LayerKind,
        indexed_sha: Option<&str>,
        merge_base_sha: Option<&str>,
    ) -> Result<UpdateStrategy> {
        let drift = self.check_drift(layer, indexed_sha, merge_base_sha)?;
        Ok(drift.strategy(self.total_files))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Count tracked files in a git repository
pub fn count_tracked_files(repo_root: &std::path::Path) -> Result<usize> {
    let output = git::git_command(&["ls-files"], Some(repo_root))?;
    Ok(output.lines().count())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper to create a git repository for testing
    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Initialize git repo
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git user
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to config git email");

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to config git name");

        // Create initial commit
        fs::write(dir.path().join("README.md"), "# Test Repo").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        dir
    }

    /// Get current HEAD SHA
    fn get_head_sha(dir: &TempDir) -> String {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to get HEAD");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    // ========================================================================
    // TDD Tests from SEM-47 Requirements
    // ========================================================================

    #[test]
    fn test_same_sha_reports_fresh() {
        let dir = setup_git_repo();
        let head_sha = get_head_sha(&dir);

        let detector = DriftDetector::new(dir.path().to_path_buf());
        let drift = detector
            .check_drift(LayerKind::Base, Some(&head_sha), None)
            .unwrap();

        assert!(!drift.is_stale, "Same SHA should report fresh");
        assert_eq!(drift.indexed_sha.as_deref(), Some(head_sha.as_str()));
        assert_eq!(drift.current_sha.as_deref(), Some(head_sha.as_str()));
        assert!(drift.changed_files.is_empty());
        assert_eq!(drift.drift_percentage, 0.0);
    }

    #[test]
    fn test_different_sha_reports_stale() {
        let dir = setup_git_repo();
        let old_sha = get_head_sha(&dir);

        // Make a new commit
        fs::write(dir.path().join("new_file.rs"), "fn main() {}").unwrap();
        Command::new("git")
            .args(["add", "new_file.rs"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Add new file"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let new_sha = get_head_sha(&dir);
        assert_ne!(old_sha, new_sha, "SHA should have changed");

        let detector = DriftDetector::new(dir.path().to_path_buf());
        let drift = detector
            .check_drift(LayerKind::Base, Some(&old_sha), None)
            .unwrap();

        assert!(drift.is_stale, "Different SHA should report stale");
        assert_eq!(drift.indexed_sha.as_deref(), Some(old_sha.as_str()));
        assert_eq!(drift.current_sha.as_deref(), Some(new_sha.as_str()));
        assert!(!drift.changed_files.is_empty(), "Should have changed files");
        assert!(
            drift.changed_files.iter().any(|p| p.ends_with("new_file.rs")),
            "Should include new_file.rs in changed files"
        );
    }

    #[test]
    fn test_strategy_incremental_under_10_files() {
        let dir = setup_git_repo();
        let old_sha = get_head_sha(&dir);

        // Add 5 files (< 10)
        for i in 0..5 {
            fs::write(dir.path().join(format!("file{}.rs", i)), "// content").unwrap();
        }
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Add 5 files"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let detector = DriftDetector::with_file_count(dir.path().to_path_buf(), 100);
        let drift = detector
            .check_drift(LayerKind::Base, Some(&old_sha), None)
            .unwrap();

        assert!(drift.is_stale);
        assert_eq!(drift.changed_files.len(), 5);

        let strategy = drift.strategy(100);
        assert!(
            matches!(strategy, UpdateStrategy::Incremental(_)),
            "Should be Incremental for < 10 files, got {:?}",
            strategy
        );

        if let UpdateStrategy::Incremental(files) = strategy {
            assert_eq!(files.len(), 5);
        }
    }

    #[test]
    fn test_strategy_rebase_under_30_percent() {
        let dir = setup_git_repo();
        let old_sha = get_head_sha(&dir);

        // Add 15 files (15% of 100 total = between 10 and 30%)
        for i in 0..15 {
            fs::write(dir.path().join(format!("file{}.rs", i)), "// content").unwrap();
        }
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Add 15 files"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let detector = DriftDetector::with_file_count(dir.path().to_path_buf(), 100);
        let drift = detector
            .check_drift(LayerKind::Base, Some(&old_sha), None)
            .unwrap();

        assert!(drift.is_stale);
        assert_eq!(drift.changed_files.len(), 15);

        let strategy = drift.strategy(100);
        assert_eq!(
            strategy,
            UpdateStrategy::Rebase,
            "Should be Rebase for 10-30% of files"
        );
    }

    #[test]
    fn test_strategy_full_rebuild_over_30_percent() {
        let dir = setup_git_repo();
        let old_sha = get_head_sha(&dir);

        // Add 35 files (35% of 100 total = >= 30%)
        for i in 0..35 {
            fs::write(dir.path().join(format!("file{}.rs", i)), "// content").unwrap();
        }
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Add 35 files"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let detector = DriftDetector::with_file_count(dir.path().to_path_buf(), 100);
        let drift = detector
            .check_drift(LayerKind::Base, Some(&old_sha), None)
            .unwrap();

        assert!(drift.is_stale);
        assert_eq!(drift.changed_files.len(), 35);

        let strategy = drift.strategy(100);
        assert_eq!(
            strategy,
            UpdateStrategy::FullRebuild,
            "Should be FullRebuild for >= 30% of files"
        );
    }

    #[test]
    fn test_merge_base_change_detected() {
        let dir = setup_git_repo();
        let initial_sha = get_head_sha(&dir);

        // Create a feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to create branch");

        // Add a commit on feature
        fs::write(dir.path().join("feature.rs"), "// feature").unwrap();
        Command::new("git")
            .args(["add", "feature.rs"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Feature commit"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let feature_sha = get_head_sha(&dir);

        // Go back to main and add a commit (simulating upstream changes)
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to checkout main");

        fs::write(dir.path().join("main_change.rs"), "// main").unwrap();
        Command::new("git")
            .args(["add", "main_change.rs"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Main commit"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let new_main_sha = get_head_sha(&dir);

        // Go back to feature branch
        Command::new("git")
            .args(["checkout", "feature"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to checkout feature");

        // The merge-base was initial_sha, but now main has moved
        // If we rebase feature onto new main, merge-base would change
        let detector = DriftDetector::new(dir.path().to_path_buf());

        // Check with old merge-base (initial_sha)
        let drift = detector
            .check_drift(LayerKind::Branch, Some(&feature_sha), Some(&initial_sha))
            .unwrap();

        // Current merge-base should be initial_sha (feature hasn't been rebased)
        // So merge_base_changed should be false in this case
        assert!(!drift.merge_base_changed, "Merge-base hasn't changed yet");

        // Now simulate rebase by using new_main_sha as the stored merge-base
        // (as if the index was created when main was at new_main_sha)
        let drift_after_rebase = detector
            .check_drift(LayerKind::Branch, Some(&feature_sha), Some(&new_main_sha))
            .unwrap();

        // The stored merge-base (new_main_sha) differs from current (initial_sha)
        assert!(
            drift_after_rebase.merge_base_changed,
            "Merge-base should be detected as changed"
        );
    }

    #[test]
    fn test_working_layer_checks_uncommitted() {
        let dir = setup_git_repo();
        let head_sha = get_head_sha(&dir);

        // Initially no uncommitted changes
        let detector = DriftDetector::new(dir.path().to_path_buf());
        let drift = detector
            .check_drift(LayerKind::Working, Some(&head_sha), None)
            .unwrap();

        assert!(!drift.is_stale, "No uncommitted changes = fresh");
        assert!(drift.changed_files.is_empty());

        // Add uncommitted changes - must be tracked file (modify existing)
        // git diff only shows tracked files, not untracked ones
        fs::write(dir.path().join("README.md"), "# Modified content").unwrap();

        let drift = detector
            .check_drift(LayerKind::Working, Some(&head_sha), None)
            .unwrap();

        assert!(drift.is_stale, "Uncommitted changes = stale");
        assert!(
            drift.changed_files.iter().any(|p| p.ends_with("README.md")),
            "Should include README.md in changed files: {:?}",
            drift.changed_files
        );
    }

    // ========================================================================
    // Additional Tests
    // ========================================================================

    #[test]
    fn test_no_index_returns_full_rebuild() {
        let dir = setup_git_repo();

        let detector = DriftDetector::with_file_count(dir.path().to_path_buf(), 100);
        let drift = detector
            .check_drift(LayerKind::Base, None, None)
            .unwrap();

        assert!(drift.is_stale);
        assert!(drift.indexed_sha.is_none());
        assert_eq!(drift.drift_percentage, 100.0);

        let strategy = drift.strategy(100);
        assert_eq!(strategy, UpdateStrategy::FullRebuild);
    }

    #[test]
    fn test_ai_layer_always_fresh() {
        let dir = setup_git_repo();

        let detector = DriftDetector::new(dir.path().to_path_buf());
        let drift = detector
            .check_drift(LayerKind::AI, None, None)
            .unwrap();

        assert!(!drift.is_stale, "AI layer should always be fresh");
    }

    #[test]
    fn test_update_strategy_needs_update() {
        assert!(!UpdateStrategy::Fresh.needs_update());
        assert!(UpdateStrategy::Incremental(vec![]).needs_update());
        assert!(UpdateStrategy::Rebase.needs_update());
        assert!(UpdateStrategy::FullRebuild.needs_update());
    }

    #[test]
    fn test_update_strategy_description() {
        assert_eq!(UpdateStrategy::Fresh.description(), "No update needed");
        assert!(UpdateStrategy::Incremental(vec![PathBuf::from("a.rs")])
            .description()
            .contains("1 files"));
        assert_eq!(UpdateStrategy::Rebase.description(), "Rebase overlay");
        assert_eq!(
            UpdateStrategy::FullRebuild.description(),
            "Full rebuild required"
        );
    }

    #[test]
    fn test_drift_status_default() {
        let status = DriftStatus::default();
        assert!(status.is_stale);
        assert!(status.indexed_sha.is_none());
        assert_eq!(status.drift_percentage, 100.0);
    }

    #[test]
    fn test_count_tracked_files() {
        let dir = setup_git_repo();

        // Should have 1 file (README.md)
        let count = count_tracked_files(dir.path()).unwrap();
        assert_eq!(count, 1);

        // Add more files
        fs::write(dir.path().join("a.rs"), "//").unwrap();
        fs::write(dir.path().join("b.rs"), "//").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .args(["commit", "-m", "Add files"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to git commit");

        let count = count_tracked_files(dir.path()).unwrap();
        assert_eq!(count, 3);
    }
}
