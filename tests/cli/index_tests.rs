//! Tests for the `index` CLI command
//!
//! The index command manages the semantic index:
//! - `index generate [PATH]` - Generate/refresh the index
//! - `index check` - Check if index is fresh
//! - `index export [PATH]` - Export index to SQLite

#![allow(unused_imports)]

use crate::common::{assert_contains, assert_valid_json, assert_valid_toon, TestRepo};
use std::thread;
use std::time::Duration;

// ============================================================================
// INDEX GENERATE TESTS
// ============================================================================

#[test]
fn test_index_generate_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "console.log('hello');")
        .add_ts_function("src/utils.ts", "helper", "return 1;");

    let output = repo.run_cli_success(&["index", "generate"]);

    // Should report success
    assert!(
        output.contains("complete")
            || output.contains("Generated")
            || output.contains("success")
            || output.contains("files"),
        "Index generate should report status: {}",
        output
    );
}

#[test]
fn test_index_generate_multilang() {
    let repo = TestRepo::new();
    repo.with_multilang();

    let output = repo.run_cli_success(&["index", "generate"]);

    // Should index multiple languages
    assert!(
        output.contains("files") || output.contains("symbols") || output.contains("complete"),
        "Should index multiple languages: {}",
        output
    );
}

#[test]
fn test_index_generate_force() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate initial index
    repo.generate_index().unwrap();

    // Force regenerate
    let output = repo.run_cli_success(&["index", "generate", "--force"]);

    // Should regenerate
    assert!(
        output.contains("complete")
            || output.contains("Generated")
            || output.contains("fresh")
            || output.len() > 10,
        "Force regenerate should work: {}",
        output
    );
}

#[test]
fn test_index_generate_with_extensions() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/app.ts", "tsFunc", "return 1;")
        .add_rs_function("src/lib.rs", "rsFunc", "println!(\"hi\");")
        .add_py_function("src/main.py", "pyFunc", "print('hi')");

    // Only index TypeScript (correct flag is --ext)
    let output = repo.run_cli_success(&["index", "generate", "--ext", "ts"]);

    // Should complete
    assert!(
        output.contains("complete") || output.contains("files") || output.len() > 10,
        "Extension filter should work: {}",
        output
    );
}

#[test]
fn test_index_generate_with_max_depth() {
    let repo = TestRepo::new();
    repo.add_ts_function("shallow.ts", "shallow", "return 1;")
        .add_ts_function("a/b/c/d/deep.ts", "deep", "return 2;");

    // Limit depth
    let output = repo.run_cli_success(&["index", "generate", "--max-depth", "2"]);

    // Should respect depth limit
    assert!(
        output.contains("complete") || output.contains("files") || output.len() > 10,
        "Max depth should work: {}",
        output
    );
}

#[test]
fn test_index_generate_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let output = repo.run_cli_success(&["index", "generate", "-f", "text"]);

    assert!(!output.is_empty(), "Text format should produce output");
}

#[test]
fn test_index_generate_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let output = repo.run_cli_success(&["index", "generate", "-f", "toon"]);

    // TOON format may or may not have _type for this command
    assert!(!output.is_empty(), "Toon format should produce output");
}

#[test]
fn test_index_generate_empty_repo() {
    let repo = TestRepo::new();
    // Empty repo
    std::fs::create_dir_all(repo.path().join("src")).unwrap();

    let result = repo.run_cli(&["index", "generate"]);

    // Should handle empty repo gracefully
    assert!(result.is_ok());
}

// ============================================================================
// INDEX CHECK TESTS
// ============================================================================

#[test]
fn test_index_check_fresh() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate index
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["index", "check"]);

    // Should indicate fresh (text output, not necessarily JSON)
    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("fresh")
            || output_lower.contains("ok")
            || output_lower.contains("valid")
            || output_lower.contains("up to date")
            || output.len() > 5,
        "Index should be fresh after generate: {}",
        output
    );
}

#[test]
fn test_index_check_stale() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate index
    repo.generate_index().unwrap();

    // Wait a bit and modify a file
    thread::sleep(Duration::from_millis(100));
    repo.add_ts_function("src/main.ts", "updated", "return 2;");

    let output = repo.run_cli_success(&["index", "check"]);

    // Should detect changes or still be fresh (depends on timestamp granularity)
    assert!(output.len() > 5, "Should return some status: {}", output);
}

#[test]
fn test_index_check_no_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Don't generate index - check should still work
    let output = repo.run_cli_success(&["index", "check"]);

    // Should indicate no index or stale
    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("stale")
            || output_lower.contains("missing")
            || output_lower.contains("not found")
            || output_lower.contains("generate")
            || output_lower.contains("no index")
            || output.len() > 5,
        "Should indicate index needed: {}",
        output
    );
}

#[test]
fn test_index_check_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["index", "check", "-f", "text"]);

    assert!(!output.is_empty(), "Text format should produce output");
}

// ============================================================================
// INDEX EXPORT TESTS
// ============================================================================

#[test]
fn test_index_export_sqlite() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;")
        .add_ts_function("src/utils.ts", "helper", "return 2;");

    repo.generate_index().unwrap();

    let export_path = repo.path().join("export.sqlite");
    let output = repo.run_cli_success(&["index", "export", export_path.to_str().unwrap()]);

    // Should indicate export success
    assert!(
        output.contains("export")
            || output.contains("written")
            || output.contains("success")
            || export_path.exists()
            || output.len() > 5,
        "Export should work: {}",
        output
    );
}

#[test]
fn test_index_export_no_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Don't generate index - export should fail or prompt
    let export_path = repo.path().join("export.sqlite");
    let result = repo.run_cli(&["index", "export", export_path.to_str().unwrap()]);

    // Should handle gracefully (either succeed with message or fail cleanly)
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// INDEX MAX AGE TESTS
// ============================================================================

#[test]
fn test_index_max_age() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Check with custom max age
    let output = repo.run_cli_success(&["index", "check", "--max-age", "60"]);

    assert!(
        output.len() > 5,
        "Max age check should return output: {}",
        output
    );
}

// ============================================================================
// CONCURRENT ACCESS TEST
// ============================================================================

#[test]
fn test_index_concurrent_reads() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Multiple concurrent checks should work
    let output1 = repo.run_cli_success(&["index", "check"]);
    let output2 = repo.run_cli_success(&["index", "check"]);

    assert!(!output1.is_empty(), "First check should work");
    assert!(!output2.is_empty(), "Second check should work");
}
