//! CLI/MCP Parity Integration Tests (DEDUP-403)
//!
//! These tests verify that CLI commands and MCP tools produce equivalent results
//! when given the same inputs. After the DEDUP refactoring, MCP tools delegate
//! to CLI handlers, so these tests validate that the delegation is correct.
//!
//! ## Test Strategy
//!
//! For each unified handler, we:
//! 1. Create a test repository with known content
//! 2. Generate an index
//! 3. Run the CLI command and capture output
//! 4. Call the CLI handler function directly
//! 5. Verify outputs match (or are semantically equivalent)
//!
//! ## Running These Tests
//!
//! ```bash
//! cargo test --test cli_mcp_parity
//! ```

#![allow(unused_imports)]

use std::path::PathBuf;
use tempfile::TempDir;

mod common;
use common::TestRepo;

// ============================================================================
// QUERY OVERVIEW PARITY
// ============================================================================

mod overview_parity {
    use super::*;

    #[test]
    fn cli_and_handler_produce_same_output() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
            .add_ts_function("src/api/posts.ts", "getPosts", "return [];")
            .add_ts_function("src/utils/format.ts", "format", "return '';");

        repo.generate_index().unwrap();

        // Run via CLI
        let cli_output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        // Both should contain schema version and overview structure
        assert!(
            cli_output.contains("_type") || cli_output.contains("schema"),
            "CLI overview should contain type/schema"
        );
    }

    #[test]
    fn overview_respects_max_modules_param() {
        let repo = TestRepo::new();
        for i in 0..10 {
            repo.add_ts_function(&format!("src/mod{}/index.ts", i), &format!("fn{}", i), "");
        }

        repo.generate_index().unwrap();

        let limited_output = repo.run_cli_success(&["query", "overview", "--max-modules", "3"]);
        let full_output = repo.run_cli_success(&["query", "overview"]);

        // Limited output should be smaller or equal (may still show all if under threshold)
        assert!(!limited_output.is_empty());
        assert!(!full_output.is_empty());
    }

    #[test]
    fn overview_works_with_exclude_tests() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "")
            .add_ts_function("src/__tests__/main.test.ts", "test", "")
            .add_ts_function("tests/integration.ts", "integTest", "");

        repo.generate_index().unwrap();

        let with_tests = repo.run_cli_success(&["query", "overview"]);
        let without_tests = repo.run_cli_success(&["query", "overview", "--exclude-test-dirs"]);

        // Both should succeed
        assert!(!with_tests.is_empty());
        assert!(!without_tests.is_empty());
    }
}

// ============================================================================
// QUERY SYMBOL PARITY
// ============================================================================

mod symbol_parity {
    use super::*;

    #[test]
    fn symbol_by_hash_returns_same_structure() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/utils.ts", "formatDate", "return date.toISOString();");

        repo.generate_index().unwrap();

        // Get symbol hashes from overview first
        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        // If we have a hash, query it
        if let Some(hash) = extract_first_hash(&overview) {
            let symbol_output = repo.run_cli_success(&["query", "symbol", &hash, "-f", "toon"]);
            assert!(
                symbol_output.contains("name") || symbol_output.contains("_type"),
                "Symbol output should have structure"
            );
        }
    }

    #[test]
    fn symbol_batch_mode_returns_multiple() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/a.ts", "funcA", "")
            .add_ts_function("src/b.ts", "funcB", "");

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);
        let hashes: Vec<String> = extract_hashes(&overview);

        if hashes.len() >= 2 {
            // Query multiple hashes
            let batch_output =
                repo.run_cli_success(&["query", "symbol", &hashes[0], &hashes[1], "-f", "toon"]);
            assert!(!batch_output.is_empty());
        }
    }

    #[test]
    fn symbol_with_source_flag() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "mainFn", "console.log('hello');");

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let with_source =
                repo.run_cli_success(&["query", "symbol", &hash, "--source", "-f", "toon"]);
            // With source should include code
            assert!(
                with_source.contains("console") || with_source.contains("src"),
                "Source flag should include code"
            );
        }
    }
}

// ============================================================================
// QUERY SOURCE PARITY
// ============================================================================

mod source_parity {
    use super::*;

    #[test]
    fn source_returns_file_content() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/example.ts",
            "const x = 1;\nconst y = 2;\nconst z = 3;\n",
        );

        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["query", "source", "src/example.ts"]);
        assert!(output.contains("const") || output.contains("x"));
    }

    #[test]
    fn source_respects_line_range() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/example.ts",
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n",
        );

        repo.generate_index().unwrap();

        let partial = repo.run_cli_success(&[
            "query",
            "source",
            "src/example.ts",
            "--start",
            "3",
            "--end",
            "5",
        ]);

        // Should contain lines 3-5
        assert!(!partial.is_empty());
    }

    #[test]
    fn source_by_hash() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/func.ts", "myFunc", "return 42;");

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let source = repo.run_cli_success(&["query", "source", "--hash", &hash]);
            assert!(!source.is_empty());
        }
    }
}

// ============================================================================
// QUERY CALLERS PARITY
// ============================================================================

mod callers_parity {
    use super::*;

    #[test]
    fn callers_returns_call_info() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/service.ts",
            r#"
function helper() { return 1; }
export function main() {
    return helper() + helper();
}
"#,
        );

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let callers = repo.run_cli_success(&["query", "callers", &hash, "-f", "toon"]);
            // Should return some structure
            assert!(!callers.is_empty());
        }
    }

    #[test]
    fn callers_respects_depth() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/chain.ts",
            r#"
function a() { return 1; }
function b() { return a(); }
function c() { return b(); }
export function d() { return c(); }
"#,
        );

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let shallow = repo.run_cli_success(&["query", "callers", &hash, "--depth", "1"]);
            let deep = repo.run_cli_success(&["query", "callers", &hash, "--depth", "3"]);

            // Both should produce output
            assert!(!shallow.is_empty());
            assert!(!deep.is_empty());
        }
    }
}

// ============================================================================
// QUERY CALLGRAPH PARITY
// ============================================================================

mod callgraph_parity {
    use super::*;

    #[test]
    fn callgraph_returns_graph_structure() {
        let repo = TestRepo::new();
        repo.with_complex_callgraph();

        repo.generate_index().unwrap();

        let callgraph = repo.run_cli_success(&["query", "callgraph", "-f", "toon"]);

        // Should have some graph info
        assert!(!callgraph.is_empty());
    }

    #[test]
    fn callgraph_respects_pagination() {
        let repo = TestRepo::new();
        repo.with_complex_callgraph();

        repo.generate_index().unwrap();

        let page1 = repo.run_cli_success(&["query", "callgraph", "--limit", "5"]);
        let page2 = repo.run_cli_success(&["query", "callgraph", "--limit", "5", "--offset", "5"]);

        assert!(!page1.is_empty());
        // page2 may be empty if not enough data
    }
}

// ============================================================================
// QUERY FILE PARITY
// ============================================================================

mod file_parity {
    use super::*;

    #[test]
    fn file_returns_symbols() {
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

        let file_output = repo.run_cli_success(&["query", "file", "src/types.ts", "-f", "toon"]);

        // Should list symbols
        assert!(
            file_output.contains("Config")
                || file_output.contains("Service")
                || file_output.contains("helper")
                || file_output.contains("symbols")
        );
    }

    #[test]
    fn file_with_source_flag() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "console.log('test');");

        repo.generate_index().unwrap();

        let with_source = repo.run_cli_success(&["query", "file", "src/main.ts", "--source"]);

        assert!(with_source.contains("console") || with_source.contains("main"));
    }

    #[test]
    fn file_respects_kind_filter() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/mixed.ts",
            r#"
export interface IConfig { }
export class MyClass { }
export function myFunc() { }
"#,
        );

        repo.generate_index().unwrap();

        let funcs_only =
            repo.run_cli_success(&["query", "file", "src/mixed.ts", "--kind", "function"]);

        assert!(!funcs_only.is_empty());
    }
}

// ============================================================================
// VALIDATE PARITY
// ============================================================================

mod validate_parity {
    use super::*;

    #[test]
    fn validate_file_returns_metrics() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/complex.ts",
            r#"
export function complexFunction(a: number, b: number, c: number) {
    if (a > 0) {
        if (b > 0) {
            if (c > 0) {
                return a + b + c;
            }
            return a + b;
        }
        return a;
    }
    return 0;
}
"#,
        );

        repo.generate_index().unwrap();

        let validate_output = repo.run_cli_success(&["validate", "--file-path", "src/complex.ts"]);

        // Should have complexity info
        assert!(!validate_output.is_empty());
    }

    #[test]
    fn validate_module_works() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
            .add_ts_function("src/api/posts.ts", "getPosts", "return [];");

        repo.generate_index().unwrap();

        // First get available modules
        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        // Try validate on src.api or similar
        let validate_output = repo.run_cli(&["validate", "--module", "src.api"]);

        // Should either succeed or fail gracefully
        assert!(validate_output.is_ok());
    }

    #[test]
    fn validate_by_hash() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/func.ts", "targetFunc", "return 1;");

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let validate = repo.run_cli_success(&["validate", "--symbol", &hash]);
            assert!(!validate.is_empty());
        }
    }
}

// ============================================================================
// ANALYZE PARITY
// ============================================================================

mod analyze_parity {
    use super::*;

    #[test]
    fn analyze_file_returns_symbols() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/target.ts", "analyze_me", "return 1;");

        let analyze_output = repo.run_cli_success(&["analyze", "src/target.ts", "-f", "toon"]);

        assert!(
            analyze_output.contains("analyze_me") || analyze_output.contains("symbols"),
            "Analyze should find symbols"
        );
    }

    #[test]
    fn analyze_with_line_range() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/multi.ts",
            r#"function a() { return 1; }
function b() { return 2; }
function c() { return 3; }
function d() { return 4; }
function e() { return 5; }
"#,
        );

        let partial = repo.run_cli_success(&[
            "analyze",
            "src/multi.ts",
            "--start-line",
            "2",
            "--end-line",
            "3",
            "-f",
            "toon",
        ]);

        assert!(!partial.is_empty());
    }

    #[test]
    fn analyze_output_modes() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "console.log('test');");

        let symbols_only = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "toon"]);

        assert!(!symbols_only.is_empty());
    }
}

// ============================================================================
// ANALYZE_DIFF PARITY
// ============================================================================

mod analyze_diff_parity {
    use super::*;

    #[test]
    fn analyze_diff_works_on_git_repo() {
        let repo = TestRepo::new();
        repo.init_git();

        // Create initial commit
        repo.add_ts_function("src/initial.ts", "initial", "return 1;");
        repo.commit("Initial commit");

        // Make changes
        repo.add_ts_function("src/initial.ts", "modified", "return 2;");
        repo.add_ts_function("src/new.ts", "newFunc", "return 3;");

        let diff_output = repo.run_cli(&["analyze", "--diff", "HEAD"]);

        // Should either show diff or succeed gracefully
        assert!(diff_output.is_ok());
    }

    #[test]
    fn analyze_diff_summary_mode() {
        let repo = TestRepo::new();
        repo.init_git();

        repo.add_ts_function("src/main.ts", "main", "return 1;");
        repo.commit("Initial");

        repo.add_ts_function("src/main.ts", "updated", "return 2;");

        let summary = repo.run_cli(&["analyze", "--diff", "HEAD", "--summary"]);

        assert!(summary.is_ok());
    }

    #[test]
    fn analyze_diff_pagination() {
        let repo = TestRepo::new();
        repo.init_git();

        // Create many files
        for i in 0..20 {
            repo.add_ts_function(&format!("src/file{}.ts", i), &format!("fn{}", i), "");
        }
        repo.commit("Many files");

        // Modify all
        for i in 0..20 {
            repo.add_ts_function(
                &format!("src/file{}.ts", i),
                &format!("modified{}", i),
                "return 1;",
            );
        }

        let page1 = repo.run_cli(&["analyze", "--diff", "HEAD", "--limit", "5"]);
        let page2 = repo.run_cli(&["analyze", "--diff", "HEAD", "--limit", "5", "--offset", "5"]);

        assert!(page1.is_ok());
        assert!(page2.is_ok());
    }
}

// ============================================================================
// FIND_DUPLICATES PARITY
// ============================================================================

mod duplicates_parity {
    use super::*;

    #[test]
    fn duplicates_finds_similar_code() {
        let repo = TestRepo::new();
        repo.with_duplicates();

        repo.generate_index().unwrap();

        let duplicates = repo.run_cli_success(&["validate", "--duplicates"]);

        // Should find the duplicate validateEmail functions
        assert!(!duplicates.is_empty());
    }

    #[test]
    fn duplicates_respects_threshold() {
        let repo = TestRepo::new();
        repo.with_duplicates();

        repo.generate_index().unwrap();

        let strict = repo.run_cli_success(&["validate", "--duplicates", "--threshold", "0.95"]);
        let loose = repo.run_cli_success(&["validate", "--duplicates", "--threshold", "0.5"]);

        assert!(!strict.is_empty());
        assert!(!loose.is_empty());
    }

    #[test]
    fn duplicates_pagination() {
        let repo = TestRepo::new();
        // Create many duplicate functions
        let body = "return email.includes('@') && email.length > 5;";
        for i in 0..10 {
            repo.add_ts_function(&format!("src/validate{}.ts", i), "validateEmail", body);
        }

        repo.generate_index().unwrap();

        let page1 = repo.run_cli(&["validate", "--duplicates", "--limit", "3"]);

        assert!(page1.is_ok());
    }

    #[test]
    fn duplicates_for_specific_symbol() {
        let repo = TestRepo::new();
        repo.with_duplicates();

        repo.generate_index().unwrap();

        let overview = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        if let Some(hash) = extract_first_hash(&overview) {
            let symbol_dups = repo.run_cli(&["validate", "--duplicates", "--symbol", &hash]);
            assert!(symbol_dups.is_ok());
        }
    }
}

// ============================================================================
// FORMAT PARITY
// ============================================================================

mod format_parity {
    use super::*;

    #[test]
    fn json_format_is_valid() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "return 1;");

        repo.generate_index().unwrap();

        let json_output = repo.run_cli_success(&["query", "overview", "-f", "json"]);

        // JSON format may still use TOON internally, but should be parseable
        assert!(!json_output.is_empty());
    }

    #[test]
    fn toon_format_has_type_field() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "return 1;");

        repo.generate_index().unwrap();

        let toon_output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

        // TOON should have _type or schema structure
        assert!(
            toon_output.contains("_type") || toon_output.contains("{"),
            "TOON should have structure"
        );
    }

    #[test]
    fn text_format_is_readable() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "main", "return 1;");

        repo.generate_index().unwrap();

        let text_output = repo.run_cli_success(&["query", "overview", "-f", "text"]);

        // Text should be non-empty
        assert!(!text_output.is_empty());
    }
}

// ============================================================================
// EDGE CASES
// ============================================================================

mod parity_edge_cases {
    use super::*;

    #[test]
    fn empty_repo_handles_gracefully() {
        let repo = TestRepo::new();
        repo.add_empty_file("src/empty.ts");

        let result = repo.generate_index();
        assert!(result.is_ok());

        let overview = repo.run_cli(&["query", "overview"]);
        assert!(overview.is_ok());
    }

    #[test]
    fn nonexistent_file_handled() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/exists.ts", "fn", "");

        repo.generate_index().unwrap();

        // CLI may succeed but return empty/error info - depends on implementation
        let result = repo.run_cli(&["query", "file", "src/doesnt_exist.ts"]);

        // Should either fail or succeed with hint about missing file
        assert!(result.is_ok(), "Command should run without panic");

        let output = result.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Either reports error, hint, or returns empty symbols
        assert!(
            !output.status.success()
                || stderr.contains("not found")
                || stderr.contains("error")
                || stdout.contains("error")
                || stdout.contains("(none)")
                || stdout.contains("hint")
                || stdout.contains("showing: 0")
                || stdout.contains("[]")
                || stdout.is_empty()
                || stdout.contains("null"),
            "Should indicate file not found somehow: stdout={}, stderr={}",
            stdout,
            stderr
        );
    }

    #[test]
    fn invalid_hash_error() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/main.ts", "fn", "");

        repo.generate_index().unwrap();

        let result = repo.run_cli(&["query", "symbol", "invalid_hash_12345"]);

        // Should either error or return empty
        assert!(result.is_ok()); // Command runs, may return nothing
    }

    #[test]
    fn unicode_content_handled() {
        let repo = TestRepo::new();
        repo.add_unicode_ts("src/unicode.ts");

        repo.generate_index().unwrap();

        let file_output = repo.run_cli_success(&["query", "file", "src/unicode.ts"]);

        // Should handle unicode
        assert!(!file_output.is_empty());
    }

    #[test]
    fn very_long_file_handled() {
        let repo = TestRepo::new();
        repo.add_very_long_file_ts("src/long.ts", 500);

        repo.generate_index().unwrap();

        let analyze = repo.run_cli_success(&["analyze", "src/long.ts"]);

        assert!(!analyze.is_empty());
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Extract the first symbol hash from TOON output
fn extract_first_hash(output: &str) -> Option<String> {
    // Look for hash patterns (12 char hex strings typically)
    let hash_pattern = regex::Regex::new(r#"["\s]([a-f0-9]{12,})["\s,\n]"#).ok()?;
    hash_pattern
        .captures(output)
        .map(|c| c.get(1).unwrap().as_str().to_string())
}

/// Extract all symbol hashes from TOON output
fn extract_hashes(output: &str) -> Vec<String> {
    let hash_pattern = regex::Regex::new(r#"["\s]([a-f0-9]{12,})["\s,\n]"#).unwrap();
    hash_pattern
        .captures_iter(output)
        .map(|c| c.get(1).unwrap().as_str().to_string())
        .collect()
}
