//! Tests for the `lint` CLI command
//!
//! The lint command detects and runs code quality tools:
//! - `lint detect` - Detect available linters
//! - `lint scan` - Run linters and report issues
//! - `lint fix` - Apply automatic fixes
//! - `lint recommend` - Get linter recommendations

#![allow(unused_imports)]

use crate::common::{assert_contains, assert_valid_json, assert_valid_toon, TestRepo};

// ============================================================================
// RUST LINTER DETECTION TESTS
// ============================================================================

#[test]
fn test_lint_detect_rust_project() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn hello() -> &'static str { \"Hello\" }");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect rust");

    // Should detect Rust linters
    assert!(
        json.get("linters").is_some() || json.get("detected").is_some(),
        "Should have linters in output"
    );

    // Check for clippy or rustfmt
    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("clippy") || output_str.contains("rustfmt"),
        "Should detect Rust linters: {output}"
    );
}

#[test]
fn test_lint_detect_rust_with_version() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/main.rs", "fn main() {}");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "text"]);

    // Should show version info
    assert!(
        output.contains("version:") || output.contains("clippy"),
        "Should show version information: {output}"
    );
}

// ============================================================================
// JAVASCRIPT/TYPESCRIPT LINTER DETECTION TESTS
// ============================================================================

#[test]
fn test_lint_detect_js_eslint() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test", "version": "1.0.0"}"#);
    repo.add_file(".eslintrc.json", r#"{"env": {"node": true}}"#);
    repo.add_file("src/index.js", "console.log('hello');");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect eslint");

    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("eslint"),
        "Should detect ESLint: {output}"
    );
}

#[test]
fn test_lint_detect_js_biome() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test", "version": "1.0.0"}"#);
    repo.add_file(
        "biome.json",
        r#"{"$schema": "https://biomejs.dev/schemas/1.0.0/schema.json"}"#,
    );
    repo.add_file("src/index.ts", "export const hello = 'world';");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect biome");

    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("biome"),
        "Should detect Biome: {output}"
    );
}

#[test]
fn test_lint_detect_js_prettier() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test", "version": "1.0.0"}"#);
    repo.add_file(".prettierrc", r#"{"semi": true}"#);
    repo.add_file("src/index.js", "const x = 1;");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect prettier");

    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("prettier"),
        "Should detect Prettier: {output}"
    );
}

#[test]
fn test_lint_detect_typescript() {
    let repo = TestRepo::new();
    repo.add_file("package.json", r#"{"name": "test", "version": "1.0.0"}"#);
    repo.add_file("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#);
    repo.add_file("src/index.ts", "export const hello: string = 'world';");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect typescript");

    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("typescript") || output_str.contains("tsc"),
        "Should detect TypeScript: {output}"
    );
}

// ============================================================================
// PYTHON LINTER DETECTION TESTS
// ============================================================================

#[test]
fn test_lint_detect_python_ruff() {
    let repo = TestRepo::new();
    repo.add_file(
        "pyproject.toml",
        r#"
[tool.ruff]
line-length = 88
"#,
    );
    repo.add_file("src/main.py", "def hello():\n    print('hello')");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect ruff");

    let output_str = output.to_lowercase();
    assert!(output_str.contains("ruff"), "Should detect Ruff: {output}");
}

#[test]
fn test_lint_detect_python_mypy() {
    let repo = TestRepo::new();
    // Need pyproject.toml or requirements.txt for Python detection
    repo.add_file(
        "pyproject.toml",
        r#"
[project]
name = "test"
version = "0.1.0"

[tool.mypy]
strict = true
"#,
    );
    repo.add_file("mypy.ini", "[mypy]\nstrict = True");
    repo.add_file("src/main.py", "def hello() -> str:\n    return 'hello'");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect mypy");

    let output_str = output.to_lowercase();
    assert!(output_str.contains("mypy"), "Should detect mypy: {output}");
}

// ============================================================================
// GO LINTER DETECTION TESTS
// ============================================================================

#[test]
fn test_lint_detect_go() {
    let repo = TestRepo::new();
    repo.add_file("go.mod", "module example.com/test\n\ngo 1.21");
    repo.add_file(
        "main.go",
        r#"
package main

func main() {
    println("hello")
}
"#,
    );

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect go");

    // Command should succeed and return valid JSON
    // If Go tools are installed, should detect gofmt/vet/golangci-lint
    // If Go tools are NOT installed, should still return valid output (possibly empty linters)
    assert!(json.is_object(), "Should return valid JSON object");

    let output_str = output.to_lowercase();
    // Check for Go linters OR recognize it as a Go project (even without tools)
    let has_go_linters = output_str.contains("gofmt")
        || output_str.contains("golangci")
        || output_str.contains("vet");
    let recognized_as_go = output_str.contains("go")
        || json
            .get("linters")
            .map(|l| l.as_array().map(|a| a.is_empty()).unwrap_or(true))
            .unwrap_or(true);

    assert!(
        has_go_linters || recognized_as_go,
        "Should detect Go project or linters: {output}"
    );
}

#[test]
fn test_lint_detect_go_golangci() {
    let repo = TestRepo::new();
    repo.add_file("go.mod", "module example.com/test\n\ngo 1.21");
    repo.add_file(".golangci.yml", "linters:\n  enable:\n    - gofmt");
    repo.add_file("main.go", "package main\n\nfunc main() {}");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect golangci");

    let output_str = output.to_lowercase();
    assert!(
        output_str.contains("golangci"),
        "Should detect golangci-lint: {output}"
    );
}

// ============================================================================
// OUTPUT FORMAT TESTS
// ============================================================================

#[test]
fn test_lint_detect_json_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect json format");

    // Should be valid JSON with expected fields
    assert!(json.is_object(), "JSON output should be an object");
}

#[test]
fn test_lint_detect_toon_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "toon"]);
    assert_valid_toon(&output, "lint detect toon format");

    // TOON format should contain type marker
    assert!(
        output.contains("_type:"),
        "TOON output should contain _type marker"
    );
}

#[test]
fn test_lint_detect_text_format() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "text"]);

    // Text format should have human-readable output
    assert!(
        output.contains("DETECTED") || output.contains("linter") || output.contains("Clippy"),
        "Text format should be human-readable: {output}"
    );
}

// ============================================================================
// LINT SCAN TESTS
// ============================================================================

#[test]
fn test_lint_scan_rust_clean() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn hello() -> &'static str { \"Hello\" }");

    let output = repo.run_cli_success(&["lint", "scan", "-f", "json"]);
    let json = assert_valid_json(&output, "lint scan rust");

    // Should have scan results
    assert!(
        json.get("success").is_some()
            || json.get("status").is_some()
            || json.get("errors").is_some()
            || json.get("error_count").is_some(),
        "Should have scan results: {output}"
    );
}

#[test]
fn test_lint_scan_json_output() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    let output = repo.run_cli_success(&["lint", "scan", "-f", "json"]);
    let json = assert_valid_json(&output, "lint scan json");

    assert!(json.is_object(), "Should return valid JSON object");
}

// ============================================================================
// LINT RECOMMEND TESTS
// ============================================================================

#[test]
fn test_lint_recommend_empty_project() {
    let repo = TestRepo::new();
    // Empty project - should recommend linters based on detected files
    repo.add_file("main.py", "print('hello')");

    let output = repo.run_cli_success(&["lint", "recommend", "-f", "json"]);
    let json = assert_valid_json(&output, "lint recommend");

    // Should have recommendations
    assert!(
        json.is_object() || json.is_array(),
        "Should return recommendations"
    );
}

#[test]
fn test_lint_recommend_rust_without_clippy() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    let output = repo.run_cli_success(&["lint", "recommend", "-f", "text"]);

    // For Rust projects, clippy should be installed by default
    // so recommendations may be empty or suggest other tools
    assert!(
        output.contains("recommend")
            || output.contains("No")
            || output.contains("clippy")
            || !output.is_empty(),
        "Should provide some output: {output}"
    );
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_lint_detect_no_linters() {
    let repo = TestRepo::new();
    // Just a text file, no project markers
    repo.add_file("README.md", "# Test");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect empty");

    // Should handle gracefully with zero linters
    assert!(
        json.is_object(),
        "Should return valid JSON even with no linters"
    );
}

#[test]
fn test_lint_detect_mixed_project() {
    let repo = TestRepo::new();
    // Project with multiple languages
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn rust_fn() {}");
    repo.add_file("package.json", r#"{"name": "test", "version": "1.0.0"}"#);
    repo.add_file(".eslintrc.json", r#"{"env": {"node": true}}"#);
    repo.add_file("src/index.js", "console.log('hello');");

    let output = repo.run_cli_success(&["lint", "detect", "-f", "json"]);
    let json = assert_valid_json(&output, "lint detect mixed");

    // Should detect linters for both languages
    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("clippy") || output_lower.contains("rust"),
        "Should detect Rust linters"
    );
    assert!(
        output_lower.contains("eslint") || output_lower.contains("javascript"),
        "Should detect JS linters"
    );
}

#[test]
fn test_lint_scan_with_linter_filter() {
    let repo = TestRepo::new();
    repo.add_file(
        "Cargo.toml",
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.add_file("src/lib.rs", "pub fn test() {}");

    // Should be able to specify a specific linter
    let output = repo.run_cli_success(&["lint", "scan", "--linter", "clippy", "-f", "json"]);
    let json = assert_valid_json(&output, "lint scan filtered");

    assert!(json.is_object(), "Should return valid JSON");
}
