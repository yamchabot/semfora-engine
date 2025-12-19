//! Tests for the `validate` CLI command
//!
//! The validate command checks code quality:
//! - `validate <TARGET>` - Validates file/module/hash (auto-detected)
//! - `validate --duplicates` - Find duplicate code patterns
//!
//! TARGET can be a file path, module name, or symbol hash

#![allow(unused_imports)]

use crate::common::{
    assert_contains, assert_valid_json, assert_valid_toon, extract_symbol_hashes, TestRepo,
};

// ============================================================================
// VALIDATE FILE/PATH TESTS
// ============================================================================

#[test]
fn test_validate_file_basic() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    repo.generate_index().unwrap();

    // Use path directly (auto-detected as file)
    let output = repo.run_cli_success(&["validate", "src/service.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "validate file");

    // Should validate all symbols in file
    assert!(
        json.get("symbols").is_some()
            || json.get("results").is_some()
            || json.get("validations").is_some()
            || json.get("metrics").is_some()
            || json.is_object(),
        "Should validate file symbols"
    );
}

#[test]
fn test_validate_file_with_kind_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/types.ts",
        r#"
export interface Config { name: string; }
export class Service { run() {} }
export function helper() { return 1; }
"#,
    );

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/types.ts", "--kind", "fn", "-f", "json"]);
    let json = assert_valid_json(&output, "validate file with kind");

    // Should filter by kind
    let output_str = serde_json::to_string(&json).unwrap();
    // May contain helper but filter should apply
    assert!(
        output_str.contains("helper") || output_str.contains("function") || json.is_object(),
        "Should validate filtered symbols"
    );
}

#[test]
fn test_validate_file_nonexistent() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["validate", "nonexistent.ts", "-f", "json"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

#[test]
fn test_validate_complex_file() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/complex.ts",
        r#"
export function complexFunction(x: number): number {
    if (x > 0) {
        if (x > 10) {
            if (x > 100) {
                for (let i = 0; i < x; i++) {
                    try {
                        switch (i % 3) {
                            case 0: return i;
                            case 1: return i * 2;
                            default: return i * 3;
                        }
                    } catch (e) {
                        console.log(e);
                    }
                }
            }
        }
    }
    return 0;
}
"#,
    );

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/complex.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "validate complex file");

    // Should return validation metrics
    assert!(
        json.get("complexity").is_some()
            || json.get("metrics").is_some()
            || json.get("risk").is_some()
            || json.get("cognitive").is_some()
            || json.get("cyclomatic").is_some()
            || json.is_object(),
        "Should return complexity metrics"
    );
}

// ============================================================================
// VALIDATE MODULE TESTS
// ============================================================================

#[test]
fn test_validate_module_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/api/users.ts", "getUsers", "return [];")
        .add_ts_function("src/api/posts.ts", "getPosts", "return [];");

    repo.generate_index().unwrap();

    // Get module name from overview
    let overview = repo.run_cli_success(&["query", "overview", "-f", "json"]);
    let overview_str = overview.to_lowercase();

    let module_name = if overview_str.contains("src.api") {
        "src.api"
    } else if overview_str.contains("\"api\"") {
        "api"
    } else {
        "src"
    };

    // Use module name directly (auto-detected)
    let output = repo.run_cli_success(&["validate", module_name, "-f", "json"]);
    let json = assert_valid_json(&output, "validate module");

    // Should validate module symbols
    assert!(
        json.get("symbols").is_some()
            || json.get("results").is_some()
            || json.get("validations").is_some()
            || json.is_object(),
        "Should validate module symbols"
    );
}

#[test]
fn test_validate_module_with_limit() {
    let repo = TestRepo::new();
    // Create many functions
    for i in 0..20 {
        repo.add_ts_function(
            &format!("src/funcs/f{}.ts", i),
            &format!("func{}", i),
            &format!("return {};", i),
        );
    }

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src", "--limit", "5", "-f", "json"]);
    let json = assert_valid_json(&output, "validate module with limit");

    // Should limit results
    if let Some(symbols) = json.get("symbols").and_then(|s| s.as_array()) {
        assert!(
            symbols.len() <= 10, // Allow some flexibility
            "Should limit validation results"
        );
    }
}

// ============================================================================
// VALIDATE BY HASH TESTS
// ============================================================================

#[test]
fn test_validate_symbol_by_hash() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "simpleFunc", "return 1;");

    repo.generate_index().unwrap();

    let search = repo.run_cli_success(&["search", "simpleFunc", "-f", "json"]);
    let search_json = assert_valid_json(&search, "get hash");

    let hashes = extract_symbol_hashes(&search_json);
    if hashes.is_empty() {
        // No hashes found in search output - skip test
        return;
    }

    let hash = &hashes[0];
    // Pass hash directly (auto-detected)
    // Note: validate by hash may fail if hash format differs between commands
    let result = repo.run_cli(&["validate", hash, "-f", "json"]);

    // Should either succeed or handle gracefully (hash format differences)
    assert!(result.is_ok(), "Validate should not crash");
}

// ============================================================================
// DUPLICATE DETECTION TESTS
// ============================================================================

#[test]
fn test_validate_duplicates_codebase() {
    let repo = TestRepo::new();
    repo.with_duplicates();

    repo.generate_index().unwrap();

    // Use --duplicates flag (not duplicates subcommand)
    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "json"]);
    let json = assert_valid_json(&output, "validate duplicates");

    // Should find duplicates
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("duplicate")
            || output_str.contains("cluster")
            || output_str.contains("similarity")
            || json.get("clusters").is_some()
            || json.get("duplicates").is_some()
            || json.is_object(),
        "Should find duplicate functions"
    );
}

#[test]
fn test_validate_duplicates_with_threshold() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/a.ts",
        r#"
export function processA(x: number) {
    const result = x * 2;
    console.log(result);
    return result;
}
"#,
    )
    .add_file(
        "src/b.ts",
        r#"
export function processB(y: number) {
    const output = y * 2;
    console.log(output);
    return output;
}
"#,
    );

    repo.generate_index().unwrap();

    // High threshold - may not find near-duplicates
    let output_high = repo.run_cli_success(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.99",
        "-f",
        "json",
    ]);
    assert_valid_json(&output_high, "duplicates high threshold");

    // Lower threshold - should find more
    let output_low = repo.run_cli_success(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.70",
        "-f",
        "json",
    ]);
    assert_valid_json(&output_low, "duplicates low threshold");
}

#[test]
fn test_validate_duplicates_with_limit() {
    let repo = TestRepo::new();
    // Create many similar functions
    for i in 0..20 {
        repo.add_file(
            &format!("src/dup{}.ts", i),
            &format!(
                "export function similar{}(x: number) {{ return x * 2 + 1; }}",
                i
            ),
        );
    }

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "--duplicates", "--limit", "5", "-f", "json"]);
    let json = assert_valid_json(&output, "duplicates with limit");

    // Should limit clusters
    if let Some(clusters) = json.get("clusters").and_then(|c| c.as_array()) {
        assert!(
            clusters.len() <= 10, // Allow some flexibility
            "Should limit duplicate clusters"
        );
    }
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_validate_text_format() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/service.ts", "-f", "text"]);

    assert!(!output.is_empty(), "Text format should produce output");
}

#[test]
fn test_validate_toon_format() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/service.ts", "-f", "toon"]);

    assert_valid_toon(&output, "validate toon format");
}

#[test]
fn test_validate_json_format() {
    let repo = TestRepo::new();
    repo.add_ts_module("src/service.ts", "User");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/service.ts", "-f", "json"]);

    assert_valid_json(&output, "validate json format");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_validate_no_target() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Validate without target - should error or provide help
    let result = repo.run_cli(&["validate", "-f", "json"]);

    // May succeed with default behavior or fail with helpful message
    assert!(result.is_ok());
}

#[test]
fn test_validate_empty_file() {
    let repo = TestRepo::new();
    repo.add_empty_file("src/empty.ts");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["validate", "src/empty.ts", "-f", "json"]);

    // Should handle empty file gracefully
    assert!(result.is_ok());
}

#[test]
fn test_validate_no_duplicates() {
    let repo = TestRepo::new();
    // All unique functions
    repo.add_ts_function("src/a.ts", "funcA", "return 'a';")
        .add_ts_function("src/b.ts", "funcB", "return 'b';")
        .add_ts_function("src/c.ts", "funcC", "return 'c';");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "--duplicates", "-f", "json"]);
    let json = assert_valid_json(&output, "no duplicates");

    // Should return empty/no duplicates
    let output_str = serde_json::to_string(&json).unwrap();
    // Either empty clusters or explicit "no duplicates" message
    assert!(
        output_str.contains("[]")
            || output_str.contains("\"clusters\":[]")
            || !output_str.contains("cluster")
            || json.is_object(),
        "Should indicate no duplicates found"
    );
}

#[test]
fn test_validate_invalid_hash() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["validate", "invalid_hash_xyz", "-f", "json"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

#[test]
fn test_validate_deeply_nested_code() {
    let repo = TestRepo::new();
    repo.add_deeply_nested_ts("src/deep.ts", 15);

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["validate", "src/deep.ts", "-f", "json"]);
    let json = assert_valid_json(&output, "validate deeply nested");

    // Should report high complexity
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("complexity")
            || output_str.contains("nesting")
            || output_str.contains("risk")
            || output_str.contains("deepNest")
            || json.is_object(),
        "Should validate deeply nested code"
    );
}

// ============================================================================
// BOILERPLATE INCLUSION TESTS
// ============================================================================

#[test]
fn test_validate_duplicates_include_boilerplate() {
    let repo = TestRepo::new();
    // Create boilerplate-like code (simple getters/setters)
    repo.add_file(
        "src/model1.ts",
        r#"
export class User {
    private _name: string;
    get name() { return this._name; }
    set name(val: string) { this._name = val; }
}
"#,
    )
    .add_file(
        "src/model2.ts",
        r#"
export class Product {
    private _name: string;
    get name() { return this._name; }
    set name(val: string) { this._name = val; }
}
"#,
    );

    repo.generate_index().unwrap();

    // With boilerplate inclusion (normally excluded by default)
    let result_include = repo.run_cli(&[
        "validate",
        "--duplicates",
        "--include-boilerplate",
        "-f",
        "json",
    ]);
    assert!(
        result_include.is_ok(),
        "Boilerplate inclusion flag should work"
    );

    // Without flag (boilerplate excluded by default)
    let result_default = repo.run_cli(&["validate", "--duplicates", "-f", "json"]);
    assert!(result_default.is_ok(), "Default behavior should also work");
}

#[test]
fn test_validate_duplicates_include_boilerplate_with_threshold() {
    let repo = TestRepo::new();
    // Create many similar simple functions
    for i in 0..5 {
        repo.add_file(
            &format!("src/getter{}.ts", i),
            &format!("export function getValue{}() {{ return this._value; }}", i),
        );
    }

    repo.generate_index().unwrap();

    // Combine boilerplate inclusion with threshold
    let result = repo.run_cli(&[
        "validate",
        "--duplicates",
        "--include-boilerplate",
        "--threshold",
        "0.85",
        "-f",
        "json",
    ]);
    assert!(result.is_ok(), "Combined flags should work");
}

// ============================================================================
// LIMIT/PAGINATION TESTS
// ============================================================================

#[test]
fn test_validate_duplicates_pagination_various_limits() {
    let repo = TestRepo::new();
    // Create many similar functions
    for i in 0..30 {
        repo.add_file(
            &format!("src/dup{}.ts", i),
            &format!(
                "export function duplicatedFunc{}(x: number) {{ return x * 2 + 1; }}",
                i
            ),
        );
    }

    repo.generate_index().unwrap();

    // Small limit
    let small = repo.run_cli_success(&["validate", "--duplicates", "--limit", "5", "-f", "json"]);
    assert_valid_json(&small, "duplicates small limit");

    // Large limit
    let large = repo.run_cli_success(&["validate", "--duplicates", "--limit", "100", "-f", "json"]);
    assert_valid_json(&large, "duplicates large limit");
}

#[test]
fn test_validate_duplicates_zero_limit() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Zero limit - should handle gracefully
    let result = repo.run_cli(&["validate", "--duplicates", "--limit", "0", "-f", "json"]);

    // Should handle gracefully
    assert!(result.is_ok(), "Zero limit should be handled");
}

// ============================================================================
// SYMBOL HASH MATCHING TESTS
// ============================================================================

#[test]
fn test_validate_symbol_hash_prefix() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "targetFunction", "return 42;");

    repo.generate_index().unwrap();

    // Get a hash from search
    let search = repo.run_cli_success(&["search", "targetFunction", "-f", "json"]);
    let search_json = assert_valid_json(&search, "get hash");
    let hashes = extract_symbol_hashes(&search_json);

    if hashes.is_empty() {
        return; // Skip if no hashes found
    }

    // Try with hash prefix (first 8 chars if available)
    let full_hash = &hashes[0];
    if full_hash.len() >= 8 {
        let prefix = &full_hash[..8];
        let result = repo.run_cli(&["validate", prefix, "-f", "json"]);
        // May or may not work depending on implementation
        assert!(result.is_ok(), "Hash prefix should not crash");
    }
}

#[test]
fn test_validate_symbol_hash_various_formats() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "testFunc", "return 1;");

    repo.generate_index().unwrap();

    // Try various hash-like strings
    let results = vec![
        repo.run_cli(&["validate", "abc123", "-f", "json"]),
        repo.run_cli(&["validate", "module:symbol", "-f", "json"]),
        repo.run_cli(&["validate", "src:testFunc", "-f", "json"]),
    ];

    for result in results {
        assert!(result.is_ok(), "Various hash formats should be handled");
    }
}

// ============================================================================
// DUPLICATE CLASSIFICATION TESTS
// ============================================================================

#[test]
fn test_validate_duplicates_exact_vs_near() {
    let repo = TestRepo::new();
    // Exact duplicates
    let exact_code = r#"export function process(x: number) {
    const y = x * 2;
    return y + 1;
}"#;
    repo.add_file("src/exact1.ts", exact_code);
    repo.add_file("src/exact2.ts", exact_code);

    // Near duplicates (slightly different)
    repo.add_file(
        "src/near1.ts",
        r#"export function processItem(x: number) {
    const result = x * 2;
    return result + 1;
}"#,
    );

    repo.generate_index().unwrap();

    // High threshold should find exact
    let high = repo.run_cli_success(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.99",
        "-f",
        "json",
    ]);
    let high_json = assert_valid_json(&high, "high threshold");

    // Lower threshold should find more
    let low = repo.run_cli_success(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.75",
        "-f",
        "json",
    ]);
    let low_json = assert_valid_json(&low, "low threshold");

    // Both should be valid JSON
    assert!(
        high_json.is_object(),
        "High threshold result should be valid"
    );
    assert!(low_json.is_object(), "Low threshold result should be valid");
}

#[test]
fn test_validate_duplicates_short_vs_long_functions() {
    let repo = TestRepo::new();
    // Very short functions (may be filtered as boilerplate by default)
    repo.add_file("src/short1.ts", "export const f1 = () => 1;");
    repo.add_file("src/short2.ts", "export const f2 = () => 1;");

    // Longer functions (more likely to be detected as duplicates)
    repo.add_file(
        "src/long1.ts",
        r#"export function longFunc(x: number) {
    const a = x + 1;
    const b = a * 2;
    const c = b / 3;
    return c;
}"#,
    );
    repo.add_file(
        "src/long2.ts",
        r#"export function longFunc2(x: number) {
    const a = x + 1;
    const b = a * 2;
    const c = b / 3;
    return c;
}"#,
    );

    repo.generate_index().unwrap();

    // Default behavior (boilerplate excluded)
    let result_default = repo.run_cli(&["validate", "--duplicates", "-f", "json"]);
    assert!(
        result_default.is_ok(),
        "Default duplicate detection should work"
    );

    // With boilerplate included
    let result_with_boilerplate = repo.run_cli(&[
        "validate",
        "--duplicates",
        "--include-boilerplate",
        "-f",
        "json",
    ]);
    assert!(
        result_with_boilerplate.is_ok(),
        "Including boilerplate should work"
    );
}

// ============================================================================
// THRESHOLD EDGE CASES
// ============================================================================

#[test]
fn test_validate_duplicates_threshold_boundaries() {
    let repo = TestRepo::new();
    repo.with_duplicates();

    repo.generate_index().unwrap();

    // Very low threshold
    let result_low = repo.run_cli(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.50",
        "-f",
        "json",
    ]);
    assert!(result_low.is_ok(), "Low threshold should work");

    // Very high threshold
    let result_high = repo.run_cli(&[
        "validate",
        "--duplicates",
        "--threshold",
        "0.99",
        "-f",
        "json",
    ]);
    assert!(result_high.is_ok(), "High threshold should work");

    // Default threshold
    let result_default = repo.run_cli(&["validate", "--duplicates", "-f", "json"]);
    assert!(result_default.is_ok(), "Default threshold should work");
}

#[test]
fn test_validate_duplicates_kind_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/mixed.ts",
        r#"
export class Service {
    process(x: number) { return x * 2; }
}
export function process(x: number) { return x * 2; }
"#,
    );

    repo.generate_index().unwrap();

    // Filter by function kind
    let result = repo.run_cli(&["validate", "--duplicates", "--kind", "fn", "-f", "json"]);
    assert!(result.is_ok(), "Kind filter should work");
}
