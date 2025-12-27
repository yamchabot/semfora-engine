//! Kotlin linter output parsers (detekt, ktlint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse detekt JSON output
pub fn parse_detekt_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(findings) = result.as_array() {
            for finding in findings {
                let rule = finding
                    .get("ruleSet")
                    .and_then(|r| r.as_str())
                    .unwrap_or("detekt");
                let rule_id = finding.get("ruleId").and_then(|r| r.as_str()).unwrap_or("");
                let message = finding
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");

                if let Some(location) = finding.get("location") {
                    let file = location.get("file").and_then(|f| f.as_str()).unwrap_or("");
                    let file_path = PathBuf::from(file);
                    let relative_file = file_path
                        .strip_prefix(dir)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| file.to_string());

                    let line = location.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                    let column = location
                        .get("column")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);

                    let severity_str = finding.get("severity").and_then(|s| s.as_str());
                    let severity = match severity_str {
                        Some("error") => LintSeverity::Error,
                        Some("warning") => LintSeverity::Warning,
                        Some("info") => LintSeverity::Info,
                        _ => LintSeverity::Warning,
                    };

                    let full_rule = if rule_id.is_empty() {
                        rule.to_string()
                    } else {
                        format!("{}/{}", rule, rule_id)
                    };

                    issues.push(LintIssue {
                        file: relative_file,
                        line,
                        column,
                        end_line: None,
                        end_column: None,
                        severity,
                        rule: full_rule,
                        message: message.to_string(),
                        linter: Linter::Detekt,
                        fix: None,
                    });
                }
            }
        }
        return issues;
    }

    // Fallback: parse console output
    // Format: file:line:col: message (RuleSet:RuleId)
    for line in stdout.lines() {
        if line.contains("- ") && line.contains(":") {
            // detekt output often has "- " prefix
            let line = line.trim_start_matches("- ").trim();
            let parts: Vec<&str> = line.splitn(4, ':').collect();

            if parts.len() >= 3 {
                let file = parts[0].trim();
                let file_path = PathBuf::from(file);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let line_num = parts[1].trim().parse().unwrap_or(1);
                let message = parts.get(3).unwrap_or(&parts[2]).trim();

                issues.push(LintIssue {
                    file: relative_file,
                    line: line_num,
                    column: parts.get(2).and_then(|s| s.trim().parse().ok()),
                    end_line: None,
                    end_column: None,
                    severity: LintSeverity::Warning,
                    rule: "detekt".to_string(),
                    message: message.to_string(),
                    linter: Linter::Detekt,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Parse ktlint JSON output
pub fn parse_ktlint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format (--reporter=json)
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("file").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            if let Some(errors) = result.get("errors").and_then(|e| e.as_array()) {
                for error in errors {
                    let line = error.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                    let column = error
                        .get("column")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);
                    let message = error.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    let rule = error
                        .get("rule")
                        .and_then(|r| r.as_str())
                        .unwrap_or("ktlint");

                    issues.push(LintIssue {
                        file: relative_file.clone(),
                        line,
                        column,
                        end_line: None,
                        end_column: None,
                        severity: LintSeverity::Error, // ktlint treats all as errors
                        rule: rule.to_string(),
                        message: message.to_string(),
                        linter: Linter::Ktlint,
                        fix: Some("Run 'ktlint -F' to fix".to_string()),
                    });
                }
            }
        }
        return issues;
    }

    // Fallback: parse console output
    // Format: file:line:col: message (rule-id)
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
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
            let column = parts.get(2).and_then(|s| s.trim().parse().ok());
            let message = parts.get(3).unwrap_or(&"").trim();

            // Extract rule from (rule-id) at end
            let (msg, rule) = if let Some(paren_start) = message.rfind('(') {
                if message.ends_with(')') {
                    (
                        message[..paren_start].trim(),
                        message[paren_start + 1..message.len() - 1].to_string(),
                    )
                } else {
                    (message, "ktlint".to_string())
                }
            } else {
                (message, "ktlint".to_string())
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Error,
                rule,
                message: msg.to_string(),
                linter: Linter::Ktlint,
                fix: Some("Run 'ktlint -F' to fix".to_string()),
            });
        }
    }

    issues
}
