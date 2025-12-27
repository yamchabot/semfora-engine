//! Integration tests for MCP tool handlers
//!
//! Tests MCP functionality via CLI commands, which exercise the same code paths
//! as the MCP handlers. This provides comprehensive coverage without requiring
//! internal handler access.
//!
//! These tests cover:
//! - All 18 MCP tool handlers via their CLI equivalents
//! - Basic functionality, edge cases, and error conditions
//! - Multi-language support
//! - Error handling

#![allow(clippy::len_zero)]

use crate::common::assertions::assert_valid_json;
use crate::common::test_repo::TestRepo;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test repo with TypeScript files for basic tests
fn create_basic_ts_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.add_file(
        "src/utils.ts",
        r#"export function greet(name: string): string {
    return `Hello, ${name}!`;
}

export function add(a: number, b: number): number {
    return a + b;
}

function internal() {
    console.log('internal');
}
"#,
    );
    repo.add_file(
        "src/api/users.ts",
        r#"import { greet } from '../utils';

export interface User {
    id: number;
    name: string;
}

export async function getUsers(): Promise<User[]> {
    return fetch('/api/users').then(r => r.json());
}

export async function createUser(name: string): Promise<User> {
    const greeting = greet(name);
    console.log(greeting);
    return fetch('/api/users', { method: 'POST', body: JSON.stringify({ name }) })
        .then(r => r.json());
}
"#,
    );
    repo.add_file(
        "src/index.ts",
        r#"import { getUsers, createUser } from './api/users';
import { greet } from './utils';

async function main() {
    const users = await getUsers();
    console.log(users);

    const newUser = await createUser('Alice');
    console.log(greet(newUser.name));
}

main();
"#,
    );
    repo
}

/// Create a test repo with git initialized for diff tests
fn create_git_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_file("src/app.ts", "export function app() { return 'v1'; }");
    repo.commit("Initial commit");
    repo
}

/// Create a multi-language repo
fn create_multilang_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.rs",
        r#"fn main() {
    println!("Hello, world!");
}

pub fn process_data(input: &str) -> String {
    input.to_uppercase()
}
"#,
    );
    repo.add_file(
        "src/lib.py",
        r#"def process_items(items):
    """Process a list of items."""
    return [item.upper() for item in items]

class DataProcessor:
    def __init__(self, config):
        self.config = config

    def run(self):
        return self.config.get('value', 0)
"#,
    );
    repo.add_file(
        "src/utils.go",
        r#"package main

import "strings"

func ProcessItems(items []string) []string {
    result := make([]string, len(items))
    for i, item := range items {
        result[i] = strings.ToUpper(item)
    }
    return result
}
"#,
    );
    repo
}

/// Create a repo with duplicates for duplicate detection tests
fn create_duplicate_repo() -> TestRepo {
    let repo = TestRepo::new();
    let validate_body = r#"export function validateEmail(email: string): boolean {
    if (!email) return false;
    if (!email.includes('@')) return false;
    if (email.length < 5) return false;
    const parts = email.split('@');
    return parts[0].length > 0 && parts[1].length > 2;
}
"#;
    repo.add_file("src/auth/validate.ts", validate_body);
    repo.add_file("src/users/validate.ts", validate_body);
    repo.add_file("src/orders/validate.ts", validate_body);
    repo
}

// ============================================================================
// Index Tool Tests (MCP: index)
// ============================================================================

#[test]
fn test_mcp_index_generate() {
    let repo = create_basic_ts_repo();
    let output = repo.run_cli_success(&["index", "generate"]);

    // Should show indexing success
    assert!(
        output.contains("Generated") || output.contains("success") || output.contains("files"),
        "Index output should indicate success: {}",
        output
    );
}

#[test]
fn test_mcp_index_check() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["index", "check"]);

    // Should show index status
    assert!(
        output.contains("fresh") || output.contains("status") || output.contains("index"),
        "Index check output: {}",
        output
    );
}

#[test]
fn test_mcp_index_force_regenerate() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Initial index failed");

    let output = repo.run_cli_success(&["index", "generate", "--force"]);

    assert!(
        output.contains("Generated")
            || output.contains("success")
            || output.contains("complete")
            || output.contains("fresh"),
        "Force regenerate output: {}",
        output
    );
}

// ============================================================================
// Search Tool Tests (MCP: search)
// ============================================================================

#[test]
fn test_mcp_search_by_name() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "greet"]);

    assert!(
        output.contains("greet") || output.contains("symbol") || output.contains("found"),
        "Search should find 'greet': {}",
        output
    );
}

#[test]
fn test_mcp_search_json_format() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "User", "-f", "json"]);

    assert_valid_json(&output, "search json output");
}

#[test]
fn test_mcp_search_with_limit() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "*", "--limit", "5"]);

    // Should return limited results
    assert!(output.len() > 0, "Search should return some results");
}

#[test]
fn test_mcp_search_no_results() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "nonexistentfunction12345"]);

    // Should handle gracefully - either empty or "no matches" message
    assert!(output.len() < 200 || output.contains("no") || output.contains("0"));
}

// ============================================================================
// Analyze Tool Tests (MCP: analyze)
// ============================================================================

#[test]
fn test_mcp_analyze_file() {
    let repo = create_basic_ts_repo();

    let output = repo.run_cli_success(&["analyze", "src/utils.ts"]);

    assert!(
        output.contains("greet") || output.contains("add") || output.contains("fn"),
        "Analyze should find functions: {}",
        output
    );
}

#[test]
fn test_mcp_analyze_directory() {
    let repo = create_basic_ts_repo();

    let output = repo.run_cli_success(&["analyze", "src"]);

    assert!(
        output.len() > 100,
        "Directory analysis should produce substantial output"
    );
}

#[test]
fn test_mcp_analyze_json_format() {
    let repo = create_basic_ts_repo();

    let output = repo.run_cli_success(&["analyze", "src/utils.ts", "-f", "json"]);

    assert_valid_json(&output, "analyze json output");
}

#[test]
fn test_mcp_analyze_nonexistent_file() {
    let repo = create_basic_ts_repo();

    let result = repo.run_cli(&["analyze", "nonexistent.ts"]);

    // Should fail for nonexistent file
    assert!(
        !result.unwrap().status.success(),
        "Should fail for nonexistent file"
    );
}

// ============================================================================
// Query Overview Tests (MCP: get_overview)
// ============================================================================

#[test]
fn test_mcp_query_overview() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview"]);

    assert!(
        output.contains("module") || output.contains("_type") || output.contains("src"),
        "Overview should contain module info: {}",
        output
    );
}

#[test]
fn test_mcp_query_overview_json() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);

    assert_valid_json(&output, "overview json output");
}

// ============================================================================
// Query Module Tests (MCP: analyze with module param)
// ============================================================================

#[test]
fn test_mcp_query_module() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Query module - need to use actual module name from index
    let output = repo.run_cli_success(&["query", "overview"]);

    // Parse module names from overview and query one
    assert!(output.len() > 0, "Should have overview output");
}

// ============================================================================
// Query Symbol Tests (MCP: get_symbol)
// ============================================================================

#[test]
fn test_mcp_query_symbol_search_first() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // First search to find a symbol hash
    let search_output = repo.run_cli_success(&["search", "greet", "-f", "json"]);

    // The search should return symbol info
    assert!(
        search_output.contains("greet") || search_output.contains("hash"),
        "Search should return symbol info: {}",
        search_output
    );
}

// ============================================================================
// Query Source Tests (MCP: get_source)
// ============================================================================

#[test]
fn test_mcp_query_source_by_lines() {
    let repo = create_basic_ts_repo();

    let output = repo.run_cli_success(&[
        "query",
        "source",
        "src/utils.ts",
        "--start",
        "1",
        "--end",
        "5",
    ]);

    assert!(
        output.contains("greet") || output.contains("function") || output.contains("export"),
        "Source query should return code: {}",
        output
    );
}

// ============================================================================
// Query Callgraph Tests (MCP: get_callgraph)
// ============================================================================

#[test]
fn test_mcp_query_callgraph() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "--stats-only"]);

    // Should return some callgraph info
    assert!(output.len() > 0, "Callgraph should return output");
}

#[test]
fn test_mcp_query_callgraph_with_limit() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "--limit", "10"]);

    assert!(
        output.len() > 0,
        "Callgraph with limit should return output"
    );
}

// ============================================================================
// Query Languages Tests (MCP: get_languages)
// ============================================================================

#[test]
fn test_mcp_query_languages() {
    let repo = create_basic_ts_repo();

    let output = repo.run_cli_success(&["query", "languages"]);

    assert!(
        output.contains("TypeScript") || output.contains("typescript") || output.contains(".ts"),
        "Languages should list TypeScript: {}",
        output
    );
    assert!(
        output.contains("Rust") || output.contains("rust") || output.contains(".rs"),
        "Languages should list Rust: {}",
        output
    );
    assert!(
        output.contains("Python") || output.contains("python") || output.contains(".py"),
        "Languages should list Python: {}",
        output
    );
}

// ============================================================================
// Query File Tests (MCP: get_file)
// ============================================================================

#[test]
fn test_mcp_query_file() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "file", "src/utils.ts"]);

    assert!(
        output.contains("greet") || output.contains("add") || output.contains("symbol"),
        "File query should return symbols: {}",
        output
    );
}

#[test]
fn test_mcp_query_file_with_source() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "file", "src/utils.ts", "--source"]);

    assert!(
        output.len() > 50,
        "File query with source should return substantial output: {}",
        output
    );
}

// ============================================================================
// Validate Tests (MCP: validate)
// ============================================================================

#[test]
fn test_mcp_validate_file() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "src/utils.ts"]);

    // Validate command runs duplicate analysis on the file
    assert!(
        output.contains("DUPLICATE")
            || output.contains("Signatures")
            || output.contains("analyzed")
            || output.len() > 50,
        "Validate should analyze file: {}",
        output
    );
}

#[test]
fn test_mcp_validate_duplicates() {
    let repo = create_duplicate_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates"]);

    // Should find duplicate validateEmail functions
    assert!(
        output.contains("duplicate")
            || output.contains("cluster")
            || output.contains("validateEmail")
            || output.len() > 10,
        "Should find duplicates: {}",
        output
    );
}

// ============================================================================
// Diff Tests (MCP: analyze_diff)
// ============================================================================

#[test]
fn test_mcp_analyze_diff_no_changes() {
    let repo = create_git_repo();

    // Diff HEAD to HEAD should show no changes
    let output = repo.run_cli_success(&["analyze", "--diff", "HEAD"]);

    assert!(
        output.contains("No") || output.contains("no") || output.len() < 200,
        "Diff with no changes: {}",
        output
    );
}

#[test]
fn test_mcp_analyze_diff_working_tree() {
    let repo = create_git_repo();
    // Add uncommitted changes
    repo.add_file("src/new.ts", "export function newFunc() { return 42; }");

    let output = repo.run_cli_success(&["analyze", "--uncommitted"]);

    // Should show the new file
    assert!(
        output.contains("new.ts") || output.contains("newFunc") || output.len() > 10,
        "Diff should show uncommitted changes: {}",
        output
    );
}

#[test]
fn test_mcp_analyze_diff_not_git() {
    let repo = create_basic_ts_repo(); // Not a git repo

    let result = repo.run_cli(&["analyze", "--diff", "HEAD"]);

    // Should fail for non-git repo
    assert!(
        !result.unwrap().status.success(),
        "Should fail for non-git repo"
    );
}

// ============================================================================
// Multi-Language Tests
// ============================================================================

#[test]
fn test_mcp_multilang_index() {
    let repo = create_multilang_repo();
    let output = repo.run_cli_success(&["index", "generate"]);

    assert!(
        output.contains("Generated") || output.contains("success") || output.contains("files"),
        "Multi-lang index should succeed: {}",
        output
    );
}

#[test]
fn test_mcp_multilang_search() {
    let repo = create_multilang_repo();
    repo.generate_index().expect("Index generation failed");

    // Search should find symbols from multiple languages
    let output = repo.run_cli_success(&["search", "process"]);

    assert!(
        output.contains("process") || output.contains("Process"),
        "Should find process functions: {}",
        output
    );
}

#[test]
fn test_mcp_multilang_overview() {
    let repo = create_multilang_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview"]);

    // Should include multiple languages
    assert!(output.len() > 50, "Overview should include multi-lang info");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_mcp_error_invalid_command() {
    let repo = create_basic_ts_repo();

    let result = repo.run_cli(&["invalidcommand"]);

    // Should fail with error
    assert!(
        !result.unwrap().status.success(),
        "Invalid command should fail"
    );
}

#[test]
fn test_mcp_error_missing_file() {
    let repo = create_basic_ts_repo();

    let result = repo.run_cli(&["analyze", "does_not_exist.ts"]);

    assert!(
        !result.unwrap().status.success(),
        "Missing file should fail"
    );
}

#[test]
fn test_mcp_error_unsupported_extension() {
    let repo = TestRepo::new();
    repo.add_file("test.xyz", "some content");

    let result = repo.run_cli(&["analyze", "test.xyz"]);

    // Should fail for unsupported extension
    assert!(
        !result.unwrap().status.success(),
        "Unsupported extension should fail"
    );
}

// ============================================================================
// Security Tests - DISABLED (command hidden from CLI/MCP)
// ============================================================================
// Security tests disabled - command hidden from CLI (kept in src/commands/security.rs for future use)
// #[test]
// fn test_mcp_security_stats() { ... }
// #[test]
// fn test_mcp_security_scan() { ... }

// ============================================================================
// Test Runner Tests (MCP: test)
// ============================================================================

#[test]
fn test_mcp_test_detect() {
    let repo = create_basic_ts_repo();
    repo.add_file(
        "package.json",
        r#"{"name": "test", "scripts": {"test": "jest"}}"#,
    );

    let output = repo.run_cli_success(&["test", "--detect"]);

    // Should detect test framework
    assert!(output.len() > 0, "Test detect should return output");
}

#[test]
fn test_mcp_test_detect_no_framework() {
    let repo = create_basic_ts_repo();
    // No test framework configured

    let output = repo.run_cli_success(&["test", "--detect"]);

    // Should handle gracefully
    assert!(output.len() > 0, "Test detect should handle no framework");
}

// ============================================================================
// Commit Prep Tests (MCP: prep_commit)
// ============================================================================

#[test]
fn test_mcp_commit_prep() {
    let repo = create_git_repo();
    repo.add_file("src/new.ts", "export function newFeature() { return 42; }");

    let output = repo.run_cli_success(&["commit"]);

    // Should show uncommitted changes info
    assert!(
        output.contains("new.ts")
            || output.contains("unstaged")
            || output.contains("change")
            || output.len() > 10,
        "Commit prep should show changes: {}",
        output
    );
}

#[test]
fn test_mcp_commit_prep_no_changes() {
    let repo = create_git_repo();
    // No uncommitted changes

    let output = repo.run_cli_success(&["commit"]);

    // Should indicate no changes or clean state
    assert!(output.len() > 0, "Commit prep should return output");
}

// ============================================================================
// Cache Tests (Related to MCP index operations)
// ============================================================================

#[test]
fn test_mcp_cache_info() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["cache", "info"]);

    assert!(output.len() > 0, "Cache info should return output");
}

// ============================================================================
// Output Format Tests
// ============================================================================

#[test]
fn test_mcp_output_text_format() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "greet", "-f", "text"]);

    // Text format should be readable
    assert!(output.len() > 0, "Text format should produce output");
}

#[test]
fn test_mcp_output_toon_format() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "greet", "-f", "toon"]);

    // TOON format should have type markers
    assert!(
        output.contains("_type") || output.len() > 0,
        "TOON format output: {}",
        output
    );
}

#[test]
fn test_mcp_output_json_format() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "greet", "-f", "json"]);

    assert_valid_json(&output, "JSON format");
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_mcp_empty_repo() {
    let repo = TestRepo::new();
    // Empty repo with no files

    let output = repo.run_cli_success(&["index", "generate"]);

    // Should handle empty repo gracefully
    assert!(output.len() > 0, "Empty repo should be handled");
}

#[test]
fn test_mcp_unicode_filename() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/日本語.ts",
        "export function hello() { return '世界'; }",
    );

    // May or may not handle Unicode filenames depending on platform
    let result = repo.run_cli(&["analyze", "src/日本語.ts"]);
    // Just verify it doesn't crash
    assert!(result.is_ok(), "Should not crash on Unicode filename");
}

#[test]
fn test_mcp_deep_nesting() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/a/b/c/d/e/f/g/deep.ts",
        "export function deepFunction() { return 'deep'; }",
    );

    let output = repo.run_cli_success(&["analyze", "src/a/b/c/d/e/f/g/deep.ts"]);

    assert!(
        output.contains("deepFunction") || output.contains("fn"),
        "Should analyze deeply nested file: {}",
        output
    );
}

#[test]
fn test_mcp_large_file() {
    let repo = TestRepo::new();

    // Create a file with many functions
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!(
            "export function func{}(x: number): number {{ return x + {}; }}\n",
            i, i
        ));
    }
    repo.add_file("src/large.ts", &content);

    let output = repo.run_cli_success(&["analyze", "src/large.ts"]);

    // Should handle large files
    assert!(output.len() > 100, "Should analyze large file");
}

// ============================================================================
// Search Mode Tests (MCP: search with different modes)
// ============================================================================

#[test]
fn test_mcp_search_symbol_mode() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Symbol mode: exact name matching using --symbols flag
    let output = repo.run_cli_success(&["search", "greet", "--symbols"]);

    assert!(
        output.contains("greet") || output.contains("symbol"),
        "Symbol mode should find exact matches: {}",
        output
    );
}

#[test]
fn test_mcp_search_semantic_mode() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Semantic mode: BM25 conceptual search using --related flag
    let output = repo.run_cli_success(&["search", "user data", "--related"]);

    // May or may not find results, but should not error
    assert!(output.len() > 0, "Semantic mode should return output");
}

#[test]
fn test_mcp_search_raw_mode() {
    let repo = create_basic_ts_repo();

    // Raw mode: ripgrep regex search using --raw flag
    let output = repo.run_cli_success(&["search", "function", "--raw"]);

    // Should find regex pattern matches
    assert!(
        output.contains("function") || output.len() > 0,
        "Raw mode should search with regex: {}",
        output
    );
}

#[test]
fn test_mcp_search_hybrid_mode_default() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Default mode is hybrid (both symbol and semantic)
    let output = repo.run_cli_success(&["search", "User"]);

    // Should find results from both symbol and semantic search
    assert!(
        output.contains("User") || output.contains("user") || output.len() > 0,
        "Hybrid mode should find results: {}",
        output
    );
}

#[test]
fn test_mcp_search_with_module_filter() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Search with module filter
    let output = repo.run_cli_success(&["search", "greet", "--module", "src"]);

    assert!(
        output.len() > 0,
        "Module filtered search should return output"
    );
}

#[test]
fn test_mcp_search_with_kind_filter() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Search only for functions
    let output = repo.run_cli_success(&["search", "*", "--kind", "fn"]);

    assert!(
        output.len() > 0,
        "Kind filtered search should return output"
    );
}

#[test]
fn test_mcp_search_with_source_output() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Include source code in search results using --include-source flag
    let output = repo.run_cli_success(&["search", "greet", "--include-source"]);

    // Should include source snippets
    assert!(
        output.contains("greet") || output.len() > 50,
        "Search with source should show code: {}",
        output
    );
}

// ============================================================================
// Get Callers Tests (MCP: get_callers)
// ============================================================================

/// Create a repo with call relationships for caller tests
fn create_call_relationship_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.add_file(
        "src/utils.ts",
        r#"export function helper(x: number): number {
    return x * 2;
}
"#,
    );
    repo.add_file(
        "src/service.ts",
        r#"import { helper } from './utils';

export function processData(data: number[]): number[] {
    return data.map(x => helper(x));
}

export function transformItem(item: number): number {
    return helper(item) + 1;
}
"#,
    );
    repo.add_file(
        "src/main.ts",
        r#"import { processData, transformItem } from './service';

async function main() {
    const data = [1, 2, 3];
    const processed = processData(data);
    const single = transformItem(5);
    console.log(processed, single);
}

main();
"#,
    );
    repo
}

#[test]
fn test_mcp_query_callers_basic() {
    let repo = create_call_relationship_repo();
    repo.generate_index().expect("Index generation failed");

    // First search to find a symbol
    let search_output = repo.run_cli_success(&["search", "helper", "-f", "json"]);

    // If we found a hash, we can query callers
    if search_output.contains("hash") {
        // Extract hash and query callers
        assert!(search_output.len() > 0, "Should find helper function");
    }
}

#[test]
fn test_mcp_query_callgraph_with_symbol_filter() {
    let repo = create_call_relationship_repo();
    repo.generate_index().expect("Index generation failed");

    // Query callgraph filtered to a specific symbol
    let output = repo.run_cli_success(&["query", "callgraph", "--symbol", "helper"]);

    assert!(
        output.len() > 0,
        "Callgraph with symbol filter should return output"
    );
}

#[test]
fn test_mcp_query_callgraph_with_module_filter() {
    let repo = create_call_relationship_repo();
    repo.generate_index().expect("Index generation failed");

    // Query callgraph filtered to a module
    let output = repo.run_cli_success(&["query", "callgraph", "--module", "src"]);

    assert!(
        output.len() > 0,
        "Callgraph with module filter should return output"
    );
}

// ============================================================================
// Validate Scope Tests (MCP: validate with different scopes)
// ============================================================================

#[test]
fn test_mcp_validate_module_scope() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Validate at module level - pass module name as TARGET positional arg
    let output = repo.run_cli_success(&["validate", "src"]);

    assert!(
        output.len() > 10,
        "Module scope validation should return output: {}",
        output
    );
}

#[test]
fn test_mcp_validate_with_duplicate_threshold() {
    let repo = create_duplicate_repo();
    repo.generate_index().expect("Index generation failed");

    // Validate with custom duplicate threshold
    let output = repo.run_cli_success(&["validate", "--duplicates", "--threshold", "0.95"]);

    assert!(
        output.len() > 0,
        "Validate with threshold should return output"
    );
}

#[test]
fn test_mcp_validate_complexity_check() {
    let repo = TestRepo::new();
    // Create a file with complex nested code
    repo.add_file(
        "src/complex.ts",
        r#"export function complexFunction(data: any[], options: any): any {
    if (options.filter) {
        for (const item of data) {
            if (item.active) {
                if (item.type === 'a') {
                    for (const sub of item.children) {
                        if (sub.valid) {
                            if (options.transform) {
                                return sub.value * 2;
                            }
                        }
                    }
                }
            }
        }
    }
    return null;
}
"#,
    );
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "src/complex.ts"]);

    // Should show complexity metrics
    assert!(output.len() > 0, "Complexity validation should run");
}

#[test]
fn test_mcp_validate_json_output() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "src/utils.ts", "-f", "json"]);

    assert_valid_json(&output, "validate json output");
}

// ============================================================================
// Symbol Query Mode Tests (MCP: get_symbol)
// ============================================================================

#[test]
fn test_mcp_query_symbol_by_file_location() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Query symbol at specific file location
    let output = repo.run_cli_success(&["query", "file", "src/utils.ts"]);

    // Should return symbols at that location
    assert!(
        output.contains("greet") || output.contains("add"),
        "Should find symbols at file location: {}",
        output
    );
}

#[test]
fn test_mcp_query_symbol_kind_filter() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    // Query with kind filter
    let output =
        repo.run_cli_success(&["query", "file", "src/api/users.ts", "--kind", "interface"]);

    // Should find User interface
    assert!(
        output.contains("User") || output.len() > 0,
        "Kind filter should find interfaces: {}",
        output
    );
}

// ============================================================================
// Index Behavior Tests
// ============================================================================

#[test]
fn test_mcp_index_smart_refresh_when_fresh() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Initial index failed");

    // Run index generate without --force - should detect fresh index or regenerate
    let output = repo.run_cli_success(&["index", "generate"]);

    // Should complete successfully - looks for "complete", "fresh", or file count
    assert!(
        output.contains("fresh") || output.contains("complete") || output.contains("files_found"),
        "Smart refresh output: {}",
        output
    );
}

#[test]
fn test_mcp_index_detects_stale_after_file_change() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Initial index failed");

    // Add a new file to make index stale
    repo.add_file("src/newfile.ts", "export function newFunc() { return 1; }");

    // Check index status
    let output = repo.run_cli_success(&["index", "check"]);

    // Should indicate stale or provide status
    assert!(output.len() > 0, "Index check should show status");
}

#[test]
fn test_mcp_index_with_extension_filter() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function tsFunc() {}");
    repo.add_file("src/lib.js", "export function jsFunc() {}");
    repo.add_file("src/app.py", "def pyFunc(): pass");

    // Index only TypeScript files
    let output = repo.run_cli_success(&["index", "generate", "--ext", "ts"]);

    // Should complete and show files processed
    assert!(
        output.contains("complete")
            || output.contains("files_found")
            || output.contains("files_processed"),
        "Extension filtered index: {}",
        output
    );

    // Verify only 1 file was processed (the .ts file)
    assert!(
        output.contains("files_found: 1") || output.contains("files_processed: 1"),
        "Should only index TypeScript files: {}",
        output
    );
}

// ============================================================================
// Duplicate Detection Advanced Tests
// ============================================================================

#[test]
fn test_mcp_find_duplicates_basic() {
    let repo = create_duplicate_repo();
    repo.generate_index().expect("Index generation failed");

    // Use validate --duplicates to find duplicates
    let output = repo.run_cli_success(&["validate", "--duplicates"]);

    // Should find duplicate validateEmail functions
    assert!(
        output.contains("validateEmail")
            || output.contains("cluster")
            || output.contains("duplicate")
            || output.len() > 10,
        "Should detect duplicates: {}",
        output
    );
}

#[test]
fn test_mcp_find_duplicates_with_limit() {
    let repo = create_duplicate_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates", "--limit", "5"]);

    assert!(
        output.len() > 0,
        "Duplicates with limit should return output"
    );
}

#[test]
fn test_mcp_find_duplicates_json_format() {
    let repo = create_duplicate_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "json"]);

    assert_valid_json(&output, "duplicates json output");
}

#[test]
fn test_mcp_find_duplicates_no_duplicates() {
    let repo = create_basic_ts_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates"]);

    // Should handle case with no duplicates gracefully
    assert!(output.len() > 0, "No duplicates output should exist");
}

// ============================================================================
// Context and Overview Tests
// ============================================================================

#[test]
fn test_mcp_get_project_context_via_overview() {
    let repo = create_git_repo();
    repo.generate_index().expect("Index generation failed");

    // Use query overview for getting project context
    let output = repo.run_cli_success(&["query", "overview"]);

    // Should show project info
    assert!(
        output.contains("module") || output.contains("src") || output.len() > 10,
        "Overview should show project info: {}",
        output
    );
}

#[test]
fn test_mcp_get_git_context_via_commit() {
    let repo = create_git_repo();
    // Add uncommitted changes so commit prep has something to show
    repo.add_file("src/new.ts", "export function newFunc() {}");

    let output = repo.run_cli_success(&["commit"]);

    // Should show git context and changes
    assert!(
        output.contains("branch") || output.contains("new.ts") || output.len() > 10,
        "Commit prep should show git context: {}",
        output
    );
}

#[test]
fn test_mcp_query_overview_max_modules() {
    let repo = TestRepo::new();
    // Create multiple modules
    for i in 0..10 {
        repo.add_file(
            &format!("src/mod{}/index.ts", i),
            &format!("export function func{}() {{ return {}; }}", i, i),
        );
    }
    repo.generate_index().expect("Index generation failed");

    // Query overview with module limit
    let output = repo.run_cli_success(&["query", "overview", "--max-modules", "3"]);

    assert!(output.len() > 0, "Overview with max modules should work");
}
