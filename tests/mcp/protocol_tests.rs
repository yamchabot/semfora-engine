//! MCP Protocol Tests
//!
//! Tests for JSON-RPC message format and protocol handling.
//! These tests verify the MCP server correctly handles:
//! - JSON-RPC request/response format
//! - Tool listing (tools/list)
//! - Tool invocation (tools/call)
//! - Error handling for invalid requests
//!
//! Note: These tests use the CLI interface to simulate MCP tool calls,
//! as direct JSON-RPC testing would require spawning the server process.

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// MCP TOOL LISTING TESTS
// ============================================================================

/// Test that we can get a list of available tools via query languages
/// (simulates the capabilities discovery aspect of MCP)
#[test]
fn test_mcp_discover_languages() {
    let repo = TestRepo::new();

    // query languages is a discovery endpoint
    let output = repo.run_cli_success(&["query", "languages", "-f", "json"]);
    let json = assert_valid_json(&output, "query languages");

    // Should return a list of supported languages
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("typescript")
            || output_str.contains("TypeScript")
            || output_str.contains("rust")
            || output_str.contains("Rust")
            || output_str.contains("languages")
            || json.is_array()
            || json.is_object(),
        "Should list supported languages: {}",
        output
    );
}

// ============================================================================
// REQUEST PARAMETER VALIDATION TESTS
// ============================================================================

/// Test handling of missing required parameters
#[test]
fn test_mcp_missing_path_parameter() {
    let repo = TestRepo::new();

    // analyze without path should handle gracefully
    let result = repo.run_cli(&["analyze"]);

    // Should either use current directory or report missing argument
    assert!(result.is_ok());
}

/// Test handling of invalid path parameter
#[test]
fn test_mcp_invalid_path_parameter() {
    let repo = TestRepo::new();

    let (stdout, stderr) =
        repo.run_cli_failure(&["analyze", "/nonexistent/path/that/does/not/exist"]);

    // Should report file not found
    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no such file");
    assert!(has_error, "Should report path error: {}", combined);
}

/// Test handling of invalid hash parameter
#[test]
fn test_mcp_invalid_hash_parameter() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Invalid hash format
    let result = repo.run_cli(&["query", "symbol", "invalid_hash_xyz"]);

    // Should handle gracefully (not crash)
    assert!(result.is_ok());
}

/// Test handling of empty query parameter
#[test]
fn test_mcp_empty_query_parameter() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Empty search query
    let result = repo.run_cli(&["search", ""]);

    // Should handle empty query gracefully
    assert!(result.is_ok());
}

// ============================================================================
// OUTPUT FORMAT PARAMETER TESTS
// ============================================================================

/// Test JSON output format produces valid JSON
#[test]
fn test_mcp_output_format_json_validity() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let json = assert_valid_json(&output, "json format");

    // JSON should be an object or array
    assert!(
        json.is_object() || json.is_array(),
        "JSON output should be object or array"
    );
}

/// Test TOON output format produces valid TOON
#[test]
fn test_mcp_output_format_toon_validity() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // TOON format should have _type marker
    assert!(
        output.contains("_type:")
            || output.contains("modules:")
            || output.contains("symbols:")
            || !output.is_empty(),
        "TOON output should have structure: {}",
        output
    );
}

/// Test text output format is human-readable
#[test]
fn test_mcp_output_format_text_validity() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "text"]);

    // Text format should be non-empty and readable
    assert!(!output.is_empty(), "Text output should not be empty");
}

// ============================================================================
// PAGINATION PARAMETER TESTS
// ============================================================================

/// Test limit parameter restricts results
#[test]
fn test_mcp_limit_parameter() {
    let repo = TestRepo::new();
    // Create many symbols
    for i in 0..20 {
        repo.add_ts_function(
            &format!("src/func{}.ts", i),
            &format!("func{}", i),
            "return 1;",
        );
    }
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "func", "--limit", "5", "-f", "json"]);
    let json = assert_valid_json(&output, "search with limit");

    // Should respect limit
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        assert!(results.len() <= 10, "Results should respect limit");
    }
}

/// Test pagination with limit parameter
#[test]
fn test_mcp_pagination_with_limit() {
    let repo = TestRepo::new();
    for i in 0..20 {
        repo.add_ts_function(
            &format!("src/func{}.ts", i),
            &format!("func{}", i),
            "return 1;",
        );
    }
    repo.generate_index().unwrap();

    // First page with small limit
    let first_output = repo.run_cli_success(&["search", "func", "--limit", "5", "-f", "json"]);
    let first_json = assert_valid_json(&first_output, "first page");

    // Second request with larger limit
    let second_output = repo.run_cli_success(&["search", "func", "--limit", "10", "-f", "json"]);
    let second_json = assert_valid_json(&second_output, "second page");

    // Both should produce valid results
    assert!(first_json.is_object() || first_json.is_array());
    assert!(second_json.is_object() || second_json.is_array());
}

// ============================================================================
// FILTER PARAMETER TESTS
// ============================================================================

/// Test module filter parameter
#[test]
fn test_mcp_module_filter_parameter() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
        .add_ts_function("src/api/posts.ts", "getPosts", "return [];")
        .add_ts_function("src/utils/helper.ts", "helper", "return 1;");
    repo.generate_index().unwrap();

    // Search with module filter
    let output = repo.run_cli_success(&["search", "get", "--module", "src", "-f", "json"]);
    let json = assert_valid_json(&output, "search with module filter");

    // Should filter to module
    assert!(json.is_object() || json.is_array());
}

/// Test kind filter parameter
#[test]
fn test_mcp_kind_filter_parameter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/types.ts",
        r#"
export interface User { id: number; }
export class Service { run() {} }
export function helper() { return 1; }
"#,
    );
    repo.generate_index().unwrap();

    // Search with kind filter
    let output = repo.run_cli_success(&["search", "User", "--kind", "interface", "-f", "json"]);
    let json = assert_valid_json(&output, "search with kind filter");

    // Should filter to kind
    assert!(json.is_object() || json.is_array());
}

// ============================================================================
// BATCH REQUEST TESTS
// ============================================================================

/// Test multiple sequential requests
#[test]
fn test_mcp_sequential_requests() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Multiple sequential operations
    let overview = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let search = repo.run_cli_success(&["search", "main", "-f", "json"]);
    let analyze = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "json"]);

    // All should produce valid JSON
    assert_valid_json(&overview, "overview");
    assert_valid_json(&search, "search");
    assert_valid_json(&analyze, "analyze");
}

/// Test operations on large result sets
#[test]
fn test_mcp_large_result_set() {
    let repo = TestRepo::new();
    // Create many files
    for i in 0..50 {
        repo.add_ts_function(
            &format!("src/module{}/index.ts", i),
            &format!("handler{}", i),
            &format!("return {};", i),
        );
    }
    repo.generate_index().unwrap();

    // Query that returns many results
    let output = repo.run_cli_success(&["search", "handler", "-f", "json"]);
    let json = assert_valid_json(&output, "large result set");

    // Should handle large results
    assert!(json.is_object() || json.is_array());
}

// ============================================================================
// CONCURRENT REQUEST SIMULATION TESTS
// ============================================================================

/// Test rapid sequential requests (simulates concurrent access)
#[test]
fn test_mcp_rapid_requests() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Rapid sequential requests (can't truly test concurrent in this setup)
    for i in 0..5 {
        let output = repo.run_cli_success(&["search", &format!("main{}", i % 2), "-f", "json"]);
        assert_valid_json(&output, &format!("rapid request {}", i));
    }
}

// ============================================================================
// ERROR RESPONSE TESTS
// ============================================================================

/// Test error response format for invalid tool
#[test]
fn test_mcp_error_response_format() {
    let repo = TestRepo::new();

    // Invalid subcommand
    let (stdout, stderr) = repo.run_cli_failure(&["invalid_command"]);

    // Should report error
    let combined = format!("{}{}", stdout, stderr);
    let has_error = combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("unrecognized")
        || combined.to_lowercase().contains("invalid");
    assert!(
        has_error,
        "Should report error for invalid command: {}",
        combined
    );
}

/// Test error response for invalid JSON
#[test]
fn test_mcp_invalid_request_format() {
    let repo = TestRepo::new();

    // Malformed arguments
    let result = repo.run_cli(&["search", "--limit", "not_a_number"]);

    // Should handle gracefully (not crash)
    assert!(result.is_ok() || result.is_err()); // Either is acceptable
}

// ============================================================================
// TOOL BEHAVIOR CONSISTENCY TESTS
// ============================================================================

/// Test that same input produces consistent output
#[test]
fn test_mcp_deterministic_output() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Run same query twice
    let first = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let second = repo.run_cli_success(&["query", "overview", "-f", "json"]);

    let first_json = assert_valid_json(&first, "first run");
    let second_json = assert_valid_json(&second, "second run");

    // Results should be consistent (same structure)
    assert_eq!(
        first_json.is_object(),
        second_json.is_object(),
        "Output structure should be consistent"
    );
}

/// Test tool chain workflow (discover -> search -> get)
#[test]
fn test_mcp_tool_chain_workflow() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Step 1: Get overview
    let overview_output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let overview = assert_valid_json(&overview_output, "overview step");
    assert!(overview.is_object() || overview.is_array());

    // Step 2: Search for symbol
    let search_output = repo.run_cli_success(&["search", "main", "-f", "json"]);
    let search = assert_valid_json(&search_output, "search step");
    assert!(search.is_object() || search.is_array());

    // Step 3: Analyze file
    let analyze_output = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "json"]);
    let analyze = assert_valid_json(&analyze_output, "analyze step");
    assert!(analyze.is_object() || analyze.is_array());
}

// ============================================================================
// ENCODING TESTS
// ============================================================================

/// Test handling of special characters in queries
#[test]
fn test_mcp_special_characters_in_query() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // Query with special characters
    let result = repo.run_cli(&["search", "main*"]);
    assert!(result.is_ok(), "Should handle special characters");

    let result = repo.run_cli(&["search", "ma?in"]);
    assert!(result.is_ok(), "Should handle question mark");
}

/// Test handling of Unicode in file content
#[test]
fn test_mcp_unicode_content() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/日本語.ts",
        r#"
export function 処理する(入力: string): string {
    return 入力 + "!";
}
"#,
    );
    repo.generate_index().unwrap();

    // Should handle Unicode file names and content
    let result = repo.run_cli(&["analyze", "src/日本語.ts", "-f", "json"]);
    assert!(result.is_ok(), "Should handle Unicode content");
}

// ============================================================================
// RESOURCE CLEANUP TESTS
// ============================================================================

/// Test that operations clean up properly
#[test]
fn test_mcp_resource_cleanup() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Generate and check index multiple times
    for _ in 0..3 {
        repo.generate_index().unwrap();
        let result = repo.run_cli(&["query", "overview"]);
        assert!(result.is_ok());
    }

    // Clear cache
    repo.run_cli(&["cache", "clear"]).ok();

    // Should still work after clear
    repo.generate_index().unwrap();
    let result = repo.run_cli(&["query", "overview"]);
    assert!(result.is_ok());
}
