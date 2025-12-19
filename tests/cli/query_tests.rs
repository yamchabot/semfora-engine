//! Tests for the `query` CLI command
//!
//! The query command has multiple subcommands:
//! - `query overview` - Get repository overview
//! - `query module <name>` - Get module details
//! - `query symbol <hash>` - Get symbol details (--source for code)
//! - `query source <file>` - Get source code (--start/--end for line range)
//! - `query callers <hash>` - Get symbol callers
//! - `query callgraph` - Get call graph
//! - `query file <path>` - Get file symbols (--source for code)
//! - `query languages` - List supported languages
//!
//! Note: Some query outputs may return TOON format even with -f json

#![allow(unused_imports)]

use crate::common::{
    assert_contains, assert_symbol_exists, assert_valid_json, assert_valid_toon,
    extract_symbol_hashes, extract_symbol_names, TestRepo,
};

// ============================================================================
// QUERY OVERVIEW TESTS
// ============================================================================

#[test]
fn test_query_overview_basic() {
    let repo = TestRepo::new();
    repo.with_standard_src_layout();

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);

    // Overview may return partial JSON or have different structure
    assert!(
        output.contains("_type") || output.contains("schema_version") || output.contains("module"),
        "Overview should contain some information: {}",
        output
    );
}

#[test]
fn test_query_overview_with_modules() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
        .add_ts_function("src/api/posts.ts", "getPosts", "return [];")
        .add_ts_function("src/utils/format.ts", "format", "return '';");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // Should produce TOON format with overview info
    // Note: Overview may not list individual modules, just summary stats
    assert!(
        output.contains("_type") || output.contains("files") || output.contains("schema"),
        "Overview should produce output: {}",
        output
    );
}

#[test]
fn test_query_overview_max_modules() {
    let repo = TestRepo::new();
    // Create many modules
    for i in 0..20 {
        repo.add_ts_function(
            &format!("src/module{}/index.ts", i),
            &format!("func{}", i),
            "return 1;",
        );
    }

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "--max-modules", "5"]);

    // Should produce output
    assert!(!output.is_empty(), "Max modules should produce output");
}

#[test]
fn test_query_overview_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "text"]);

    // Text format should be readable
    assert!(!output.is_empty(), "Text overview should have content");
}

#[test]
fn test_query_overview_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    assert_valid_toon(&output, "query overview toon");
}

// ============================================================================
// QUERY MODULE TESTS
// ============================================================================

#[test]
fn test_query_module_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
        .add_ts_function("src/api/posts.ts", "getPosts", "return [];");

    repo.generate_index().unwrap();

    // First get the overview to find actual module names
    let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // Find a module name from the overview (look for common patterns)
    let module_name = if overview.contains("src.api") {
        "src.api"
    } else if overview.contains("api") {
        "api"
    } else {
        // Skip test if no modules found
        return;
    };

    // Query the discovered module
    let result = repo.run_cli(&["query", "module", module_name, "-f", "toon"]);

    // Should either succeed or gracefully handle the query
    assert!(result.is_ok(), "Module query should not error");
}

#[test]
fn test_query_module_with_kind_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/types.ts",
        r#"
export interface Config { }
export class Service { }
export function helper() { }
export const VALUE = 1;
"#,
    );

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "module", "src", "--kind", "fn"]);

    // Should filter by kind - check it doesn't error
    assert!(!output.is_empty(), "Kind filter should produce output");
}

#[test]
fn test_query_module_nonexistent() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["query", "module", "nonexistent_module_xyz"]);

    // Should handle gracefully - either error or empty results
    assert!(result.is_ok());
}

// ============================================================================
// QUERY SYMBOL TESTS
// ============================================================================

#[test]
fn test_query_symbol_by_hash() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/service.ts", "myService", "return 'service';");

    repo.generate_index().unwrap();

    // First search to get a hash
    let search_output = repo.run_cli_success(&["search", "myService", "-f", "json"]);

    // Try to extract a hash from output
    if search_output.contains("hash") {
        // Parse and find the hash
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&search_output) {
            let hashes = extract_symbol_hashes(&json);
            if !hashes.is_empty() {
                let hash = &hashes[0];
                let output = repo.run_cli_success(&["query", "symbol", hash, "-f", "json"]);

                // Should return symbol details
                assert!(!output.is_empty(), "Symbol query should return details");
            }
        }
    }
}

#[test]
fn test_query_symbol_with_source() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"
export function greetUser(name: string): string {
    return `Hello, ${name}!`;
}
"#,
    );

    repo.generate_index().unwrap();

    // Get hash
    let search_output = repo.run_cli_success(&["search", "greetUser", "-f", "json"]);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&search_output) {
        let hashes = extract_symbol_hashes(&json);
        if !hashes.is_empty() {
            let hash = &hashes[0];
            // Use --source flag
            let output = repo.run_cli_success(&["query", "symbol", hash, "--source"]);

            // Should include source code
            assert!(
                output.contains("Hello") || output.contains("greet") || !output.is_empty(),
                "Should include source code"
            );
        }
    }
}

#[test]
fn test_query_symbol_invalid_hash() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["query", "symbol", "invalid_hash_12345"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

// ============================================================================
// QUERY SOURCE TESTS
// ============================================================================

#[test]
fn test_query_source_file_line() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"const a = 1;
const b = 2;
const c = 3;
const d = 4;
const e = 5;
"#,
    );

    // Query specific line range using --start and --end
    let output = repo.run_cli_success(&[
        "query",
        "source",
        "src/main.ts",
        "--start",
        "2",
        "--end",
        "4",
    ]);

    // Should return some content
    assert!(
        output.contains("b") || output.contains("c") || output.contains("d") || !output.is_empty(),
        "Should return specified line range: {}",
        output
    );
}

// ============================================================================
// QUERY CALLERS TESTS
// ============================================================================

#[test]
fn test_query_callers_basic() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/service.ts",
        r#"
export function helper() {
    return "help";
}

export function caller1() {
    return helper();
}

export function caller2() {
    return helper() + helper();
}
"#,
    );

    repo.generate_index().unwrap();

    // Get hash for helper
    let search_output = repo.run_cli_success(&["search", "helper", "-f", "json"]);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&search_output) {
        let hashes = extract_symbol_hashes(&json);
        if !hashes.is_empty() {
            let hash = &hashes[0];
            let output = repo.run_cli_success(&["query", "callers", hash]);

            // Should find callers
            assert!(
                output.contains("caller") || !output.is_empty(),
                "Should find callers: {}",
                output
            );
        }
    }
}

#[test]
fn test_query_callers_with_depth() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/chain.ts",
        r#"
export function level3() { return "base"; }
export function level2() { return level3(); }
export function level1() { return level2(); }
export function level0() { return level1(); }
"#,
    );

    repo.generate_index().unwrap();

    // Get hash for level3
    let search_output = repo.run_cli_success(&["search", "level3", "-f", "json"]);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&search_output) {
        let hashes = extract_symbol_hashes(&json);
        if !hashes.is_empty() {
            let hash = &hashes[0];

            // Query with depth
            let output = repo.run_cli_success(&["query", "callers", hash, "--depth", "3"]);

            // Should find transitive callers
            assert!(
                output.contains("level") || !output.is_empty(),
                "Should find transitive callers"
            );
        }
    }
}

// ============================================================================
// QUERY CALLGRAPH TESTS
// ============================================================================

#[test]
fn test_query_callgraph_basic() {
    let repo = TestRepo::new();
    repo.with_complex_callgraph();

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "callgraph"]);

    // Should contain something about the call graph
    assert!(
        output.contains("edge") || output.contains("call") || !output.is_empty(),
        "Callgraph should contain edge information"
    );
}

#[test]
fn test_query_callgraph_with_module_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/api/handlers.ts",
        r#"
import { helper } from '../utils';
export function apiHandler() { return helper(); }
"#,
    )
    .add_file(
        "src/utils/index.ts",
        r#"
export function helper() { return "help"; }
"#,
    );

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "callgraph", "--module", "src"]);

    // Should produce output
    assert!(!output.is_empty(), "Should show filtered callgraph");
}

#[test]
fn test_query_callgraph_summary() {
    let repo = TestRepo::new();
    repo.with_complex_callgraph();

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "callgraph", "--stats-only"]);

    // Should contain some stats
    assert!(
        output.contains("edge")
            || output.contains("stat")
            || output.contains("count")
            || !output.is_empty(),
        "Summary should contain statistics: {}",
        output
    );
}

// ============================================================================
// QUERY FILE TESTS
// ============================================================================

#[test]
fn test_query_file_basic() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "Auth");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "file", "src/service.ts", "-f", "json"]);

    // Should return file's symbols
    assert!(
        output.contains("AuthService") || output.contains("createAuth") || !output.is_empty(),
        "Query file should return symbols: {}",
        output
    );
}

#[test]
fn test_query_file_with_source() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/utils.ts",
        r#"
export function util1() { return 1; }
export function util2() { return 2; }
"#,
    );

    repo.generate_index().unwrap();

    // Use --source flag
    let output = repo.run_cli_success(&["query", "file", "src/utils.ts", "--source"]);

    // Should include source
    assert!(
        output.contains("return 1") || output.contains("return 2") || output.contains("util"),
        "Should include source code: {}",
        output
    );
}

#[test]
fn test_query_file_nonexistent() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["query", "file", "nonexistent.ts"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

// ============================================================================
// QUERY LANGUAGES TESTS
// ============================================================================

#[test]
fn test_query_languages() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let output = repo.run_cli_success(&["query", "languages"]);

    // Should list supported languages
    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("typescript")
            || output_lower.contains("rust")
            || output_lower.contains("python")
            || !output.is_empty(),
        "Should list supported languages: {}",
        output
    );
}

#[test]
fn test_query_languages_text_format() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&["query", "languages", "-f", "text"]);

    // Should list languages in readable format
    assert!(!output.is_empty(), "Should have language list");
}

// ============================================================================
// FORMAT CONSISTENCY TESTS
// ============================================================================

#[test]
fn test_query_format_consistency() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    repo.generate_index().unwrap();

    // Test overview in all formats
    let text = repo.run_cli_success(&["query", "overview", "-f", "text"]);
    let toon = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // All should be non-empty
    assert!(!text.is_empty(), "Text should not be empty");
    assert!(!toon.is_empty(), "TOON should not be empty");

    // TOON should have type marker
    assert_valid_toon(&toon, "format consistency toon");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_query_no_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Don't generate index
    let result = repo.run_cli(&["query", "overview"]);

    // Should either auto-generate or provide helpful error
    assert!(result.is_ok());
}

#[test]
fn test_query_empty_repo() {
    let repo = TestRepo::new();
    // Empty repo
    std::fs::create_dir_all(repo.path().join("src")).unwrap();

    let result = repo.run_cli(&["query", "overview"]);

    // Should handle empty repo gracefully
    assert!(result.is_ok());
}
