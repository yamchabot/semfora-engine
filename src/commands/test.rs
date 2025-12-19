//! Test command handler - Run or detect tests

use std::path::PathBuf;

use crate::cli::{OutputFormat, TestArgs};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::test_runner::{
    detect_all_frameworks, run_tests, run_tests_with_framework, TestFramework, TestRunOptions,
};

/// Run the test command
pub fn run_test(args: &TestArgs, ctx: &CommandContext) -> Result<String> {
    let project_dir = args
        .path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    if args.detect {
        run_detect_tests(&project_dir, ctx)
    } else {
        run_execute_tests(args, &project_dir, ctx)
    }
}

/// Detect test frameworks without running
fn run_detect_tests(project_dir: &PathBuf, ctx: &CommandContext) -> Result<String> {
    let frameworks = detect_all_frameworks(project_dir);

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "test_detect",
        "path": project_dir.to_string_lossy(),
        "frameworks": frameworks.iter().map(|(fw, path)| serde_json::json!({
            "name": format!("{:?}", fw),
            "path": path.to_string_lossy()
        })).collect::<Vec<_>>(),
        "count": frameworks.len()
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("path: {}\n", project_dir.display()));
            output.push_str(&format!("frameworks_detected: {}\n\n", frameworks.len()));

            if frameworks.is_empty() {
                output.push_str("No test frameworks detected.\n");
                output.push_str("\nSupported frameworks:\n");
                output.push_str("  - pytest (Python)\n");
                output.push_str("  - cargo test (Rust)\n");
                output.push_str("  - npm test / vitest / jest (JavaScript/TypeScript)\n");
                output.push_str("  - go test (Go)\n");
            } else {
                for (fw, path) in &frameworks {
                    output.push_str("---\n");
                    output.push_str(&format!("framework: {:?}\n", fw));
                    output.push_str(&format!("path: {}\n", path.display()));
                }
            }
        }
    }

    Ok(output)
}

/// Execute tests
fn run_execute_tests(
    args: &TestArgs,
    project_dir: &PathBuf,
    ctx: &CommandContext,
) -> Result<String> {
    let options = TestRunOptions {
        filter: args.filter.clone(),
        verbose: args.test_verbose,
        timeout_secs: Some(args.timeout),
        ..Default::default()
    };

    if ctx.verbose {
        eprintln!("Running tests in: {}", project_dir.display());
        if let Some(ref filter) = args.filter {
            eprintln!("Filter: {}", filter);
        }
    }

    let results = if let Some(ref framework_name) = args.framework {
        // Force specific framework
        let framework = parse_framework_name(framework_name)?;
        run_tests_with_framework(project_dir, framework, &options)
    } else {
        // Auto-detect
        run_tests(project_dir, &options)
    };

    let results = results.map_err(|e| McpDiffError::GitError {
        message: format!("Test execution failed: {}", e),
    })?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "test_results",
        "framework": format!("{:?}", results.framework),
        "passed": results.passed,
        "failed": results.failed,
        "skipped": results.skipped,
        "total": results.total,
        "duration_ms": results.duration_ms,
        "success": results.failed == 0,
        "failures": results.failures.iter().map(|f| serde_json::json!({
            "name": f.name,
            "message": f.message,
            "file": f.file,
            "line": f.line
        })).collect::<Vec<_>>()
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            let status = if results.failed == 0 {
                "✓ PASSED"
            } else {
                "✗ FAILED"
            };

            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  TEST RESULTS: {}\n", status));
            output.push_str("═══════════════════════════════════════════\n\n");

            output.push_str(&format!("framework: {:?}\n", results.framework));
            output.push_str(&format!(
                "duration: {:.2}s\n\n",
                results.duration_ms as f64 / 1000.0
            ));

            output.push_str(&format!("passed: {}\n", results.passed));
            output.push_str(&format!("failed: {}\n", results.failed));
            output.push_str(&format!("skipped: {}\n", results.skipped));
            output.push_str(&format!("total: {}\n", results.total));

            if !results.failures.is_empty() {
                output.push_str("\n───────────────────────────────────────────\n");
                output.push_str("FAILURES\n");
                output.push_str("───────────────────────────────────────────\n");

                for failure in &results.failures {
                    output.push_str(&format!("\n• {}\n", failure.name));
                    if let Some(ref file) = failure.file {
                        if let Some(line) = failure.line {
                            output.push_str(&format!("  at {}:{}\n", file, line));
                        } else {
                            output.push_str(&format!("  at {}\n", file));
                        }
                    }
                    if !failure.message.is_empty() {
                        // Truncate long messages
                        let msg = if failure.message.len() > 200 {
                            format!("{}...", &failure.message[..200])
                        } else {
                            failure.message.clone()
                        };
                        output.push_str(&format!("  {}\n", msg));
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Parse framework name string to enum
fn parse_framework_name(name: &str) -> Result<TestFramework> {
    match name.to_lowercase().as_str() {
        "pytest" | "python" => Ok(TestFramework::Pytest),
        "cargo" | "rust" => Ok(TestFramework::Cargo),
        "npm" | "node" => Ok(TestFramework::Npm),
        "vitest" => Ok(TestFramework::Vitest),
        "jest" => Ok(TestFramework::Jest),
        "go" | "golang" => Ok(TestFramework::Go),
        _ => Err(McpDiffError::GitError {
            message: format!(
                "Unknown test framework: '{}'. Supported: pytest, cargo, npm, vitest, jest, go",
                name
            ),
        }),
    }
}
