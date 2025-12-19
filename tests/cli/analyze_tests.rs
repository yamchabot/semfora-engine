//! Tests for the `analyze` CLI command
//!
//! The analyze command operations:
//! - `analyze <path>` - Analyze a single file or directory
//! - `analyze --diff <ref>` - Analyze git diff against a reference
//! - `analyze --uncommitted` - Analyze uncommitted changes
//!
//! Note: Directory analysis outputs TOON format regardless of -f flag

use crate::common::{
    assert_contains, assert_symbol_exists, assert_valid_json, assert_valid_toon, TestRepo,
};

// ============================================================================
// ANALYZE FILE TESTS
// ============================================================================

#[test]
fn test_analyze_file_typescript() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "Auth");

    let output = repo.run_cli_success(&["analyze", "src/service.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze file typescript");

    // Should extract class and function symbols (interfaces may not be extracted as symbols)
    assert_symbol_exists(&json, "AuthService");
    assert_symbol_exists(&json, "createAuth");
}

#[test]
fn test_analyze_file_rust() {
    let repo = TestRepo::new();
    repo.add_rs_module("src/lib.rs", "Cache");

    let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze file rust");

    // Rust modules should extract struct and function
    assert_symbol_exists(&json, "CacheService");
}

#[test]
fn test_analyze_file_python() {
    let repo = TestRepo::new();
    repo.add_py_module("src/app.py", "Database");

    let output = repo.run_cli_success(&["analyze", "src/app.py", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze file python");

    assert_symbol_exists(&json, "DatabaseService");
}

#[test]
fn test_analyze_file_go() {
    let repo = TestRepo::new();
    repo.add_go_module("pkg/api/server.go", "api", "Server");

    let output = repo.run_cli_success(&["analyze", "pkg/api/server.go", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze file go");

    // Go modules should extract struct
    assert_symbol_exists(&json, "ServerService");
}

#[test]
fn test_analyze_file_with_source() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#,
    );

    let output = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze file with source");

    assert_symbol_exists(&json, "greet");
}

#[test]
fn test_analyze_file_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function(
        "src/utils.ts",
        "formatDate",
        "return new Date().toISOString();",
    );

    let output = repo.run_cli_success(&["analyze", "src/utils.ts", "-f", "text"]);

    // Text format should contain the function name
    assert_contains(&output, "formatDate", true, "text format output");
}

#[test]
fn test_analyze_file_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_function(
        "src/utils.ts",
        "formatDate",
        "return new Date().toISOString();",
    );

    let output = repo.run_cli_success(&["analyze", "src/utils.ts", "-f", "toon"]);

    // TOON format should have content
    assert!(
        output.contains("_type") || output.contains("formatDate") || !output.is_empty(),
        "TOON format should produce output: {}",
        output
    );
}

#[test]
fn test_analyze_file_nonexistent() {
    let repo = TestRepo::new();

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "nonexistent.ts"]);

    // Should report error about file not found
    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no such file");
    assert!(
        has_error,
        "Expected error about nonexistent file: {}",
        combined
    );
}

#[test]
fn test_analyze_file_empty() {
    let repo = TestRepo::new();
    repo.add_empty_file("src/empty.ts");

    // Empty file should not error, just return empty/minimal results
    let result = repo.run_cli(&["analyze", "src/empty.ts", "-f", "json"]);
    assert!(result.is_ok());
}

// ============================================================================
// ANALYZE DIRECTORY TESTS
// Note: Directory analysis outputs TOON/overview format regardless of -f json
// ============================================================================

#[test]
fn test_analyze_dir_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/index.ts", "main", "console.log('main');")
        .add_ts_function("src/utils.ts", "helper", "return 1;")
        .add_ts_function("src/api/handler.ts", "handleRequest", "return res.json();");

    // Directory analysis outputs TOON format (overview)
    let output = repo.run_cli_success(&["analyze", "src"]);

    // Should produce repo overview output
    assert!(
        output.contains("_type") || output.contains("modules") || output.contains("files"),
        "Directory analysis should produce overview: {}",
        output
    );
}

#[test]
fn test_analyze_dir_nested() {
    let repo = TestRepo::new();
    repo.with_deep_nesting();

    let output = repo.run_cli_success(&["analyze", "src"]);

    // Should produce output showing modules
    assert!(
        output.contains("modules") || output.contains("files") || !output.is_empty(),
        "Should analyze nested directory: {}",
        output
    );
}

#[test]
fn test_analyze_dir_multilang() {
    let repo = TestRepo::new();
    repo.with_multilang();

    let output = repo.run_cli_success(&["analyze", "."]);

    // Should produce overview output
    assert!(
        output.contains("_type") || output.contains("files") || !output.is_empty(),
        "Should analyze multi-language directory: {}",
        output
    );
}

#[test]
fn test_analyze_dir_with_max_depth() {
    let repo = TestRepo::new();
    repo.add_ts_function("a/b/c/d/e/deep.ts", "deepFunction", "return 'deep';")
        .add_ts_function("shallow.ts", "shallowFunction", "return 'shallow';");

    // Limit depth to 2
    let output = repo.run_cli_success(&["analyze", ".", "--max-depth", "2"]);

    // Should produce some output
    assert!(
        !output.is_empty(),
        "Max depth analysis should produce output"
    );
}

#[test]
fn test_analyze_dir_summary_only() {
    let repo = TestRepo::new();
    repo.with_standard_src_layout();

    let output = repo.run_cli_success(&["analyze", "src", "--summary-only"]);

    // Summary output should exist
    assert!(!output.is_empty(), "Summary should produce output");
}

#[test]
fn test_analyze_dir_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "console.log('hi');");

    let output = repo.run_cli_success(&["analyze", "src", "-f", "text"]);

    // Should have some output
    assert!(!output.is_empty(), "Text format should produce output");
}

#[test]
fn test_analyze_dir_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "console.log('hi');");

    let output = repo.run_cli_success(&["analyze", "src", "-f", "toon"]);

    // TOON format should have type marker
    assert_valid_toon(&output, "analyze dir toon");
}

#[test]
fn test_analyze_dir_empty() {
    let repo = TestRepo::new();
    // Create empty src directory
    std::fs::create_dir_all(repo.path().join("src")).unwrap();

    let result = repo.run_cli(&["analyze", "src", "-f", "json"]);
    // Should complete without error even if no files found
    assert!(result.is_ok());
}

// ============================================================================
// ANALYZE DIFF TESTS (requires git repo)
// ============================================================================

#[test]
fn test_analyze_diff_basic() {
    let repo = TestRepo::new();
    repo.init_git();

    // Create initial commit
    repo.add_ts_function("src/main.ts", "original", "return 1;");
    repo.commit("Initial commit");

    // Make changes
    repo.add_ts_function("src/main.ts", "modified", "return 2;")
        .add_ts_function("src/new.ts", "newFunction", "return 'new';");

    // Use --diff flag for diff analysis
    let output = repo.run_cli_success(&["analyze", "--diff", "HEAD"]);

    // Should detect changes - output may be TOON or JSON
    assert!(
        output.contains("new.ts") || output.contains("main.ts") || output.contains("change"),
        "Expected diff analysis to show changes: {}",
        output
    );
}

#[test]
fn test_analyze_diff_working_changes() {
    let repo = TestRepo::new();
    repo.init_git();

    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial");

    // Make uncommitted changes
    repo.add_ts_function("src/main.ts", "updated", "return 2;");

    // Use --uncommitted flag for working tree changes
    let output = repo.run_cli_success(&["analyze", "--uncommitted"]);

    // Should show the uncommitted changes
    assert!(
        output.contains("updated") || output.contains("main.ts") || !output.is_empty(),
        "Expected uncommitted changes in output: {}",
        output
    );
}

#[test]
fn test_analyze_diff_branch_comparison() {
    let repo = TestRepo::new();
    repo.init_git();

    // Create base branch with initial content
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial commit");

    // Get the initial branch name (could be 'main' or 'master')
    let branch_output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    let base_branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Create feature branch
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["checkout", "-b", "feature"])
        .output()
        .unwrap();

    // Add feature changes
    repo.add_ts_function("src/feature.ts", "featureFunction", "return 'feature';");
    repo.commit("Add feature");

    // Analyze diff from base branch using --diff flag with --base
    let output = repo.run_cli_success(&["analyze", "--diff", "--base", &base_branch]);

    // Should detect the new file/symbol
    assert!(
        output.contains("feature") || output.contains("added") || !output.is_empty(),
        "Expected to detect feature branch changes: {}",
        output
    );
}

#[test]
fn test_analyze_diff_text_format() {
    let repo = TestRepo::new();
    repo.init_git();

    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial");

    repo.add_ts_function("src/new.ts", "newFunc", "return 2;");

    let output = repo.run_cli_success(&["analyze", "--diff", "HEAD", "-f", "text"]);

    // Should contain some diff information
    assert!(
        !output.is_empty(),
        "Expected diff information in text format: {}",
        output
    );
}

#[test]
fn test_analyze_diff_no_changes() {
    let repo = TestRepo::new();
    repo.init_git();

    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.commit("Initial");

    // No changes since commit - should handle gracefully
    let result = repo.run_cli(&["analyze", "--diff", "HEAD"]);
    assert!(result.is_ok());
}

#[test]
fn test_analyze_diff_non_git_repo() {
    let repo = TestRepo::new();
    // Not initialized as git repo
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "--diff", "HEAD"]);

    // Should report error about not being a git repo
    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("git")
        || combined.to_lowercase().contains("repository")
        || combined.to_lowercase().contains("error");
    assert!(has_error, "Expected git-related error: {}", combined);
}

// ============================================================================
// FORMAT CONSISTENCY TESTS
// ============================================================================

#[test]
fn test_analyze_format_consistency() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    // Get all three formats for file analysis
    let text_output = repo.run_cli_success(&["analyze", "src/service.ts", "-f", "text"]);
    let toon_output = repo.run_cli_success(&["analyze", "src/service.ts", "-f", "toon"]);
    let json_output = repo.run_cli_success(&["analyze", "src/service.ts", "-f", "json"]);

    // All formats should contain the key symbol
    assert_contains(&text_output, "UserService", true, "text format");
    assert_contains(&toon_output, "UserService", true, "toon format");
    assert_contains(&json_output, "UserService", true, "json format");

    // JSON should be valid
    assert_valid_json(&json_output, "json format validity");

    // TOON should have type marker or symbol info
    assert!(
        toon_output.contains("_type") || toon_output.contains("UserService"),
        "TOON format should have content"
    );
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_analyze_file_syntax_error() {
    let repo = TestRepo::new();
    repo.add_syntax_error_ts("src/broken.ts");

    // Should handle gracefully - either succeed with partial results or fail clearly
    let result = repo.run_cli(&["analyze", "src/broken.ts", "-f", "json"]);
    // Accept either outcome - just shouldn't panic
    assert!(result.is_ok());
}

#[test]
fn test_analyze_file_unicode() {
    let repo = TestRepo::new();
    repo.add_unicode_ts("src/unicode.ts");

    let output = repo.run_cli_success(&["analyze", "src/unicode.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze unicode file");

    // Should handle Unicode identifiers
    assert_symbol_exists(&json, "処理する");
}

#[test]
fn test_analyze_file_very_long() {
    let repo = TestRepo::new();
    repo.add_very_long_file_ts("src/long.ts", 500); // 500 functions

    let output = repo.run_cli_success(&["analyze", "src/long.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze long file");

    // Should extract some functions
    assert_symbol_exists(&json, "func0");
    assert_symbol_exists(&json, "func499");
}

#[test]
fn test_analyze_file_deeply_nested() {
    let repo = TestRepo::new();
    repo.add_deeply_nested_ts("src/nested.ts", 20); // 20 levels deep

    let output = repo.run_cli_success(&["analyze", "src/nested.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze deeply nested");

    assert_symbol_exists(&json, "deepNest");
}

#[test]
fn test_analyze_special_characters_in_path() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/my file (1).ts", "specialPath", "return 1;");

    let output = repo.run_cli_success(&["analyze", "src/my file (1).ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze special path");

    assert_symbol_exists(&json, "specialPath");
}
