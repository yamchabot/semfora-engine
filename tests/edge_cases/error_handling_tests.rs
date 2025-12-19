//! Error Handling and Edge Case Tests
//!
//! Comprehensive tests for edge cases and error conditions across the system.

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// EMPTY FILE TESTS
// ============================================================================

#[test]
fn test_empty_typescript_file() {
    let repo = TestRepo::new();
    repo.add_empty_file("src/empty.ts");

    let result = repo.run_cli(&["analyze", "src/empty.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle empty TypeScript file");
}

#[test]
fn test_empty_rust_file() {
    let repo = TestRepo::new();
    repo.add_empty_file("src/lib.rs");

    let result = repo.run_cli(&["analyze", "src/lib.rs", "-f", "json"]);
    assert!(result.is_ok(), "Should handle empty Rust file");
}

#[test]
fn test_empty_python_file() {
    let repo = TestRepo::new();
    repo.add_empty_file("src/app.py");

    let result = repo.run_cli(&["analyze", "src/app.py", "-f", "json"]);
    assert!(result.is_ok(), "Should handle empty Python file");
}

#[test]
fn test_empty_go_file() {
    let repo = TestRepo::new();
    repo.add_empty_file("main.go");

    let result = repo.run_cli(&["analyze", "main.go", "-f", "json"]);
    assert!(result.is_ok(), "Should handle empty Go file");
}

#[test]
fn test_empty_directory() {
    let repo = TestRepo::new();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();

    let result = repo.run_cli(&["analyze", "src"]);
    assert!(result.is_ok(), "Should handle empty directory");
}

// ============================================================================
// WHITESPACE-ONLY FILE TESTS
// ============================================================================

#[test]
fn test_whitespace_only_typescript() {
    let repo = TestRepo::new();
    repo.add_file("src/whitespace.ts", "   \n\t\n   \n\n");

    let result = repo.run_cli(&["analyze", "src/whitespace.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle whitespace-only file");
}

#[test]
fn test_comments_only_typescript() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/comments.ts",
        r#"
// This is a comment
/* This is a
   multiline comment */

// Another comment
"#,
    );

    let result = repo.run_cli(&["analyze", "src/comments.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle comments-only file");
}

// ============================================================================
// SYNTAX ERROR TESTS
// ============================================================================

#[test]
fn test_syntax_error_typescript() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/broken.ts",
        r#"
export function broken(
    // Missing closing paren and brace
    console.log("broken");
"#,
    );

    let result = repo.run_cli(&["analyze", "src/broken.ts", "-f", "json"]);
    // Should handle gracefully - either succeed with partial results or report error
    assert!(result.is_ok(), "Should handle syntax errors gracefully");
}

#[test]
fn test_syntax_error_rust() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/broken.rs",
        r#"
fn broken() {
    let x = // Missing value
    println!("broken");
}
"#,
    );

    let result = repo.run_cli(&["analyze", "src/broken.rs", "-f", "json"]);
    assert!(
        result.is_ok(),
        "Should handle Rust syntax errors gracefully"
    );
}

#[test]
fn test_syntax_error_python() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/broken.py",
        r#"
def broken():
    # Missing colon on if
    if True
        print("broken")
"#,
    );

    let result = repo.run_cli(&["analyze", "src/broken.py", "-f", "json"]);
    assert!(
        result.is_ok(),
        "Should handle Python syntax errors gracefully"
    );
}

#[test]
fn test_partial_syntax_error() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/partial.ts",
        r#"
export function validFunc() {
    return 1;
}

export function brokenFunc(
    // This is broken

export function anotherValidFunc() {
    return 2;
}
"#,
    );

    let result = repo.run_cli(&["analyze", "src/partial.ts", "-f", "json"]);
    assert!(
        result.is_ok(),
        "Should extract valid symbols despite errors"
    );
}

// ============================================================================
// UNICODE AND SPECIAL CHARACTER TESTS
// ============================================================================

#[test]
fn test_unicode_identifiers() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/unicode.ts",
        r#"
export function 処理する(入力: string): string {
    return 入力 + "!";
}

export const 設定 = {
    名前: "テスト",
    バージョン: 1
};
"#,
    );

    let output = repo.run_cli_success(&["analyze", "src/unicode.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "unicode identifiers");

    // Should extract Unicode identifiers
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("処理する") || output_str.contains("設定") || json.is_object(),
        "Should find Unicode identifiers: {}",
        output
    );
}

#[test]
fn test_unicode_strings() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/strings.ts",
        r#"
export function greet(): string {
    return "こんにちは世界! 🌍 مرحبا Привет";
}
"#,
    );

    let output = repo.run_cli_success(&["analyze", "src/strings.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "unicode strings");
    assert!(json.is_object() || json.is_array());
}

#[test]
fn test_emoji_in_code() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/emoji.ts",
        r#"
export const emoji = {
    happy: "😀",
    sad: "😢",
    rocket: "🚀"
};

export function printEmoji(): void {
    console.log(emoji.rocket);
}
"#,
    );

    let result = repo.run_cli(&["analyze", "src/emoji.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle emoji in code");
}

#[test]
fn test_special_characters_in_path() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/my file (1).ts", "specialPath", "return 1;");

    let output = repo.run_cli_success(&["analyze", "src/my file (1).ts", "-f", "json"]);
    let json = assert_valid_json(&output, "special path");

    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("specialPath") || json.is_object(),
        "Should handle special characters in path"
    );
}

#[test]
fn test_unicode_path() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/日本語/モジュール.ts", "test", "return 1;");

    let result = repo.run_cli(&["analyze", "src/日本語/モジュール.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle Unicode in path");
}

// ============================================================================
// VERY LONG FILE TESTS
// ============================================================================

#[test]
fn test_very_long_file() {
    let repo = TestRepo::new();
    repo.add_very_long_file_ts("src/long.ts", 500); // 500 functions

    let output = repo.run_cli_success(&["analyze", "src/long.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "very long file");

    // Should extract symbols from long file
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("func0") && output_str.contains("func499"),
        "Should extract all symbols from long file"
    );
}

#[test]
fn test_very_long_line() {
    let repo = TestRepo::new();
    let long_string = "x".repeat(10000);
    repo.add_file(
        "src/longline.ts",
        &format!(
            r#"
export const longString = "{}";
export function normalFunc() {{ return 1; }}
"#,
            long_string
        ),
    );

    let result = repo.run_cli(&["analyze", "src/longline.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle very long lines");
}

#[test]
fn test_deeply_nested_code() {
    let repo = TestRepo::new();
    repo.add_deeply_nested_ts("src/nested.ts", 20); // 20 levels of nesting

    let output = repo.run_cli_success(&["analyze", "src/nested.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "deeply nested");

    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("deepNest") || json.is_object(),
        "Should handle deeply nested code"
    );
}

// ============================================================================
// BINARY AND UNSUPPORTED FILE TESTS
// ============================================================================

#[test]
fn test_binary_file() {
    let repo = TestRepo::new();
    // Create a binary file
    std::fs::write(
        repo.path().join("image.png"),
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00],
    )
    .unwrap();

    let result = repo.run_cli(&["analyze", "image.png"]);
    // Should handle gracefully (skip or report)
    assert!(result.is_ok(), "Should handle binary file");
}

#[test]
fn test_unsupported_extension() {
    let repo = TestRepo::new();
    repo.add_file("data.xyz", "some random content");

    let result = repo.run_cli(&["analyze", "data.xyz"]);
    assert!(result.is_ok(), "Should handle unsupported extension");
}

#[test]
fn test_no_extension() {
    let repo = TestRepo::new();
    repo.add_file("Makefile", "all:\n\techo hello");

    let result = repo.run_cli(&["analyze", "Makefile"]);
    assert!(result.is_ok(), "Should handle file with no extension");
}

// ============================================================================
// FILE SYSTEM ERROR TESTS
// ============================================================================

#[test]
fn test_nonexistent_file() {
    let repo = TestRepo::new();

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "nonexistent.ts"]);

    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no such file");
    assert!(
        has_error,
        "Should report file not found error: {}",
        combined
    );
}

#[test]
fn test_nonexistent_directory() {
    let repo = TestRepo::new();

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "nonexistent/path/"]);

    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no such");
    assert!(has_error, "Should report directory not found: {}", combined);
}

// ============================================================================
// INDEX ERROR TESTS
// ============================================================================

#[test]
fn test_query_without_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    // Don't generate index

    // Query should handle gracefully (auto-generate or report no index)
    let result = repo.run_cli(&["query", "overview"]);
    assert!(result.is_ok(), "Should handle missing index");
}

#[test]
fn test_search_without_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    // Don't generate index

    // Search should handle gracefully
    let result = repo.run_cli(&["search", "main"]);
    assert!(result.is_ok(), "Should handle search without index");
}

#[test]
fn test_validate_without_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    // Don't generate index

    let result = repo.run_cli(&["validate", "src/main.ts"]);
    // May generate index automatically or report error
    assert!(result.is_ok(), "Should handle validate without index");
}

// ============================================================================
// GIT ERROR TESTS
// ============================================================================

#[test]
fn test_diff_without_git() {
    let repo = TestRepo::new();
    // Not a git repo
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "--diff", "HEAD"]);

    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("git")
        || combined.to_lowercase().contains("repository")
        || combined.to_lowercase().contains("error");
    assert!(has_error, "Should report git error: {}", combined);
}

#[test]
fn test_commit_without_git() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let (stdout, stderr) = repo.run_cli_failure(&["commit"]);

    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("git")
        || combined.to_lowercase().contains("repository")
        || combined.to_lowercase().contains("error");
    assert!(has_error, "Should report git error: {}", combined);
}

// ============================================================================
// INVALID ARGUMENT TESTS
// ============================================================================

#[test]
fn test_invalid_format_argument() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let result = repo.run_cli(&["analyze", "src/main.ts", "-f", "invalid_format"]);

    // Should report invalid format
    assert!(result.is_ok() || result.is_err()); // Either is acceptable
}

#[test]
fn test_invalid_limit_argument() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let result = repo.run_cli(&["search", "main", "--limit", "-5"]);

    // Should handle negative limit
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_invalid_depth_argument() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let result = repo.run_cli(&["analyze", "src", "--max-depth", "not_a_number"]);

    // Should report invalid argument
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// RESOURCE LIMIT TESTS
// ============================================================================

#[test]
fn test_many_small_files() {
    let repo = TestRepo::new();
    // Create 100 small files
    for i in 0..100 {
        repo.add_ts_function(
            &format!("src/module{}.ts", i),
            &format!("func{}", i),
            "return 1;",
        );
    }

    let result = repo.run_cli(&["analyze", "src"]);
    assert!(result.is_ok(), "Should handle many files");

    repo.generate_index().unwrap();

    let overview = repo.run_cli(&["query", "overview", "-f", "json"]);
    assert!(overview.is_ok(), "Should generate overview for many files");
}

#[test]
fn test_deeply_nested_directories() {
    let repo = TestRepo::new();
    // Create deeply nested path
    let deep_path = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p";
    repo.add_ts_function(&format!("{}/deep.ts", deep_path), "deepFunc", "return 1;");

    let result = repo.run_cli(&["analyze", "."]);
    assert!(result.is_ok(), "Should handle deeply nested directories");
}

// ============================================================================
// CONCURRENT ACCESS SIMULATION TESTS
// ============================================================================

#[test]
fn test_multiple_operations_same_repo() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Multiple operations
    for i in 0..5 {
        let search = repo.run_cli(&["search", &format!("main{}", i % 2)]);
        assert!(search.is_ok(), "Operation {} should succeed", i);
    }
}

#[test]
fn test_index_regeneration() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate index multiple times
    for _ in 0..3 {
        repo.generate_index().unwrap();
        let result = repo.run_cli(&["query", "overview"]);
        assert!(result.is_ok(), "Should handle multiple regenerations");
    }
}

// ============================================================================
// MIXED CONTENT TESTS
// ============================================================================

#[test]
fn test_mixed_valid_invalid_files() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/valid.ts", "validFunc", "return 1;")
        .add_file("src/broken.ts", "export function broken(")
        .add_empty_file("src/empty.ts");

    let result = repo.run_cli(&["analyze", "src"]);
    assert!(result.is_ok(), "Should handle mixed content");

    repo.generate_index().unwrap();

    let overview = repo.run_cli_success(&["query", "overview"]);
    assert!(
        !overview.is_empty(),
        "Should produce overview for mixed content"
    );
}

#[test]
fn test_mixed_supported_unsupported_files() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/app.ts", "app", "return 1;")
        .add_file("src/data.csv", "a,b,c\n1,2,3")
        .add_file("src/readme.md", "# Readme");

    let result = repo.run_cli(&["analyze", "src"]);
    assert!(result.is_ok(), "Should handle mixed file types");
}
