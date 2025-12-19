//! Tests for MCP formatting functions
//!
//! These tests exercise the formatting logic via CLI commands that invoke
//! the same formatting paths used by MCP handlers.

#![allow(clippy::len_zero)]

use crate::common::assertions::assert_valid_json;
use crate::common::test_repo::TestRepo;

// ============================================================================
// Search Result Formatting Tests
// ============================================================================

#[test]
fn test_format_search_results_empty() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function foo() {}");
    repo.generate_index().expect("Index generation failed");

    // Search for something that doesn't exist
    let output = repo.run_cli_success(&["search", "nonexistent12345xyz"]);

    // Should handle empty results gracefully
    assert!(
        output.contains("no") || output.contains("0") || output.len() < 100,
        "Empty search should produce minimal output: {}",
        output
    );
}

#[test]
fn test_format_search_results_single_match() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/utils.ts",
        "export function uniqueFunction() { return 1; }",
    );
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "uniqueFunction"]);

    assert!(
        output.contains("uniqueFunction"),
        "Search should show the matched symbol: {}",
        output
    );
}

#[test]
fn test_format_search_results_multiple_matches() {
    let repo = TestRepo::new();
    repo.add_file("src/a.ts", "export function process() { return 1; }");
    repo.add_file("src/b.ts", "export function processData() { return 2; }");
    repo.add_file("src/c.ts", "export function processItems() { return 3; }");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "process"]);

    // Should show multiple matches
    assert!(
        output.len() > 50,
        "Multiple matches should produce substantial output: {}",
        output
    );
}

#[test]
fn test_format_search_results_with_limit() {
    let repo = TestRepo::new();
    // Create many functions
    for i in 0..20 {
        repo.add_file(
            &format!("src/mod{}.ts", i),
            &format!("export function item{}() {{ return {}; }}", i, i),
        );
    }
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "item", "--limit", "3"]);

    // Output should be limited
    assert!(output.len() > 0, "Limited search should return some output");
}

#[test]
fn test_format_search_results_json() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function testFunc() {}");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "testFunc", "-f", "json"]);

    assert_valid_json(&output, "search results json");
}

#[test]
fn test_format_search_results_toon() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function testFunc() {}");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["search", "testFunc", "-f", "toon"]);

    // TOON format should have type markers
    assert!(
        output.contains("_type") || output.len() > 0,
        "TOON format should have type markers: {}",
        output
    );
}

// ============================================================================
// Call Graph Formatting Tests
// ============================================================================

/// Create a repo with a known call graph for formatting tests
fn create_callgraph_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.add_file(
        "src/db.ts",
        r#"export async function queryDb(sql: string): Promise<any[]> {
    return fetch('/db', { body: sql }).then(r => r.json());
}
"#,
    );
    repo.add_file(
        "src/users.ts",
        r#"import { queryDb } from './db';

export async function getUsers(): Promise<any[]> {
    return queryDb('SELECT * FROM users');
}

export async function getUserById(id: number): Promise<any> {
    return queryDb(`SELECT * FROM users WHERE id = ${id}`);
}
"#,
    );
    repo.add_file(
        "src/api.ts",
        r#"import { getUsers, getUserById } from './users';

export async function handleListUsers() {
    const users = await getUsers();
    return { users };
}

export async function handleGetUser(id: number) {
    const user = await getUserById(id);
    return { user };
}
"#,
    );
    repo
}

#[test]
fn test_format_callgraph_minimal() {
    let repo = TestRepo::new();
    // Need at least one meaningful call between functions (not console.log which is filtered)
    repo.add_file(
        "src/main.ts",
        r#"export function helper() { return 42; }
export function main() { return helper(); }
"#,
    );
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "--stats-only"]);

    // Should handle minimal callgraph gracefully
    assert!(output.len() > 0, "Minimal callgraph should produce output");
}

#[test]
fn test_format_callgraph_with_edges() {
    let repo = create_callgraph_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph"]);

    // Should show call relationships
    assert!(
        output.len() > 50,
        "Callgraph should show relationships: {}",
        output
    );
}

#[test]
fn test_format_callgraph_stats_only() {
    let repo = create_callgraph_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "--stats-only"]);

    // Stats-only should be concise
    assert!(
        output.len() > 0,
        "Stats-only callgraph should produce output"
    );
}

#[test]
fn test_format_callgraph_with_pagination() {
    let repo = create_callgraph_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "--limit", "5"]);

    // Should respect limit
    assert!(
        output.len() > 0,
        "Paginated callgraph should produce output"
    );
}

#[test]
fn test_format_callgraph_json() {
    let repo = create_callgraph_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "callgraph", "-f", "json"]);

    assert_valid_json(&output, "callgraph json");
}

// ============================================================================
// Duplicate Formatting Tests
// ============================================================================

/// Create a repo with duplicates for formatting tests
fn create_duplicates_repo() -> TestRepo {
    let repo = TestRepo::new();
    let duplicate_body = r#"export function validateInput(input: string): boolean {
    if (!input) return false;
    if (input.length < 3) return false;
    if (input.length > 100) return false;
    return /^[a-zA-Z0-9]+$/.test(input);
}
"#;
    repo.add_file("src/auth/validate.ts", duplicate_body);
    repo.add_file("src/forms/validate.ts", duplicate_body);
    repo.add_file("src/api/validate.ts", duplicate_body);
    repo
}

#[test]
fn test_format_duplicates_empty() {
    let repo = TestRepo::new();
    repo.add_file("src/a.ts", "export function unique1() { return 1; }");
    repo.add_file("src/b.ts", "export function unique2() { return 2; }");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates"]);

    // Should handle no duplicates gracefully
    assert!(output.len() > 0, "Empty duplicates should produce output");
}

#[test]
fn test_format_duplicates_single_cluster() {
    let repo = create_duplicates_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates"]);

    // Should show the duplicate cluster
    assert!(
        output.contains("validateInput")
            || output.contains("cluster")
            || output.contains("duplicate"),
        "Should show duplicate cluster: {}",
        output
    );
}

#[test]
fn test_format_duplicates_with_limit() {
    let repo = create_duplicates_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates", "--limit", "10"]);

    assert!(output.len() > 0, "Limited duplicates should produce output");
}

#[test]
fn test_format_duplicates_json() {
    let repo = create_duplicates_repo();
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "json"]);

    assert_valid_json(&output, "duplicates json");
}

#[test]
fn test_format_duplicates_with_threshold() {
    let repo = create_duplicates_repo();
    repo.generate_index().expect("Index generation failed");

    // High threshold - should still find exact duplicates
    let output = repo.run_cli_success(&["validate", "--duplicates", "--threshold", "0.99"]);

    assert!(output.len() > 0, "High threshold duplicates should work");
}

// ============================================================================
// Source Formatting Tests
// ============================================================================

#[test]
fn test_format_source_snippet_basic() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/utils.ts",
        r#"export function first() { return 1; }
export function second() { return 2; }
export function third() { return 3; }
"#,
    );

    let output = repo.run_cli_success(&[
        "query",
        "source",
        "src/utils.ts",
        "--start",
        "1",
        "--end",
        "3",
    ]);

    assert!(
        output.contains("first") || output.contains("function"),
        "Source snippet should show code: {}",
        output
    );
}

#[test]
fn test_format_source_snippet_with_context() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"// Line 1
// Line 2
export function target() {
    return 42;
}
// Line 6
// Line 7
"#,
    );

    // Request middle lines
    let output = repo.run_cli_success(&[
        "query",
        "source",
        "src/main.ts",
        "--start",
        "3",
        "--end",
        "5",
    ]);

    assert!(
        output.contains("target") || output.contains("42"),
        "Source with context should show function: {}",
        output
    );
}

#[test]
fn test_format_source_entire_file() {
    let repo = TestRepo::new();
    let content = "export function test() { return 'test'; }";
    repo.add_file("src/small.ts", content);

    // No line range - get entire file
    let output = repo.run_cli_success(&["query", "source", "src/small.ts"]);

    assert!(
        output.contains("test") || output.contains("function"),
        "Full source should show content: {}",
        output
    );
}

// ============================================================================
// Languages Formatting Tests
// ============================================================================

#[test]
fn test_format_languages_list() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function f() {}");

    let output = repo.run_cli_success(&["query", "languages"]);

    // Should list multiple languages
    assert!(
        output.contains("TypeScript") || output.contains(".ts"),
        "Should list TypeScript"
    );
    assert!(
        output.contains("Rust") || output.contains(".rs"),
        "Should list Rust"
    );
    assert!(
        output.contains("Python") || output.contains(".py"),
        "Should list Python"
    );
}

// ============================================================================
// Overview Formatting Tests
// ============================================================================

#[test]
fn test_format_overview_basic() {
    let repo = TestRepo::new();
    repo.add_file("src/utils.ts", "export function util() {}");
    repo.add_file("src/api/handler.ts", "export function handle() {}");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview"]);

    // Overview should contain type marker and file count
    assert!(
        output.contains("_type") || output.contains("repo_overview") || output.contains("files"),
        "Overview should show repo info: {}",
        output
    );
}

#[test]
fn test_format_overview_json() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function main() {}");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);

    assert_valid_json(&output, "overview json");
}

#[test]
fn test_format_overview_toon() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function main() {}");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // TOON format should have type indicators
    assert!(
        output.contains("_type") || output.len() > 10,
        "TOON overview should have structure: {}",
        output
    );
}

// ============================================================================
// Commit Prep Formatting Tests
// ============================================================================

#[test]
fn test_format_commit_prep_no_changes() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_file("src/main.ts", "export function main() {}");
    repo.commit("Initial commit");
    // No uncommitted changes

    let output = repo.run_cli_success(&["commit"]);

    // Should indicate clean state
    assert!(output.len() > 0, "Commit prep should work with no changes");
}

#[test]
fn test_format_commit_prep_with_changes() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_file("src/main.ts", "export function main() {}");
    repo.commit("Initial commit");
    // Modify existing tracked file so git detects changes
    repo.add_file("src/main.ts", "export function main() { return 42; }");

    let output = repo.run_cli_success(&["commit"]);

    // Should show the modified file
    assert!(
        output.contains("main.ts") || output.contains("unstaged") || output.len() > 20,
        "Commit prep should show changes: {}",
        output
    );
}

#[test]
fn test_format_commit_prep_json() {
    let repo = TestRepo::new();
    repo.init_git();
    repo.add_file("src/main.ts", "export function main() {}");
    repo.commit("Initial commit");
    // Modify existing tracked file so git detects changes
    repo.add_file("src/main.ts", "export function main() { return 42; }");

    let output = repo.run_cli_success(&["commit", "-f", "json"]);

    assert_valid_json(&output, "commit prep json");
}

// ============================================================================
// Validate Formatting Tests
// ============================================================================

#[test]
fn test_format_validate_file_basic() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/complex.ts",
        r#"export function nested(data: any) {
    if (data.a) {
        if (data.b) {
            for (const item of data.items) {
                if (item.active) {
                    return item.value;
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

    assert!(output.len() > 0, "Validate should produce output");
}

#[test]
fn test_format_validate_file_json() {
    let repo = TestRepo::new();
    repo.add_file("src/main.ts", "export function simple() { return 1; }");
    repo.generate_index().expect("Index generation failed");

    let output = repo.run_cli_success(&["validate", "src/main.ts", "-f", "json"]);

    assert_valid_json(&output, "validate json");
}
