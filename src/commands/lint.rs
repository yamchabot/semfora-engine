//! Lint command handler - Run linters, formatters, and type checkers

use std::path::PathBuf;

use crate::cli::{LintArgs, LintOperation, OutputFormat};
use crate::commands::CommandContext;
use crate::error::Result;
use crate::lint::{detect_linters, get_recommendations, DetectedLinter};

/// Run the lint command
pub fn run_lint(args: &LintArgs, ctx: &CommandContext) -> Result<String> {
    match &args.operation {
        LintOperation::Scan {
            path,
            linter,
            severity,
            limit,
            file,
            fixable_only,
        } => run_lint_scan(path, linter, severity, *limit, file, *fixable_only, ctx),

        LintOperation::Fix {
            path,
            linter,
            dry_run,
            safe_only,
        } => run_lint_fix(path, linter, *dry_run, *safe_only, ctx),

        LintOperation::Typecheck {
            path,
            checker,
            limit,
        } => run_typecheck(path, checker, *limit, ctx),

        LintOperation::Detect { path } => run_detect_linters(path, ctx),

        LintOperation::Recommend { path } => run_lint_recommend(path, ctx),
    }
}

/// Scan for lint issues
fn run_lint_scan(
    path: &Option<PathBuf>,
    linter: &Option<String>,
    severity: &Option<Vec<String>>,
    limit: usize,
    _file: &Option<String>,
    fixable_only: bool,
    ctx: &CommandContext,
) -> Result<String> {
    use crate::lint::{run_lint, LintRunOptions, LintSeverity, Linter};

    let project_dir = path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Parse options
    let target_linter = linter.as_ref().and_then(|l| l.parse::<Linter>().ok());
    let severity_filter = severity
        .as_ref()
        .and_then(|sev| sev.first().and_then(|s| s.parse::<LintSeverity>().ok()));

    let options = LintRunOptions {
        linter: target_linter,
        path_filter: None,
        severity_filter,
        limit: Some(limit),
        fixable_only,
        fix: false,
        dry_run: false,
        safe_only: false,
    };

    // Run linters
    let results = run_lint(&project_dir, &options)?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "lint_scan",
        "path": project_dir.to_string_lossy(),
        "success": results.success,
        "error_count": results.error_count,
        "warning_count": results.warning_count,
        "files_with_issues": results.files_with_issues,
        "duration_ms": results.duration_ms,
        "linters": results.linters.iter().map(|l| serde_json::json!({
            "linter": l.linter.as_str(),
            "success": l.success,
            "errors": l.error_count,
            "warnings": l.warning_count,
            "duration_ms": l.duration_ms,
        })).collect::<Vec<_>>(),
        "issues": results.issues.iter().map(|i| serde_json::json!({
            "file": i.file,
            "line": i.line,
            "column": i.column,
            "severity": i.severity.as_str(),
            "rule": i.rule,
            "message": i.message,
            "linter": i.linter.as_str(),
            "fix": i.fix,
        })).collect::<Vec<_>>(),
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            let status = if results.success { "pass" } else { "fail" };
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  LINT SCAN: {}\n", status.to_uppercase()));
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("path: {}\n", project_dir.display()));
            output.push_str(&format!(
                "status: {} | errors: {} | warnings: {} | files: {} | duration: {}ms\n",
                status,
                results.error_count,
                results.warning_count,
                results.files_with_issues,
                results.duration_ms
            ));

            // Show linter summary
            if !results.linters.is_empty() {
                output.push_str("\nlinters:\n");
                for l in &results.linters {
                    let status_icon = if l.success { "✓" } else { "✗" };
                    output.push_str(&format!(
                        "  {} {} (E:{} W:{}, {}ms)\n",
                        status_icon,
                        l.linter.display_name(),
                        l.error_count,
                        l.warning_count,
                        l.duration_ms
                    ));
                }
            }

            // Group issues by file
            if !results.issues.is_empty() {
                output.push_str("\n───────────────────────────────────────────\n");
                output.push_str("  ISSUES\n");
                output.push_str("───────────────────────────────────────────\n");

                let mut current_file = String::new();
                for issue in &results.issues {
                    if issue.file != current_file {
                        current_file = issue.file.clone();
                        output.push_str(&format!("\n[{}]\n", current_file));
                    }

                    let severity_code = issue.severity.code();
                    let col = issue.column.map(|c| format!(":{}", c)).unwrap_or_default();
                    output.push_str(&format!(
                        "  {}:{}{} {} [{}] {}\n",
                        issue.line, col, "", severity_code, issue.rule, issue.message
                    ));
                }
            }
        }
    }

    Ok(output)
}

/// Apply automatic fixes
fn run_lint_fix(
    path: &Option<PathBuf>,
    linter: &Option<String>,
    dry_run: bool,
    safe_only: bool,
    ctx: &CommandContext,
) -> Result<String> {
    use crate::lint::{run_lint, LintRunOptions, Linter};

    let project_dir = path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Parse options
    let target_linter = linter.as_ref().and_then(|l| l.parse::<Linter>().ok());

    let options = LintRunOptions {
        linter: target_linter,
        path_filter: None,
        severity_filter: None,
        limit: None,
        fixable_only: false,
        fix: true,
        dry_run,
        safe_only,
    };

    // Run linters in fix mode
    let results = run_lint(&project_dir, &options)?;

    let mut output = String::new();

    let mode = if dry_run { "dry-run" } else { "fix" };

    let json_value = serde_json::json!({
        "_type": "lint_fix",
        "path": project_dir.to_string_lossy(),
        "mode": mode,
        "success": results.success,
        "linters_run": results.linters.iter().map(|l| l.linter.as_str()).collect::<Vec<_>>(),
        "duration_ms": results.duration_ms,
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  LINT FIX ({})\n", mode.to_uppercase()));
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("path: {}\n", project_dir.display()));
            output.push_str(&format!("duration: {}ms\n\n", results.duration_ms));

            if !results.linters.is_empty() {
                output.push_str("linters run:\n");
                for l in &results.linters {
                    let status_icon = if l.success { "✓" } else { "✗" };
                    output.push_str(&format!("  {} {}\n", status_icon, l.linter.display_name()));
                }
            }

            if dry_run {
                output.push_str("\n(dry run - no changes made)\n");
            }
        }
    }

    Ok(output)
}

/// Run type checkers only
fn run_typecheck(
    path: &Option<PathBuf>,
    _checker: &Option<String>,
    _limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let project_dir = path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "typecheck",
        "path": project_dir.to_string_lossy(),
        "status": "not_implemented",
        "message": "Type checking not yet implemented."
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("Type checking not yet implemented.\n");
        }
    }

    Ok(output)
}

/// Detect available linters
fn run_detect_linters(path: &Option<PathBuf>, ctx: &CommandContext) -> Result<String> {
    let project_dir = path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let detected = detect_linters(&project_dir);

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "lint_detect",
        "path": project_dir.to_string_lossy(),
        "linters": detected.iter().map(|d| serde_json::json!({
            "linter": d.linter.as_str(),
            "display_name": d.linter.display_name(),
            "language": d.linter.language(),
            "available": d.available,
            "version": d.version,
            "config_path": d.config_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            "capabilities": {
                "can_fix": d.capabilities.can_fix,
                "can_format": d.capabilities.can_format,
                "can_typecheck": d.capabilities.can_typecheck,
            },
            "run_command": {
                "program": d.run_command.program,
                "args": d.run_command.args,
                "fix_args": d.run_command.fix_args,
            }
        })).collect::<Vec<_>>(),
        "count": detected.len()
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  DETECTED LINTERS\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("path: {}\n", project_dir.display()));
            output.push_str(&format!("linters_detected: {}\n\n", detected.len()));

            if detected.is_empty() {
                output.push_str("No linters detected.\n");
                output.push_str(
                    "\nUse 'lint recommend' to get suggestions for setting up linters.\n",
                );
            } else {
                // Group by language
                let mut by_language: std::collections::HashMap<&str, Vec<&DetectedLinter>> =
                    std::collections::HashMap::new();
                for d in &detected {
                    by_language.entry(d.linter.language()).or_default().push(d);
                }

                for (lang, linters) in by_language.iter() {
                    output.push_str(&format!("───────────────────────────────────────────\n"));
                    output.push_str(&format!("{}\n", lang.to_uppercase()));
                    output.push_str(&format!("───────────────────────────────────────────\n"));

                    for d in linters {
                        let status = if d.available { "✓" } else { "✗" };
                        output.push_str(&format!("\n{} {}\n", status, d.linter.display_name()));

                        if let Some(ref config) = d.config_path {
                            output.push_str(&format!("  config: {}\n", config.display()));
                        }

                        if let Some(ref version) = d.version {
                            output.push_str(&format!("  version: {}\n", version));
                        }

                        // Show capabilities
                        let mut caps = Vec::new();
                        if d.capabilities.can_fix {
                            caps.push("fix");
                        }
                        if d.capabilities.can_format {
                            caps.push("format");
                        }
                        if d.capabilities.can_typecheck {
                            caps.push("typecheck");
                        }
                        if !caps.is_empty() {
                            output.push_str(&format!("  capabilities: {}\n", caps.join(", ")));
                        }

                        // Show command
                        output.push_str(&format!(
                            "  command: {} {}\n",
                            d.run_command.program,
                            d.run_command.args.join(" ")
                        ));
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Get recommendations for missing linters
fn run_lint_recommend(path: &Option<PathBuf>, ctx: &CommandContext) -> Result<String> {
    let project_dir = path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let recommendations = get_recommendations(&project_dir);

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "lint_recommend",
        "path": project_dir.to_string_lossy(),
        "recommendations": recommendations.iter().map(|r| serde_json::json!({
            "linter": r.linter.as_str(),
            "display_name": r.linter.display_name(),
            "reason": r.reason,
            "install_command": r.install_command,
            "priority": r.priority,
        })).collect::<Vec<_>>(),
        "count": recommendations.len()
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  LINTER RECOMMENDATIONS\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("path: {}\n", project_dir.display()));
            output.push_str(&format!("recommendations: {}\n\n", recommendations.len()));

            if recommendations.is_empty() {
                output.push_str("No recommendations - you have all recommended linters!\n");
            } else {
                for r in &recommendations {
                    output.push_str(&format!("───────────────────────────────────────────\n"));
                    output.push_str(&format!(
                        "{} (priority: {})\n",
                        r.linter.display_name(),
                        r.priority
                    ));
                    output.push_str(&format!("  {}\n", r.reason));
                    output.push_str(&format!("  install: {}\n", r.install_command));
                }
            }
        }
    }

    Ok(output)
}
