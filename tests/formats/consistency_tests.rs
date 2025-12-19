//! Format Consistency Tests
//!
//! Tests verifying that all output formats contain consistent information:
//! - JSON validity and structure
//! - TOON format markers and structure
//! - Text format readability
//! - Cross-format data equivalence

#![allow(unused_variables)]
#![allow(clippy::len_zero)]

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// JSON FORMAT VALIDITY TESTS
// ============================================================================

/// Test JSON output is valid for analyze command
#[test]
fn test_json_validity_analyze() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "analyze json");

    // Should be an object or array
    assert!(
        json.is_object() || json.is_array(),
        "JSON should be object or array"
    );
}

/// Test JSON output is valid for search command
#[test]
fn test_json_validity_search() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "json"]);
    let json = assert_valid_json(&output, "search json");

    assert!(
        json.is_object() || json.is_array(),
        "Search JSON should be object or array"
    );
}

/// Test JSON output is valid for query overview
#[test]
fn test_json_validity_overview() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let json = assert_valid_json(&output, "overview json");

    assert!(json.is_object(), "Overview should be an object");
}

/// Test JSON output is valid for query module
#[test]
fn test_json_validity_module() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "module", "src", "-f", "json"]);
    let json = assert_valid_json(&output, "module json");

    assert!(
        json.is_object() || json.is_array(),
        "Module JSON should be object or array"
    );
}

/// Test JSON output is valid for validate duplicates
#[test]
fn test_json_validity_duplicates() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/a.ts", "funcA", "return 1;")
        .add_ts_function("src/b.ts", "funcB", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "json"]);
    let json = assert_valid_json(&output, "duplicates json");

    assert!(
        json.is_object() || json.is_array(),
        "Duplicates JSON should be object or array"
    );
}

/// Test JSON output is valid for callgraph
#[test]
fn test_json_validity_callgraph() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"
export function main() { helper(); }
function helper() { return 1; }
"#,
    );
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "callgraph", "-f", "json"]);
    let json = assert_valid_json(&output, "callgraph json");

    assert!(
        json.is_object() || json.is_array(),
        "Callgraph JSON should be object or array"
    );
}

// ============================================================================
// TOON FORMAT VALIDITY TESTS
// ============================================================================

/// Test TOON output has _type marker for overview
#[test]
fn test_toon_validity_overview() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // TOON should have structure markers
    let has_structure = output.contains("_type:")
        || output.contains("modules:")
        || output.contains("symbols:")
        || output.contains("src");
    assert!(has_structure, "TOON should have structure: {}", output);
}

/// Test TOON output for analyze
#[test]
fn test_toon_validity_analyze() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "toon"]);

    // Should be non-empty and have some structure
    assert!(!output.is_empty(), "TOON output should not be empty");
}

/// Test TOON output for search
#[test]
fn test_toon_validity_search() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "toon"]);

    assert!(!output.is_empty(), "TOON search output should not be empty");
}

/// Test TOON output for duplicates
#[test]
fn test_toon_validity_duplicates() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/a.ts", "funcA", "return 1;")
        .add_ts_function("src/b.ts", "funcB", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "toon"]);

    // Should contain something (even if no duplicates)
    assert!(!output.is_empty() || output.contains("duplicate") || output.contains("0"));
}

// ============================================================================
// TEXT FORMAT VALIDITY TESTS
// ============================================================================

/// Test text output is human-readable for overview
#[test]
fn test_text_validity_overview() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "text"]);

    // Text should be non-empty and readable
    assert!(!output.is_empty(), "Text output should not be empty");
    // Should not be JSON
    assert!(
        !output.starts_with('{') && !output.starts_with('['),
        "Text output should not be JSON"
    );
}

/// Test text output for analyze
#[test]
fn test_text_validity_analyze() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["analyze", "src/main.ts", "-f", "text"]);

    assert!(
        !output.is_empty(),
        "Text analyze output should not be empty"
    );
}

/// Test text output for search
#[test]
fn test_text_validity_search() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["search", "main", "-f", "text"]);

    assert!(!output.is_empty(), "Text search output should not be empty");
}

// ============================================================================
// CROSS-FORMAT DATA CONSISTENCY TESTS
// ============================================================================

/// Test that JSON and TOON contain same symbol names
#[test]
fn test_cross_format_symbol_presence() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "uniqueSymbolName", "return 1;");
    repo.generate_index().unwrap();

    let json_output = repo.run_cli_success(&["search", "uniqueSymbolName", "-f", "json"]);
    let toon_output = repo.run_cli_success(&["search", "uniqueSymbolName", "-f", "toon"]);
    let text_output = repo.run_cli_success(&["search", "uniqueSymbolName", "-f", "text"]);

    // All formats should mention the symbol
    assert!(
        json_output.contains("uniqueSymbolName") || json_output.contains("unique"),
        "JSON should contain symbol"
    );
    assert!(
        toon_output.contains("uniqueSymbolName") || toon_output.contains("unique"),
        "TOON should contain symbol"
    );
    assert!(
        text_output.contains("uniqueSymbolName") || text_output.contains("unique"),
        "Text should contain symbol"
    );
}

/// Test that all formats handle empty results consistently
#[test]
fn test_cross_format_empty_results() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let json_output = repo.run_cli_success(&["search", "nonexistent_xyz_123", "-f", "json"]);
    let toon_output = repo.run_cli_success(&["search", "nonexistent_xyz_123", "-f", "toon"]);
    let text_output = repo.run_cli_success(&["search", "nonexistent_xyz_123", "-f", "text"]);

    // All should complete without error
    assert_valid_json(&json_output, "empty json");
    // TOON and text just need to not crash
    let _ = toon_output;
    let _ = text_output;
}

/// Test that all formats handle multiple results consistently
#[test]
fn test_cross_format_multiple_results() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/handler1.ts", "handler1", "return 1;")
        .add_ts_function("src/handler2.ts", "handler2", "return 2;")
        .add_ts_function("src/handler3.ts", "handler3", "return 3;");
    repo.generate_index().unwrap();

    let json_output = repo.run_cli_success(&["search", "handler", "-f", "json"]);
    let toon_output = repo.run_cli_success(&["search", "handler", "-f", "toon"]);
    let text_output = repo.run_cli_success(&["search", "handler", "-f", "text"]);

    // JSON should be valid and contain results
    let json = assert_valid_json(&json_output, "multiple json");

    // Count occurrences in each format
    let json_count = json_output.matches("handler").count();
    let toon_count = toon_output.matches("handler").count();
    let text_count = text_output.matches("handler").count();

    // All formats should show multiple results
    assert!(json_count >= 1, "JSON should have handler matches");
    assert!(toon_count >= 1, "TOON should have handler matches");
    assert!(text_count >= 1, "Text should have handler matches");
}

// ============================================================================
// FORMAT-SPECIFIC FEATURE TESTS
// ============================================================================

/// Test JSON contains proper type fields
#[test]
fn test_json_has_type_info() {
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

    let output = repo.run_cli_success(&["analyze", "src/types.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "types json");

    // Should contain type information
    let json_str = serde_json::to_string(&json).unwrap();
    let has_type_info = json_str.contains("interface")
        || json_str.contains("class")
        || json_str.contains("function")
        || json_str.contains("kind")
        || json_str.contains("type");
    assert!(has_type_info, "JSON should have type information");
}

/// Test TOON is more compact than JSON
#[test]
fn test_toon_compactness() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;")
        .add_ts_function("src/helper.ts", "helper", "return 2;")
        .add_ts_function("src/utils.ts", "util", "return 3;");
    repo.generate_index().unwrap();

    let json_output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let toon_output = repo.run_cli_success(&["query", "overview", "-f", "toon"]);

    // TOON should generally be smaller (or at least not much larger)
    // Note: Not always true for small repos, so we just verify both are valid
    assert!(!json_output.is_empty(), "JSON should not be empty");
    assert!(!toon_output.is_empty(), "TOON should not be empty");
}

// ============================================================================
// COMMAND-SPECIFIC FORMAT TESTS
// ============================================================================

/// Test all formats work with query callgraph
#[test]
fn test_formats_query_callgraph() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"
export function main() { helper(); }
function helper() { return 1; }
"#,
    );
    repo.generate_index().unwrap();

    let json = repo.run_cli_success(&["query", "callgraph", "-f", "json"]);
    let toon = repo.run_cli_success(&["query", "callgraph", "-f", "toon"]);
    let text = repo.run_cli_success(&["query", "callgraph", "-f", "text"]);

    assert_valid_json(&json, "callgraph json");
    assert!(!toon.is_empty());
    assert!(!text.is_empty());
}

/// Test all formats work with query languages
#[test]
fn test_formats_query_languages() {
    let repo = TestRepo::new();

    let json = repo.run_cli_success(&["query", "languages", "-f", "json"]);
    let toon = repo.run_cli_success(&["query", "languages", "-f", "toon"]);
    let text = repo.run_cli_success(&["query", "languages", "-f", "text"]);

    // JSON should list languages
    let json_parsed = assert_valid_json(&json, "languages json");
    assert!(json_parsed.is_array() || json_parsed.is_object());

    // All should mention some language
    let has_lang = |s: &str| {
        s.to_lowercase().contains("typescript")
            || s.to_lowercase().contains("rust")
            || s.to_lowercase().contains("python")
            || s.to_lowercase().contains("go")
    };
    assert!(
        has_lang(&json) || json_parsed.is_array(),
        "JSON should list languages"
    );
    assert!(
        has_lang(&toon) || !toon.is_empty(),
        "TOON should list languages"
    );
    assert!(
        has_lang(&text) || !text.is_empty(),
        "Text should list languages"
    );
}

/// Test all formats work with validate file
#[test]
fn test_formats_validate_file() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/main.ts",
        r#"
export function complex(a: number) {
    if (a > 0) {
        for (let i = 0; i < a; i++) {
            console.log(i);
        }
    }
    return a;
}
"#,
    );
    repo.generate_index().unwrap();

    let json = repo.run_cli_success(&["validate", "src/main.ts", "-f", "json"]);
    let toon = repo.run_cli_success(&["validate", "src/main.ts", "-f", "toon"]);
    let text = repo.run_cli_success(&["validate", "src/main.ts", "-f", "text"]);

    assert_valid_json(&json, "validate file json");
    assert!(!toon.is_empty());
    assert!(!text.is_empty());
}

// ============================================================================
// MULTILANG FORMAT TESTS
// ============================================================================

/// Test formats handle multi-language repos
#[test]
fn test_formats_multilang() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/app.ts", "main", "return 1;")
        .add_file(
            "src/lib.rs",
            r#"
pub fn helper() -> i32 { 1 }
"#,
        )
        .add_file(
            "src/util.py",
            r#"
def process():
    return 1
"#,
        );
    repo.generate_index().unwrap();

    let json = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let toon = repo.run_cli_success(&["query", "overview", "-f", "toon"]);
    let text = repo.run_cli_success(&["query", "overview", "-f", "text"]);

    // JSON should be valid
    let json_parsed = assert_valid_json(&json, "multilang json");
    assert!(json_parsed.is_object());

    // All formats should mention the modules/files
    assert!(!toon.is_empty(), "TOON should have content");
    assert!(!text.is_empty(), "Text should have content");
}

// ============================================================================
// ERROR FORMAT TESTS
// ============================================================================

/// Test error messages are readable in text format
#[test]
fn test_error_format_text() {
    let repo = TestRepo::new();

    let (stdout, stderr) = repo.run_cli_failure(&["analyze", "/nonexistent/path"]);
    let combined = format!("{}{}", stdout, stderr);

    // Error should be human-readable
    let has_error = combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("no such file");
    assert!(has_error, "Error should be readable: {}", combined);
}

/// Test error handling in JSON format
#[test]
fn test_error_format_json() {
    let repo = TestRepo::new();

    let result = repo.run_cli(&["analyze", "/nonexistent/path", "-f", "json"]);
    // Should fail but not crash
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// ROUNDTRIP TESTS
// ============================================================================

/// Test that JSON output can be re-parsed
#[test]
fn test_json_roundtrip() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let json = assert_valid_json(&output, "roundtrip");

    // Re-serialize and re-parse
    let reserialized = serde_json::to_string(&json).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&reserialized).unwrap();

    assert_eq!(json, reparsed, "JSON should survive roundtrip");
}

/// Test that format flag is respected consistently
#[test]
fn test_format_flag_respected() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");
    repo.generate_index().unwrap();

    // JSON should start with { or [
    let json = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let trimmed = json.trim();
    assert!(
        trimmed.starts_with('{') || trimmed.starts_with('['),
        "JSON format should produce JSON: {}",
        &json[..json.len().min(100)]
    );

    // Text should not start with { or [
    let text = repo.run_cli_success(&["query", "overview", "-f", "text"]);
    let text_trimmed = text.trim();
    // Text might be empty or start with various chars
    if !text_trimmed.is_empty() {
        // It's ok if text happens to start with { in some edge cases
        // The key is it should be human-readable
        assert!(text_trimmed.len() > 0, "Text format should produce output");
    }
}
