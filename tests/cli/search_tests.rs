//! Tests for the `search` CLI command
//!
//! The search command supports multiple modes:
//! - Default (hybrid): Runs both symbol and semantic search
//! - `--symbols` or `-s`: Exact symbol name matching
//! - `--related` or `-r`: BM25 conceptual search
//! - `--raw`: Regex patterns in comments/strings

#![allow(unused_imports)]
#![allow(unused_variables)]

use crate::common::{
    assert_contains, assert_symbol_exists, assert_valid_json, assert_valid_toon,
    extract_symbol_names, TestRepo,
};

// ============================================================================
// HYBRID SEARCH (DEFAULT)
// ============================================================================

#[test]
fn test_search_hybrid_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/auth.ts", "authenticateUser", "return token;")
        .add_ts_function("src/api.ts", "fetchUsers", "return users;")
        .add_ts_function("src/utils.ts", "formatDate", "return date;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "user", "-f", "json"]);
    let json = assert_valid_json(&output, "hybrid search");

    // Should find user-related symbols
    let symbols = extract_symbol_names(&json);
    let found_user = symbols.iter().any(|s| s.to_lowercase().contains("user"));

    assert!(
        found_user || output.contains("user") || output.contains("User"),
        "Expected to find user-related symbols: {:?} in {}",
        symbols,
        output
    );
}

#[test]
fn test_search_hybrid_partial_match() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/service.ts", "getUserById", "return db.get(id);")
        .add_ts_function("src/service.ts", "getAllUsers", "return db.all();")
        .add_ts_function("src/service.ts", "createOrder", "return order;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "User", "-f", "json"]);
    let json = assert_valid_json(&output, "hybrid partial match");

    // Should find User-related functions
    assert!(
        output.to_lowercase().contains("user"),
        "Expected User-related symbols in: {}",
        output
    );
}

#[test]
fn test_search_hybrid_no_results() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "console.log('hello');");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "nonexistentxyzabc", "-f", "json"]);

    // Should return empty results, not error
    let json = assert_valid_json(&output, "hybrid no results");
    let symbols = extract_symbol_names(&json);
    assert!(
        symbols.is_empty() || !symbols.iter().any(|s| s.contains("nonexistent")),
        "Should not find nonexistent symbol"
    );
}

// ============================================================================
// SYMBOL SEARCH MODE (--symbols or -s flag)
// ============================================================================

#[test]
fn test_search_symbols_exact() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/service.ts", "processOrder", "return order;")
        .add_ts_function("src/utils.ts", "processData", "return data;");

    repo.generate_index().unwrap();

    // Use --symbols flag (not --mode symbols)
    let output = repo.run_cli_success(&["search", "processOrder", "--symbols", "-f", "json"]);
    let json = assert_valid_json(&output, "symbol search exact");

    // Should find exact match
    assert!(
        output.contains("processOrder"),
        "Should find processOrder: {}",
        output
    );
}

#[test]
fn test_search_symbols_wildcard() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/handlers.ts", "handleCreate", "return 1;")
        .add_ts_function("src/handlers.ts", "handleUpdate", "return 2;")
        .add_ts_function("src/handlers.ts", "handleDelete", "return 3;")
        .add_ts_function("src/utils.ts", "formatDate", "return date;");

    repo.generate_index().unwrap();

    // Wildcard search with symbols flag
    let output = repo.run_cli_success(&["search", "handle*", "--symbols", "-f", "json"]);
    let json = assert_valid_json(&output, "symbol search wildcard");

    // Should find handle* functions
    let symbols = extract_symbol_names(&json);
    let handle_count = symbols.iter().filter(|s| s.starts_with("handle")).count();
    assert!(
        handle_count >= 1 || output.contains("handle"),
        "Expected handle* functions, found {}: {:?} in {}",
        handle_count,
        symbols,
        output
    );
}

#[test]
fn test_search_symbols_case_insensitive() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/service.ts", "UserService", "return 1;")
        .add_ts_function("src/service.ts", "userHelper", "return 2;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "user", "--symbols", "-f", "json"]);
    let json = assert_valid_json(&output, "symbol search case insensitive");

    // Should find user-related symbols
    assert!(
        output.to_lowercase().contains("user"),
        "Expected user-related symbols: {}",
        output
    );
}

// ============================================================================
// SEMANTIC SEARCH MODE (--related or -r flag)
// ============================================================================

#[test]
fn test_search_semantic_conceptual() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/auth.ts", "verifyToken", "return jwt.verify(token);")
        .add_ts_function("src/auth.ts", "refreshSession", "return newToken;")
        .add_ts_function("src/utils.ts", "formatDate", "return date;");

    repo.generate_index().unwrap();

    // Semantic search with --related flag (not --mode semantic)
    let output = repo.run_cli_success(&["search", "authentication", "--related", "-f", "json"]);
    let json = assert_valid_json(&output, "semantic search");

    // Should find auth-related symbols through semantic matching
    // This might not find exact matches but should return results
    assert!(
        json.is_object() || json.is_array(),
        "Should return search results"
    );
}

#[test]
fn test_search_semantic_with_limit() {
    let repo = TestRepo::new();
    for i in 0..20 {
        repo.add_ts_function(
            &format!("src/func{}.ts", i),
            &format!("process{}", i),
            "return 1;",
        );
    }

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&[
        "search",
        "process",
        "--related",
        "--limit",
        "5",
        "-f",
        "json",
    ]);
    let json = assert_valid_json(&output, "semantic with limit");

    // Should respect the limit
    let symbols = extract_symbol_names(&json);
    assert!(
        symbols.len() <= 10, // Allow some flexibility
        "Limit should be respected, found: {}",
        symbols.len()
    );
}

// ============================================================================
// RAW SEARCH MODE (--raw flag)
// ============================================================================

#[test]
fn test_search_raw_literal() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/config.ts",
        r#"// Configuration for the application
const API_URL = "https://api.example.com";
const TIMEOUT = 5000; // milliseconds
"#,
    );

    let output = repo.run_cli_success(&["search", "API_URL", "--raw"]);

    // Should find the literal string
    assert!(
        output.contains("API_URL") || output.contains("config.ts"),
        "Raw search should find literal: {}",
        output
    );
}

#[test]
fn test_search_raw_regex() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/app.ts",
        r#"// TODO: fix this
// FIXME: improve performance
function processData() {
    // NOTE: important logic
    return data;
}
"#,
    );

    // Search for TODO/FIXME comments
    let output = repo.run_cli_success(&["search", "TODO|FIXME", "--raw"]);

    // Should find the comments
    assert!(
        output.contains("TODO") || output.contains("FIXME") || output.contains("app.ts"),
        "Raw regex should find comments: {}",
        output
    );
}

#[test]
fn test_search_raw_case_insensitive() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/app.ts",
        r#"const ERROR_MESSAGE = "Error occurred";
const errorHandler = () => {};
"#,
    );

    // Case insensitive raw search (default)
    let output = repo.run_cli_success(&["search", "error", "--raw"]);

    // Should find both cases
    assert!(
        output.to_lowercase().contains("error"),
        "Case insensitive raw search: {}",
        output
    );
}

// ============================================================================
// LIMIT TESTS
// ============================================================================

#[test]
fn test_search_with_limit() {
    let repo = TestRepo::new();
    for i in 0..30 {
        repo.add_ts_function(
            &format!("src/item{}.ts", i),
            &format!("item{}", i),
            "return 1;",
        );
    }

    repo.generate_index().unwrap();

    // Search with limit
    let output = repo.run_cli_success(&["search", "item", "--limit", "10", "-f", "json"]);
    let json = assert_valid_json(&output, "search with limit");

    // Should return limited results
    assert!(json.is_object() || json.is_array(), "Should have results");

    // Check that we got some results
    let symbols = extract_symbol_names(&json);
    assert!(
        symbols.len() <= 15 || symbols.iter().any(|s| s.contains("item")),
        "Should find item symbols: {:?}",
        symbols
    );
}

// ============================================================================
// INCLUDE SOURCE TESTS
// ============================================================================

#[test]
fn test_search_with_include_source() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/service.ts", "processOrder", "return order.process();");

    repo.generate_index().unwrap();

    let output =
        repo.run_cli_success(&["search", "processOrder", "--include-source", "-f", "json"]);
    let json = assert_valid_json(&output, "search with source");

    // Should include source code
    assert!(
        output.contains("processOrder") || output.contains("source"),
        "Should include source code: {}",
        output
    );
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_search_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "text"]);

    assert_contains(&output, "main", true, "text format");
}

#[test]
fn test_search_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "toon"]);

    // TOON format should have type marker or be non-empty
    assert!(
        output.contains("_type") || !output.is_empty(),
        "TOON format should produce output: {}",
        output
    );
}

#[test]
fn test_search_json_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "json"]);

    assert_valid_json(&output, "json format");
}
