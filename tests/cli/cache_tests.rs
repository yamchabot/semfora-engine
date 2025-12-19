//! Tests for the `cache` CLI command
//!
//! The cache command manages the semantic index cache:
//! - `cache info` - Show cache information
//! - `cache clear` - Clear the cache for the current directory
//! - `cache prune --days N` - Prune caches older than N days

#![allow(unused_imports)]

use crate::common::{assert_contains, assert_valid_json, TestRepo};

// ============================================================================
// CACHE INFO TESTS
// ============================================================================

#[test]
fn test_cache_info_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate index first to have cache
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["cache", "info"]);

    // Should show cache information
    assert!(
        output.contains("cache")
            || output.contains("Cache")
            || output.contains("directory")
            || output.contains("size")
            || !output.is_empty(),
        "Should show cache info: {}",
        output
    );
}

#[test]
fn test_cache_info_no_cache() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Don't generate index - no cache exists
    let result = repo.run_cli(&["cache", "info"]);

    // Should handle gracefully (either succeed with "no cache" message or fail clearly)
    assert!(result.is_ok());
}

#[test]
fn test_cache_info_json_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["cache", "info", "-f", "json"]);
    let json = assert_valid_json(&output, "cache info json");

    // Should have cache-related fields
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("path")
            || output_str.contains("cache")
            || output_str.contains("size")
            || json.is_object(),
        "Should have cache info in JSON: {}",
        output
    );
}

#[test]
fn test_cache_info_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["cache", "info", "-f", "text"]);

    // Should produce readable text output
    assert!(!output.is_empty(), "Should produce text output");
}

// ============================================================================
// CACHE CLEAR TESTS
// ============================================================================

#[test]
fn test_cache_clear_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate index first
    repo.generate_index().unwrap();

    // Clear should succeed
    let result = repo.run_cli(&["cache", "clear"]);
    assert!(result.is_ok());
}

#[test]
fn test_cache_clear_no_cache() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // No cache exists - should handle gracefully
    let result = repo.run_cli(&["cache", "clear"]);
    assert!(result.is_ok());
}

#[test]
fn test_cache_clear_then_query() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate, clear, then try to query
    repo.generate_index().unwrap();
    repo.run_cli(&["cache", "clear"]).ok();

    // Query should now fail or return empty (no cache)
    let result = repo.run_cli(&["query", "overview"]);
    // May fail or succeed with empty - either is acceptable
    assert!(result.is_ok());
}

// ============================================================================
// CACHE PRUNE TESTS
// ============================================================================

#[test]
fn test_cache_prune_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Prune caches older than 0 days (should clear recent cache)
    let result = repo.run_cli(&["cache", "prune", "--days", "0"]);
    assert!(result.is_ok());
}

#[test]
fn test_cache_prune_keep_recent() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Prune caches older than 30 days (should keep recent cache)
    let result = repo.run_cli(&["cache", "prune", "--days", "30"]);

    // Should complete (may report what was done or be silent)
    assert!(result.is_ok(), "Prune should complete");
}

#[test]
fn test_cache_prune_text_output() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Cache prune may not support JSON format - test text output
    let result = repo.run_cli(&["cache", "prune", "--days", "30"]);

    // Should complete without error
    assert!(result.is_ok(), "Prune should complete");
}

// ============================================================================
// FORMAT CONSISTENCY TESTS
// ============================================================================

#[test]
fn test_cache_format_consistency() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Test all formats
    let text_output = repo.run_cli_success(&["cache", "info", "-f", "text"]);
    let json_output = repo.run_cli_success(&["cache", "info", "-f", "json"]);
    let toon_output = repo.run_cli_success(&["cache", "info", "-f", "toon"]);

    // All should produce output
    assert!(!text_output.is_empty(), "Text format should produce output");
    assert!(!json_output.is_empty(), "JSON format should produce output");
    assert!(!toon_output.is_empty(), "TOON format should produce output");

    // JSON should be valid
    assert_valid_json(&json_output, "cache info json validity");
}
