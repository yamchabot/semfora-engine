//! Multi-language test runner for the VALIDATE phase.
//!
//! Detects test frameworks and runs tests, returning structured results
//! that can be used by the ADK validation loop.
//!
//! Supported frameworks:
//! - Python: pytest
//! - Rust: cargo test
//! - JavaScript/TypeScript: npm test, vitest, jest
//! - Go: go test

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::McpDiffError;

// ============================================================================
// Types
// ============================================================================

/// Detected test framework
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestFramework {
    Pytest,
    Cargo,
    Npm,
    Vitest,
    Jest,
    Go,
    Unknown,
}

impl TestFramework {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestFramework::Pytest => "pytest",
            TestFramework::Cargo => "cargo",
            TestFramework::Npm => "npm",
            TestFramework::Vitest => "vitest",
            TestFramework::Jest => "jest",
            TestFramework::Go => "go",
            TestFramework::Unknown => "unknown",
        }
    }
}

/// Result of running tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    /// Test framework used
    pub framework: TestFramework,

    /// Whether tests passed overall
    pub success: bool,

    /// Number of tests passed
    pub passed: usize,

    /// Number of tests failed
    pub failed: usize,

    /// Number of tests skipped
    pub skipped: usize,

    /// Total number of tests
    pub total: usize,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Individual test failures with details
    pub failures: Vec<TestFailure>,

    /// Raw stdout from test run
    pub stdout: String,

    /// Raw stderr from test run
    pub stderr: String,

    /// Exit code from test command
    pub exit_code: Option<i32>,
}

impl Default for TestResults {
    fn default() -> Self {
        Self {
            framework: TestFramework::Unknown,
            success: false,
            passed: 0,
            failed: 0,
            skipped: 0,
            total: 0,
            duration_ms: 0,
            failures: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        }
    }
}

/// Details about a single test failure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    /// Test name/path
    pub name: String,

    /// File where test is located (if known)
    pub file: Option<String>,

    /// Line number (if known)
    pub line: Option<usize>,

    /// Error message
    pub message: String,

    /// Stack trace or additional context
    pub traceback: Option<String>,
}

/// Options for running tests
#[derive(Debug, Clone, Default)]
pub struct TestRunOptions {
    /// Only run tests matching this filter
    pub filter: Option<String>,

    /// Maximum time to run tests (seconds)
    pub timeout_secs: Option<u64>,

    /// Run tests in verbose mode
    pub verbose: bool,

    /// Additional arguments to pass to test command
    pub extra_args: Vec<String>,
}

// ============================================================================
// Framework Detection
// ============================================================================

/// Detect the test framework for a project directory
pub fn detect_framework(dir: &Path) -> TestFramework {
    // Check for Rust (Cargo.toml)
    if dir.join("Cargo.toml").exists() {
        return TestFramework::Cargo;
    }

    // Check for Go (go.mod)
    if dir.join("go.mod").exists() {
        return TestFramework::Go;
    }

    // Check for Python (pytest)
    if dir.join("pytest.ini").exists()
        || dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("conftest.py").exists()
    {
        return TestFramework::Pytest;
    }

    // Check for Node.js
    if dir.join("package.json").exists() {
        // Try to detect specific test runner
        if let Ok(content) = std::fs::read_to_string(dir.join("package.json")) {
            if content.contains("vitest") {
                return TestFramework::Vitest;
            }
            if content.contains("jest") {
                return TestFramework::Jest;
            }
            // Default to npm test
            return TestFramework::Npm;
        }
    }

    TestFramework::Unknown
}

/// Detect all test frameworks in a directory (for monorepos)
pub fn detect_all_frameworks(dir: &Path) -> Vec<(TestFramework, std::path::PathBuf)> {
    let mut frameworks = Vec::new();

    // Check root
    let root_framework = detect_framework(dir);
    if root_framework != TestFramework::Unknown {
        frameworks.push((root_framework, dir.to_path_buf()));
    }

    // Check immediate subdirectories for monorepo setups
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Skip common non-project directories
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                    || name == "build"
                    || name == "__pycache__"
                    || name == ".venv"
                    || name == "venv"
                {
                    continue;
                }

                let sub_framework = detect_framework(&path);
                if sub_framework != TestFramework::Unknown {
                    frameworks.push((sub_framework, path));
                }
            }
        }
    }

    frameworks
}

// ============================================================================
// Test Running
// ============================================================================

/// Run tests for a project
pub fn run_tests(dir: &Path, options: &TestRunOptions) -> Result<TestResults> {
    let framework = detect_framework(dir);
    run_tests_with_framework(dir, framework, options)
}

/// Run tests with a specific framework
pub fn run_tests_with_framework(
    dir: &Path,
    framework: TestFramework,
    options: &TestRunOptions,
) -> Result<TestResults> {
    match framework {
        TestFramework::Pytest => run_pytest(dir, options),
        TestFramework::Cargo => run_cargo_test(dir, options),
        TestFramework::Npm | TestFramework::Vitest | TestFramework::Jest => {
            run_npm_test(dir, options)
        }
        TestFramework::Go => run_go_test(dir, options),
        TestFramework::Unknown => Err(McpDiffError::ExtractionFailure {
            message: "No test framework detected".to_string(),
        }),
    }
}

/// Run pytest
fn run_pytest(dir: &Path, options: &TestRunOptions) -> Result<TestResults> {
    let start = Instant::now();

    let mut cmd = Command::new("python3");
    cmd.arg("-m").arg("pytest");

    // JSON output for structured results
    cmd.arg("--tb=short");
    cmd.arg("-q");

    if options.verbose {
        cmd.arg("-v");
    }

    if let Some(ref filter) = options.filter {
        cmd.arg("-k").arg(filter);
    }

    for arg in &options.extra_args {
        cmd.arg(arg);
    }

    cmd.current_dir(dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::ExtractionFailure {
        message: format!("Failed to run pytest: {}", e),
    })?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut results = parse_pytest_output(&stdout, &stderr);
    results.framework = TestFramework::Pytest;
    results.duration_ms = duration.as_millis() as u64;
    results.exit_code = output.status.code();
    results.success = output.status.success();
    results.stdout = stdout;
    results.stderr = stderr;

    Ok(results)
}

/// Run cargo test
fn run_cargo_test(dir: &Path, options: &TestRunOptions) -> Result<TestResults> {
    let start = Instant::now();

    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if let Some(ref filter) = options.filter {
        cmd.arg(filter);
    }

    // Pass -- to separate cargo args from test binary args
    cmd.arg("--");

    if !options.verbose {
        cmd.arg("--quiet");
    }

    for arg in &options.extra_args {
        cmd.arg(arg);
    }

    cmd.current_dir(dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::ExtractionFailure {
        message: format!("Failed to run cargo test: {}", e),
    })?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut results = parse_cargo_test_output(&stdout, &stderr);
    results.framework = TestFramework::Cargo;
    results.duration_ms = duration.as_millis() as u64;
    results.exit_code = output.status.code();
    results.success = output.status.success();
    results.stdout = stdout;
    results.stderr = stderr;

    Ok(results)
}

/// Run npm test (or vitest/jest)
fn run_npm_test(dir: &Path, options: &TestRunOptions) -> Result<TestResults> {
    let start = Instant::now();

    let mut cmd = Command::new("npm");
    cmd.arg("test");
    cmd.arg("--");

    if let Some(ref filter) = options.filter {
        cmd.arg(filter);
    }

    for arg in &options.extra_args {
        cmd.arg(arg);
    }

    cmd.current_dir(dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::ExtractionFailure {
        message: format!("Failed to run npm test: {}", e),
    })?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut results = parse_npm_test_output(&stdout, &stderr);
    results.framework = TestFramework::Npm;
    results.duration_ms = duration.as_millis() as u64;
    results.exit_code = output.status.code();
    results.success = output.status.success();
    results.stdout = stdout;
    results.stderr = stderr;

    Ok(results)
}

/// Run go test
fn run_go_test(dir: &Path, options: &TestRunOptions) -> Result<TestResults> {
    let start = Instant::now();

    let mut cmd = Command::new("go");
    cmd.arg("test");
    cmd.arg("./...");

    if options.verbose {
        cmd.arg("-v");
    }

    if let Some(ref filter) = options.filter {
        cmd.arg("-run").arg(filter);
    }

    for arg in &options.extra_args {
        cmd.arg(arg);
    }

    cmd.current_dir(dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::ExtractionFailure {
        message: format!("Failed to run go test: {}", e),
    })?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut results = parse_go_test_output(&stdout, &stderr);
    results.framework = TestFramework::Go;
    results.duration_ms = duration.as_millis() as u64;
    results.exit_code = output.status.code();
    results.success = output.status.success();
    results.stdout = stdout;
    results.stderr = stderr;

    Ok(results)
}

// ============================================================================
// Output Parsing
// ============================================================================

/// Parse pytest output
fn parse_pytest_output(stdout: &str, _stderr: &str) -> TestResults {
    let mut results = TestResults::default();

    // Look for summary line: "X passed, Y failed, Z skipped in Ns"
    // Or: "X passed in Ns"
    for line in stdout.lines().rev() {
        let line = line.trim();

        // Match patterns like "5 passed, 2 failed in 1.23s"
        if line.contains(" passed") || line.contains(" failed") {
            // Extract numbers
            let parts: Vec<&str> = line.split_whitespace().collect();

            for (i, part) in parts.iter().enumerate() {
                if *part == "passed" || part.starts_with("passed,") {
                    if i > 0 {
                        results.passed = parts[i - 1].parse().unwrap_or(0);
                    }
                } else if *part == "failed" || part.starts_with("failed,") {
                    if i > 0 {
                        results.failed = parts[i - 1].parse().unwrap_or(0);
                    }
                } else if *part == "skipped" || part.starts_with("skipped,") {
                    if i > 0 {
                        results.skipped = parts[i - 1].parse().unwrap_or(0);
                    }
                } else if *part == "error" || part.starts_with("error,") || *part == "errors" {
                    if i > 0 {
                        results.failed += parts[i - 1].parse().unwrap_or(0);
                    }
                }
            }

            break;
        }
    }

    results.total = results.passed + results.failed + results.skipped;

    // Parse failures - look for FAILED test names
    let mut current_failure: Option<TestFailure> = None;

    for line in stdout.lines() {
        if line.starts_with("FAILED ") || line.starts_with("ERROR ") {
            // Save previous failure
            if let Some(failure) = current_failure.take() {
                results.failures.push(failure);
            }

            // Parse test name and file
            let test_part = line
                .trim_start_matches("FAILED ")
                .trim_start_matches("ERROR ");
            let (name, file) = if test_part.contains("::") {
                let parts: Vec<&str> = test_part.split("::").collect();
                if parts.len() >= 2 {
                    (parts[1..].join("::"), Some(parts[0].to_string()))
                } else {
                    (test_part.to_string(), None)
                }
            } else {
                (test_part.to_string(), None)
            };

            current_failure = Some(TestFailure {
                name,
                file,
                line: None,
                message: String::new(),
                traceback: None,
            });
        } else if line.starts_with("E ") {
            // Error message line
            if let Some(ref mut failure) = current_failure {
                let msg = line.trim_start_matches("E ").trim();
                if failure.message.is_empty() {
                    failure.message = msg.to_string();
                } else {
                    failure.message.push_str("\n");
                    failure.message.push_str(msg);
                }
            }
        }
    }

    // Don't forget the last failure
    if let Some(failure) = current_failure {
        results.failures.push(failure);
    }

    results
}

/// Parse cargo test output
fn parse_cargo_test_output(stdout: &str, stderr: &str) -> TestResults {
    let mut results = TestResults::default();
    let combined = format!("{}\n{}", stdout, stderr);

    // Look for "test result: ok/FAILED. X passed; Y failed; Z ignored"
    for line in combined.lines() {
        if line.starts_with("test result:") {
            let parts: Vec<&str> = line.split_whitespace().collect();

            for (i, part) in parts.iter().enumerate() {
                if *part == "passed;" {
                    if i > 0 {
                        results.passed = parts[i - 1].parse().unwrap_or(0);
                    }
                } else if *part == "failed;" {
                    if i > 0 {
                        results.failed = parts[i - 1].parse().unwrap_or(0);
                    }
                } else if *part == "ignored;" || *part == "ignored" {
                    if i > 0 {
                        results.skipped = parts[i - 1].parse().unwrap_or(0);
                    }
                }
            }

            break;
        }
    }

    results.total = results.passed + results.failed + results.skipped;

    // Parse individual failures
    let mut in_failure = false;
    let mut failure_name = String::new();
    let mut failure_message = String::new();

    for line in combined.lines() {
        if line.starts_with("---- ") && line.ends_with(" ----") {
            // Save previous failure
            if !failure_name.is_empty() {
                results.failures.push(TestFailure {
                    name: failure_name.clone(),
                    file: None,
                    line: None,
                    message: failure_message.trim().to_string(),
                    traceback: None,
                });
            }

            failure_name = line
                .trim_start_matches("---- ")
                .trim_end_matches(" ----")
                .trim_end_matches(" stdout")
                .to_string();
            failure_message.clear();
            in_failure = true;
        } else if in_failure && !line.is_empty() {
            failure_message.push_str(line);
            failure_message.push('\n');
        }
    }

    // Don't forget the last failure
    if !failure_name.is_empty() && !failure_message.is_empty() {
        results.failures.push(TestFailure {
            name: failure_name,
            file: None,
            line: None,
            message: failure_message.trim().to_string(),
            traceback: None,
        });
    }

    results
}

/// Parse npm/vitest/jest test output
fn parse_npm_test_output(stdout: &str, stderr: &str) -> TestResults {
    let mut results = TestResults::default();
    let combined = format!("{}\n{}", stdout, stderr);

    // Try to find summary lines
    // Jest: "Tests: X passed, Y failed, Z total"
    // Vitest: "Tests  X passed | Y failed | Z total"
    for line in combined.lines() {
        let line_lower = line.to_lowercase();

        if line_lower.contains("tests")
            && (line_lower.contains("passed") || line_lower.contains("failed"))
        {
            // Extract numbers by looking for patterns
            let parts: Vec<&str> = line.split(|c: char| !c.is_numeric()).collect();
            let numbers: Vec<usize> = parts.iter().filter_map(|p| p.parse().ok()).collect();

            if !numbers.is_empty() {
                // Usually format is: passed, failed, total
                if line_lower.contains("passed") {
                    results.passed = numbers.first().copied().unwrap_or(0);
                }
                if line_lower.contains("failed") && numbers.len() > 1 {
                    results.failed = numbers.get(1).copied().unwrap_or(0);
                }
                if line_lower.contains("total") {
                    results.total = numbers.last().copied().unwrap_or(0);
                }
            }
        }
    }

    if results.total == 0 {
        results.total = results.passed + results.failed + results.skipped;
    }

    results
}

/// Parse go test output
fn parse_go_test_output(stdout: &str, stderr: &str) -> TestResults {
    let mut results = TestResults::default();
    let combined = format!("{}\n{}", stdout, stderr);

    for line in combined.lines() {
        if line.starts_with("--- PASS:") {
            results.passed += 1;
        } else if line.starts_with("--- FAIL:") {
            results.failed += 1;

            // Extract test name
            let name = line
                .trim_start_matches("--- FAIL:")
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();

            results.failures.push(TestFailure {
                name,
                file: None,
                line: None,
                message: String::new(),
                traceback: None,
            });
        } else if line.starts_with("--- SKIP:") {
            results.skipped += 1;
        } else if line.starts_with("ok ") || line.starts_with("PASS") {
            // Package passed
        } else if line.starts_with("FAIL") {
            // Package failed (already counted individual tests)
        }
    }

    results.total = results.passed + results.failed + results.skipped;
    results
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_framework_cargo() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        assert_eq!(detect_framework(temp_dir.path()), TestFramework::Cargo);
    }

    #[test]
    fn test_detect_framework_pytest() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("pyproject.toml"), "[project]").unwrap();

        assert_eq!(detect_framework(temp_dir.path()), TestFramework::Pytest);
    }

    #[test]
    fn test_detect_framework_npm() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        assert_eq!(detect_framework(temp_dir.path()), TestFramework::Npm);
    }

    #[test]
    fn test_detect_framework_vitest() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join("package.json"),
            r#"{"devDependencies": {"vitest": "^1.0.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(temp_dir.path()), TestFramework::Vitest);
    }

    #[test]
    fn test_detect_framework_go() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("go.mod"), "module test").unwrap();

        assert_eq!(detect_framework(temp_dir.path()), TestFramework::Go);
    }

    #[test]
    fn test_parse_pytest_output() {
        let stdout = r#"
tests/test_example.py::test_one PASSED
tests/test_example.py::test_two FAILED
E       AssertionError: assert 1 == 2
FAILED tests/test_example.py::test_two - AssertionError: assert 1 == 2
========= 1 passed, 1 failed in 0.05s =========
"#;

        let results = parse_pytest_output(stdout, "");

        assert_eq!(results.passed, 1);
        assert_eq!(results.failed, 1);
        assert_eq!(results.total, 2);
        assert_eq!(results.failures.len(), 1);
    }

    #[test]
    fn test_parse_cargo_output() {
        let stdout = r#"
running 3 tests
test tests::test_one ... ok
test tests::test_two ... ok
test tests::test_three ... FAILED

failures:

---- tests::test_three stdout ----
thread 'tests::test_three' panicked at 'assertion failed'

failures:
    tests::test_three

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
"#;

        let results = parse_cargo_test_output(stdout, "");

        assert_eq!(results.passed, 2);
        assert_eq!(results.failed, 1);
        assert_eq!(results.total, 3);
    }

    #[test]
    fn test_parse_go_output() {
        let stdout = r#"
=== RUN   TestOne
--- PASS: TestOne (0.00s)
=== RUN   TestTwo
--- FAIL: TestTwo (0.00s)
    example_test.go:10: expected 1, got 2
FAIL
"#;

        let results = parse_go_test_output(stdout, "");

        assert_eq!(results.passed, 1);
        assert_eq!(results.failed, 1);
        assert_eq!(results.total, 2);
    }
}
