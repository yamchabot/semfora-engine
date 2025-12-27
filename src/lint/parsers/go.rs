//! Go linter output parsers (golangci-lint, gofmt, go vet).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse golangci-lint JSON output
pub fn parse_golangci_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(result) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(lint_issues) = result.get("Issues").and_then(|i| i.as_array()) {
            for issue in lint_issues {
                let file = issue
                    .get("Pos")
                    .and_then(|p| p.get("Filename"))
                    .and_then(|f| f.as_str())
                    .unwrap_or("");
                let file_path = PathBuf::from(file);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let line = issue
                    .get("Pos")
                    .and_then(|p| p.get("Line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(1) as usize;
                let column = issue
                    .get("Pos")
                    .and_then(|p| p.get("Column"))
                    .and_then(|c| c.as_u64())
                    .map(|c| c as usize);

                let rule = issue
                    .get("FromLinter")
                    .and_then(|r| r.as_str())
                    .unwrap_or("");
                let message = issue.get("Text").and_then(|m| m.as_str()).unwrap_or("");
                let severity_str = issue.get("Severity").and_then(|s| s.as_str()).unwrap_or("");

                let severity = match severity_str {
                    "error" => LintSeverity::Error,
                    "warning" => LintSeverity::Warning,
                    _ => LintSeverity::Warning,
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line,
                    column,
                    end_line: None,
                    end_column: None,
                    severity,
                    rule: rule.to_string(),
                    message: message.to_string(),
                    linter: Linter::GolangciLint,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Parse gofmt output
pub fn parse_gofmt_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // gofmt -l outputs list of files that need formatting
    for line in stdout.lines() {
        let file = line.trim();
        if file.is_empty() {
            continue;
        }

        let file_path = PathBuf::from(file);
        let relative_file = file_path
            .strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file.to_string());

        issues.push(LintIssue {
            file: relative_file,
            line: 1,
            column: None,
            end_line: None,
            end_column: None,
            severity: LintSeverity::Warning,
            rule: "formatting".to_string(),
            message: "File needs formatting".to_string(),
            linter: Linter::Gofmt,
            fix: Some("Run 'gofmt -w' to fix".to_string()),
        });
    }

    issues
}

/// Parse go vet output
pub fn parse_govet_output(stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // go vet outputs: file:line:col: message
    for line in stderr.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 {
            let file = parts[0].trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line_num = parts[1].trim().parse().unwrap_or(1);
            let message = if parts.len() >= 4 {
                parts[3].trim()
            } else {
                parts[2].trim()
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "vet".to_string(),
                message: message.to_string(),
                linter: Linter::GoVet,
                fix: None,
            });
        }
    }

    issues
}
