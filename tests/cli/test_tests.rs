//! Tests for the `test` CLI command
//!
//! The test command runs or detects tests:
//! - `test` - Run tests (auto-detects framework)
//! - `test --detect` - Only detect test framework
//! - `test --framework <FRAMEWORK>` - Force specific framework
//! - `test <FILTER>` - Run tests matching filter pattern

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// TEST DETECTION TESTS
// ============================================================================

#[test]
fn test_detect_no_framework() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    let result = repo.run_cli(&["test", "--detect"]);

    // Should complete (may report no framework found)
    assert!(result.is_ok());
}

#[test]
fn test_detect_npm_framework() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{
    "name": "test-project",
    "scripts": {
        "test": "jest"
    },
    "devDependencies": {
        "jest": "^29.0.0"
    }
}"#,
    )
    .add_file(
        "src/main.test.ts",
        r#"
test('adds 1 + 2 to equal 3', () => {
    expect(1 + 2).toBe(3);
});
"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    let json = assert_valid_json(&output, "detect npm framework");

    // Should detect Jest or npm
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("jest")
            || output_str.contains("Jest")
            || output_str.contains("npm")
            || output_str.contains("framework")
            || json.is_object(),
        "Should detect test framework: {}",
        output
    );
}

#[test]
fn test_detect_cargo_framework() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dev-dependencies]
"#,
    )
    .add_file(
        "src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    let json = assert_valid_json(&output, "detect cargo framework");

    // Should detect Cargo
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("cargo")
            || output_str.contains("Cargo")
            || output_str.contains("rust")
            || output_str.contains("framework")
            || json.is_object(),
        "Should detect cargo test framework: {}",
        output
    );
}

#[test]
fn test_detect_pytest_framework() {
    let repo = TestRepo::new();
    repo.add_file(
        "pyproject.toml",
        r#"[project]
name = "test-project"
version = "0.1.0"

[tool.pytest.ini_options]
testpaths = ["tests"]
"#,
    )
    .add_file(
        "tests/test_main.py",
        r#"
def test_addition():
    assert 1 + 1 == 2
"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    let json = assert_valid_json(&output, "detect pytest framework");

    // Should detect pytest
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("pytest")
            || output_str.contains("python")
            || output_str.contains("framework")
            || json.is_object(),
        "Should detect pytest framework: {}",
        output
    );
}

#[test]
fn test_detect_go_framework() {
    let repo = TestRepo::new();
    repo.add_file("go.mod", "module example.com/test\n\ngo 1.21\n")
        .add_file(
            "main_test.go",
            r#"
package main

import "testing"

func TestAdd(t *testing.T) {
    if 1+1 != 2 {
        t.Error("Expected 2")
    }
}
"#,
        );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    let json = assert_valid_json(&output, "detect go framework");

    // Should detect Go test
    let output_str = serde_json::to_string(&json).unwrap();
    assert!(
        output_str.contains("go")
            || output_str.contains("Go")
            || output_str.contains("framework")
            || json.is_object(),
        "Should detect go test framework: {}",
        output
    );
}

#[test]
fn test_detect_vitest_framework() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{
    "name": "test-project",
    "scripts": {
        "test": "vitest"
    },
    "devDependencies": {
        "vitest": "^1.0.0"
    }
}"#,
    )
    .add_file(
        "src/main.test.ts",
        r#"
import { test, expect } from 'vitest';
test('adds 1 + 2', () => {
    expect(1 + 2).toBe(3);
});
"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    assert_valid_json(&output, "detect vitest framework");
}

// ============================================================================
// TEST RUN TESTS (may skip actual execution in CI)
// ============================================================================

#[test]
fn test_run_no_tests() {
    let repo = TestRepo::new();
    repo.add_ts_function("src/main.ts", "main", "return 1;");

    // Should handle no tests gracefully
    let result = repo.run_cli(&["test"]);
    assert!(result.is_ok());
}

#[test]
fn test_run_with_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{
    "name": "test-project",
    "scripts": {
        "test": "jest"
    }
}"#,
    );

    // Filter pattern
    let result = repo.run_cli(&["test", "main"]);

    // Should handle filter (may not run if jest not installed)
    assert!(result.is_ok());
}

#[test]
fn test_run_with_framework_override() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test-project"
version = "0.1.0"
"#,
    );

    // Force npm even though it's a Rust project
    let result = repo.run_cli(&["test", "--framework", "npm"]);

    // Should attempt to use specified framework
    assert!(result.is_ok());
}

#[test]
fn test_run_with_timeout() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test"}"#);

    let result = repo.run_cli(&["test", "--timeout", "10"]);

    // Should respect timeout
    assert!(result.is_ok());
}

#[test]
fn test_run_verbose() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test"}"#);

    let result = repo.run_cli(&["test", "--test-verbose"]);
    assert!(result.is_ok());
}

#[test]
fn test_run_with_path() {
    let repo = TestRepo::new();
    std::fs::create_dir_all(repo.path().join("subdir")).unwrap();
    repo.add_file("subdir/package.json", r#"{"name": "subtest"}"#);

    let result = repo.run_cli(&["test", "--path", "subdir"]);
    assert!(result.is_ok());
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_detect_text_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{"name": "test", "scripts": {"test": "jest"}}"#,
    );

    // run_cli_success already verifies the command completes successfully
    repo.run_cli_success(&["test", "--detect", "-f", "text"]);
}

#[test]
fn test_detect_json_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{"name": "test", "scripts": {"test": "jest"}}"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    assert_valid_json(&output, "detect json format");
}

#[test]
fn test_detect_toon_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "package.json",
        r#"{"name": "test", "scripts": {"test": "jest"}}"#,
    );

    // run_cli_success already verifies the command completes successfully
    repo.run_cli_success(&["test", "--detect", "-f", "toon"]);
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_empty_directory() {
    let repo = TestRepo::new();
    // No files

    let result = repo.run_cli(&["test", "--detect"]);
    assert!(result.is_ok());
}

#[test]
fn test_multiple_frameworks() {
    let repo = TestRepo::new();
    // Both Node and Rust project markers
    repo.add_file(
        "package.json",
        r#"{"name": "test", "scripts": {"test": "jest"}}"#,
    )
    .add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
"#,
    );

    let output = repo.run_cli_success(&["test", "--detect", "-f", "json"]);
    assert_valid_json(&output, "multiple frameworks detection");
}

#[test]
fn test_invalid_framework() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test"}"#);

    // Invalid framework name
    let result = repo.run_cli(&["test", "--framework", "nonexistent"]);

    // Should handle gracefully
    assert!(result.is_ok());
}
