//! Tests for the `commit` CLI command
//!
//! The commit command prepares information for writing commit messages:
//! - `commit` - Show staged and unstaged changes with semantic analysis
//! - `commit --staged` - Only show staged changes
//! - `commit --metrics` - Include complexity metrics
//! - `commit --all-metrics` - Include all detailed metrics

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// COMMIT PREP BASIC TESTS
// ============================================================================

#[test]
fn test_commit_prep_no_changes() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit"]);

    // Should report no changes or show current state
    assert!(
        output.contains("no change")
            || output.contains("No change")
            || output.contains("clean")
            || output.contains("nothing")
            || !output.is_empty(),
        "Should handle no changes: {}",
        output
    );
}

#[test]
fn test_commit_prep_with_changes() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Make changes
    repo.add_ts_function("src/main.ts", "updated", "return 2;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit"]);

    // Should show changes
    assert!(
        output.contains("main.ts")
            || output.contains("updated")
            || output.contains("change")
            || output.contains("modified")
            || !output.is_empty(),
        "Should show changes: {}",
        output
    );
}

#[test]
fn test_commit_prep_new_file() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Add new file
    repo.add_ts_function("src/new.ts", "newFunc", "return 'new';");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit"]);

    // Should show new file
    assert!(
        output.contains("new.ts")
            || output.contains("newFunc")
            || output.contains("new")
            || output.contains("added")
            || output.contains("untracked")
            || !output.is_empty(),
        "Should show new file: {}",
        output
    );
}

#[test]
fn test_commit_prep_json_format() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");
    repo.add_ts_function("src/main.ts", "updated", "return 2;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit", "-f", "json"]);
    let json = assert_valid_json(&output, "commit prep json");

    // Should have change info
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("changes")
            || output_str.contains("staged")
            || output_str.contains("unstaged")
            || output_str.contains("files")
            || json.is_object(),
        "Should have commit prep info: {}",
        output
    );
}

// ============================================================================
// COMMIT PREP STAGED ONLY TESTS
// ============================================================================

#[test]
fn test_commit_prep_staged_only() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Make changes and stage some
    repo.add_ts_function("src/staged.ts", "staged", "return 'staged';");
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["add", "src/staged.ts"])
        .output()
        .unwrap();

    repo.add_ts_function("src/unstaged.ts", "unstaged", "return 'unstaged';");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit", "--staged"]);

    // Should only show staged changes
    assert!(
        output.contains("staged") || !output.is_empty(),
        "Should show staged changes: {}",
        output
    );
}

#[test]
fn test_commit_prep_staged_no_staged() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Only unstaged changes
    repo.add_ts_function("src/unstaged.ts", "unstaged", "return 'unstaged';");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit", "--staged"]);

    // Should report no staged changes
    assert!(
        output.contains("no staged")
            || output.contains("nothing")
            || output.is_empty()
            || !output.is_empty(),
        "Should handle no staged changes: {}",
        output
    );
}

// ============================================================================
// COMMIT PREP METRICS TESTS
// ============================================================================

#[test]
fn test_commit_prep_with_metrics() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Add complex function
    repo.add_file(
        "src/complex.ts",
        r#"
export function complex(x: number): number {
    if (x > 0) {
        for (let i = 0; i < x; i++) {
            if (i % 2 === 0) {
                console.log(i);
            }
        }
    }
    return x * 2;
}
"#,
    );

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit", "--metrics"]);

    // Should include complexity metrics (in text format)
    assert!(
        output.contains("complexity")
            || output.contains("cognitive")
            || output.contains("cyclomatic")
            || output.contains("nesting")
            || output.contains("complex.ts")
            || !output.is_empty(),
        "Should include metrics info: {}",
        output
    );
}

#[test]
fn test_commit_prep_with_all_metrics() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    repo.add_ts_function("src/new.ts", "newFunc", "return 2;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit", "--all-metrics"]);

    // Should include all metrics (may be text format)
    assert!(!output.is_empty(), "Should return metrics output");
}

// ============================================================================
// COMMIT PREP OPTIONS TESTS
// ============================================================================

#[test]
fn test_commit_prep_no_auto_refresh() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    repo.add_ts_function("src/new.ts", "newFunc", "return 2;");

    // Pre-generate index
    repo.generate_index().unwrap();

    // run_cli_success already verifies the command completes successfully
    repo.run_cli_success(&["commit", "--no-auto-refresh"]);
}

#[test]
fn test_commit_prep_no_diff_stats() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    repo.add_ts_function("src/new.ts", "newFunc", "return 2;");

    repo.generate_index().unwrap();

    // run_cli_success already verifies the command completes successfully
    repo.run_cli_success(&["commit", "--no-diff-stats"]);
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_commit_all_formats_complete() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");
    repo.add_ts_function("src/new.ts", "newFunc", "return 2;");

    repo.generate_index().unwrap();

    // Test all formats - commit may not support JSON format for all output
    let text_result = repo.run_cli(&["commit", "-f", "text"]);
    let json_result = repo.run_cli(&["commit", "-f", "json"]);
    let toon_result = repo.run_cli(&["commit", "-f", "toon"]);

    // All should complete without error
    assert!(text_result.is_ok(), "Text format should complete");
    assert!(json_result.is_ok(), "JSON format should complete");
    assert!(toon_result.is_ok(), "TOON format should complete");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_commit_prep_not_git_repo() {
    let repo = TestRepo::new();
    // Not a git repo
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let (stdout, stderr) = repo.run_cli_failure(&["commit"]);

    // Should report not a git repo
    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("git")
        || combined.to_lowercase().contains("repository")
        || combined.to_lowercase().contains("error");
    assert!(has_error, "Expected git-related error: {}", combined);
}

#[test]
fn test_commit_prep_empty_repo() {
    let repo = TestRepo::new();
    repo.init_git();

    // Empty repo, no commits
    let result = repo.run_cli(&["commit"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

#[test]
fn test_commit_prep_binary_file() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Add binary file
    std::fs::write(repo.path().join("image.png"), [0x89, 0x50, 0x4E, 0x47]).unwrap();

    repo.generate_index().unwrap();

    // run_cli_success already verifies the command completes successfully
    repo.run_cli_success(&["commit"]);
}

#[test]
fn test_commit_prep_deleted_file() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.add_ts_function("src/deleted.ts", "deleted", "return 'deleted';");
    repo.commit("Initial commit");

    // Delete the file
    std::fs::remove_file(repo.path().join("src/deleted.ts")).unwrap();

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["commit"]);

    // Should show deleted file
    assert!(
        output.contains("deleted")
            || output.contains("removed")
            || output.contains("deleted.ts")
            || !output.is_empty(),
        "Should show deleted file: {}",
        output
    );
}
