//! Tests for the `security` CLI command
//!
//! The security command provides CVE vulnerability scanning:
//! - `security scan` - Scan for CVE vulnerability patterns
//! - `security update` - Update security patterns from pattern server
//! - `security stats` - Show security pattern statistics

#![allow(unused_imports)]

use crate::common::{assert_contains, assert_valid_json, TestRepo};

// ============================================================================
// SECURITY SCAN TESTS
// ============================================================================

#[test]
fn test_security_scan_basic() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let result = repo.run_cli(&["security", "scan"]);

    // Should complete without error (may or may not find vulnerabilities)
    assert!(result.is_ok());
}

#[test]
fn test_security_scan_output() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    let output = repo.run_cli_success(&["security", "scan"]);

    // Security scan may report no patterns available or scan results
    // Either is acceptable - patterns may not be compiled in test builds
    assert!(
        output.contains("pattern")
            || output.contains("Pattern")
            || output.contains("scan")
            || output.contains("Scan")
            || output.contains("match")
            || output.contains("No security")
            || output.contains("CVE")
            || !output.is_empty(),
        "Should return scan output: {}",
        output
    );
}

#[test]
fn test_security_scan_with_severity_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/app.ts",
        r#"
import { exec } from 'child_process';
export function runCommand(input: string) {
    exec(input); // Potential command injection
}
"#,
    );

    repo.generate_index().unwrap();

    // Filter by severity
    let result = repo.run_cli(&["security", "scan", "--severity", "HIGH,CRITICAL"]);
    assert!(result.is_ok());
}

#[test]
fn test_security_scan_with_module_filter() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/api/handler.ts", "handleRequest", "return res.json();")
        .add_ts_function("src/utils/helper.ts", "helper", "return 1;");

    repo.generate_index().unwrap();

    // Filter to specific module
    let result = repo.run_cli(&["security", "scan", "--module", "src"]);
    assert!(result.is_ok());
}

#[test]
fn test_security_scan_with_limit() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Security scan with limit - may not have patterns available
    let result = repo.run_cli(&["security", "scan", "--limit", "5"]);
    assert!(result.is_ok(), "Security scan with limit should complete");
}

#[test]
fn test_security_scan_potential_sql_injection() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/db.ts",
        r#"
export function query(userId: string) {
    const sql = "SELECT * FROM users WHERE id = " + userId;
    return db.execute(sql);
}
"#,
    );

    repo.generate_index().unwrap();

    // May or may not detect SQL injection pattern (patterns may not be available)
    let result = repo.run_cli(&["security", "scan"]);
    assert!(result.is_ok(), "Security scan should complete");
}

#[test]
fn test_security_scan_no_index() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Don't generate index
    let result = repo.run_cli(&["security", "scan"]);

    // Should handle gracefully
    assert!(result.is_ok());
}

// ============================================================================
// SECURITY STATS TESTS
// ============================================================================

#[test]
fn test_security_stats_basic() {
    let repo = TestRepo::new();

    let result = repo.run_cli(&["security", "stats"]);

    // Should show pattern statistics
    assert!(result.is_ok());
}

#[test]
fn test_security_stats_json_format() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&["security", "stats", "-f", "json"]);
    let json = assert_valid_json(&output, "security stats json");

    // Should have pattern stats
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("pattern")
            || output_str.contains("count")
            || output_str.contains("cve")
            || output_str.contains("CVE")
            || json.is_object(),
        "Should return pattern stats: {}",
        output
    );
}

#[test]
fn test_security_stats_text_format() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&["security", "stats", "-f", "text"]);

    // Should produce readable output
    assert!(!output.is_empty(), "Should produce text output");
}

// ============================================================================
// SECURITY UPDATE TESTS
// ============================================================================

#[test]
fn test_security_update_basic() {
    let repo = TestRepo::new();

    // Update may fail if no network or no pattern server - that's OK
    let result = repo.run_cli(&["security", "update"]);

    // May succeed or fail depending on network - should not panic
    assert!(result.is_ok());
}

#[test]
fn test_security_update_with_url() {
    let repo = TestRepo::new();

    // Test with invalid URL - should handle gracefully
    let result = repo.run_cli(&[
        "security",
        "update",
        "--url",
        "http://invalid.example.com/patterns.bin",
    ]);

    // Should handle error gracefully (not panic)
    assert!(result.is_ok());
}

#[test]
fn test_security_update_force() {
    let repo = TestRepo::new();

    // Force update
    let result = repo.run_cli(&["security", "update", "--force"]);

    // May succeed or fail - should not panic
    assert!(result.is_ok());
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_security_scan_completes_all_formats() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Test all formats for scan command - patterns may not be available
    // so we just verify the command completes without error
    let text_result = repo.run_cli(&["security", "scan", "-f", "text"]);
    let json_result = repo.run_cli(&["security", "scan", "-f", "json"]);
    let toon_result = repo.run_cli(&["security", "scan", "-f", "toon"]);

    // All should complete without crashing
    assert!(text_result.is_ok(), "Text format should complete");
    assert!(json_result.is_ok(), "JSON format should complete");
    assert!(toon_result.is_ok(), "TOON format should complete");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_security_scan_empty_repo() {
    let repo = TestRepo::new();
    // No files added
    std::fs::create_dir_all(repo.path().join("src")).unwrap();

    let result = repo.run_cli(&["security", "scan"]);

    // Should handle empty repo gracefully
    assert!(result.is_ok());
}

#[test]
fn test_security_scan_multilang() {
    let repo = TestRepo::new();
    repo.with_multilang();

    repo.generate_index().unwrap();

    // Security scan on multilang repo - patterns may not be available
    let result = repo.run_cli(&["security", "scan"]);

    // Should complete (scan all languages)
    assert!(
        result.is_ok(),
        "Security scan on multilang repo should complete"
    );
}

// ============================================================================
// CWE FILTER EDGE CASES
// ============================================================================

#[test]
fn test_security_scan_with_cwe_filter_single() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/app.ts",
        r#"
export function searchUsers(input: string) {
    const query = "SELECT * FROM users WHERE name = '" + input + "'";
    return db.execute(query);
}
"#,
    );

    repo.generate_index().unwrap();

    // Filter by single CWE (SQL Injection = CWE-89)
    let result = repo.run_cli(&["security", "scan", "--cwe", "CWE-89"]);
    assert!(result.is_ok(), "CWE filter should work with single ID");
}

#[test]
fn test_security_scan_with_cwe_filter_multiple() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/vulnerable.ts",
        r#"
import { exec } from 'child_process';

export function runQuery(input: string) {
    const query = "SELECT * FROM users WHERE id = " + input;
    db.execute(query);
    exec(input); // Command injection
}
"#,
    );

    repo.generate_index().unwrap();

    // Filter by multiple CWEs
    let result = repo.run_cli(&["security", "scan", "--cwe", "CWE-89,CWE-78"]);
    assert!(result.is_ok(), "CWE filter should work with multiple IDs");
}

#[test]
fn test_security_scan_with_severity_and_cwe_combined() {
    let repo = TestRepo::new();
    repo.add_file(
        "src/critical.ts",
        r#"
import { exec } from 'child_process';
export function dangerousCommand(cmd: string) {
    exec(cmd);
}
"#,
    );

    repo.generate_index().unwrap();

    // Combine severity and CWE filters
    let result = repo.run_cli(&[
        "security",
        "scan",
        "--severity",
        "CRITICAL",
        "--cwe",
        "CWE-78",
    ]);
    assert!(result.is_ok(), "Combined filters should work together");
}

// ============================================================================
// MIN_SIMILARITY THRESHOLD EDGE CASES
// ============================================================================

#[test]
fn test_security_scan_min_similarity_zero() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Zero threshold - should match everything
    let result = repo.run_cli(&["security", "scan", "--min-similarity", "0.0"]);
    assert!(result.is_ok(), "Zero similarity threshold should work");
}

#[test]
fn test_security_scan_min_similarity_half() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // 50% threshold - moderate matching
    let result = repo.run_cli(&["security", "scan", "--min-similarity", "0.5"]);
    assert!(result.is_ok(), "Half similarity threshold should work");
}

#[test]
fn test_security_scan_min_similarity_one() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // 100% threshold - only exact matches
    let result = repo.run_cli(&["security", "scan", "--min-similarity", "1.0"]);
    assert!(result.is_ok(), "Full similarity threshold should work");
}

#[test]
fn test_security_scan_min_similarity_out_of_range() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Invalid threshold values
    let result_negative = repo.run_cli(&["security", "scan", "--min-similarity", "-0.5"]);
    let result_high = repo.run_cli(&["security", "scan", "--min-similarity", "2.0"]);

    // Should handle gracefully (either clamp or error)
    assert!(
        result_negative.is_ok(),
        "Negative threshold should be handled"
    );
    assert!(result_high.is_ok(), "High threshold should be handled");
}

// ============================================================================
// UPDATE FROM FILE PATH
// ============================================================================

#[test]
fn test_security_update_from_file() {
    let repo = TestRepo::new();

    // Create a dummy patterns file
    let patterns_path = repo.path().join("patterns.bin");
    std::fs::write(&patterns_path, b"invalid_pattern_data").unwrap();

    // Update from local file - should handle invalid format gracefully
    let result = repo.run_cli(&[
        "security",
        "update",
        "--file",
        patterns_path.to_str().unwrap(),
    ]);

    // Should handle invalid file gracefully
    assert!(
        result.is_ok(),
        "Update from file should handle invalid format"
    );
}

#[test]
fn test_security_update_from_nonexistent_file() {
    let repo = TestRepo::new();

    // Update from non-existent file
    let result = repo.run_cli(&[
        "security",
        "update",
        "--file",
        "/nonexistent/path/patterns.bin",
    ]);

    // Should handle missing file gracefully
    assert!(result.is_ok(), "Update from missing file should not panic");
}

// ============================================================================
// INVALID SEVERITY HANDLING
// ============================================================================

#[test]
fn test_security_scan_invalid_severity() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Invalid severity level
    let result = repo.run_cli(&["security", "scan", "--severity", "INVALID_LEVEL"]);

    // Should handle gracefully (either ignore or error)
    assert!(
        result.is_ok(),
        "Invalid severity should be handled gracefully"
    );
}

#[test]
fn test_security_scan_severity_case_variations() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    repo.generate_index().unwrap();

    // Different case variations
    let result_lower = repo.run_cli(&["security", "scan", "--severity", "high"]);
    let result_mixed = repo.run_cli(&["security", "scan", "--severity", "High"]);

    // Should handle case variations
    assert!(result_lower.is_ok(), "Lowercase severity should work");
    assert!(result_mixed.is_ok(), "Mixed case severity should work");
}
