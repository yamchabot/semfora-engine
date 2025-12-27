//! Python linter output parsers (Ruff, Black, Mypy).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse Ruff JSON output
pub fn parse_ruff_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result
                .get("filename")
                .and_then(|f| f.as_str())
                .unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line = result
                .get("location")
                .and_then(|l| l.get("row"))
                .and_then(|r| r.as_u64())
                .unwrap_or(1) as usize;
            let column = result
                .get("location")
                .and_then(|l| l.get("column"))
                .and_then(|c| c.as_u64())
                .map(|c| c as usize);

            let code = result.get("code").and_then(|c| c.as_str()).unwrap_or("");
            let message = result.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let fix = result.get("fix").map(|_| "Auto-fixable".to_string());

            issues.push(LintIssue {
                file: relative_file,
                line,
                column,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Error,
                rule: code.to_string(),
                message: message.to_string(),
                linter: Linter::Ruff,
                fix,
            });
        }
    }

    issues
}

/// Parse Black output
pub fn parse_black_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Black --check outputs: "would reformat <file>"
    for line in stdout.lines() {
        if line.starts_with("would reformat ") {
            let file = line.trim_start_matches("would reformat ").trim();
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
                linter: Linter::Black,
                fix: Some("Run 'black' to fix".to_string()),
            });
        }
    }

    issues
}

/// Parse mypy output
pub fn parse_mypy_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format first
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("file").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line = result.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
            let column = result
                .get("column")
                .and_then(|c| c.as_u64())
                .map(|c| c as usize);
            let severity_str = result
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("error");
            let message = result.get("message").and_then(|m| m.as_str()).unwrap_or("");

            let severity = match severity_str {
                "error" => LintSeverity::Error,
                "warning" => LintSeverity::Warning,
                "note" => LintSeverity::Info,
                _ => LintSeverity::Error,
            };

            issues.push(LintIssue {
                file: relative_file,
                line,
                column,
                end_line: None,
                end_column: None,
                severity,
                rule: "type-error".to_string(),
                message: message.to_string(),
                linter: Linter::Mypy,
                fix: None,
            });
        }
        return issues;
    }

    // Fallback to text format: file:line:col: severity: message
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let file = parts[0].trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line_num = parts[1].trim().parse().unwrap_or(1);
            let rest = parts[3].trim();

            let (severity, message) = if rest.starts_with("error:") {
                (
                    LintSeverity::Error,
                    rest.trim_start_matches("error:").trim(),
                )
            } else if rest.starts_with("warning:") {
                (
                    LintSeverity::Warning,
                    rest.trim_start_matches("warning:").trim(),
                )
            } else if rest.starts_with("note:") {
                (LintSeverity::Info, rest.trim_start_matches("note:").trim())
            } else {
                (LintSeverity::Error, rest)
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column: None,
                end_line: None,
                end_column: None,
                severity,
                rule: "type-error".to_string(),
                message: message.to_string(),
                linter: Linter::Mypy,
                fix: None,
            });
        }
    }

    issues
}

/// Parse Pylint JSON output
///
/// Pylint with --output-format=json outputs an array of messages:
/// [{ "type": "convention", "module": "...", "obj": "...", "line": 1, "column": 0, "path": "...", "symbol": "...", "message": "..." }]
pub fn parse_pylint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format first
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("path").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line = result.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
            let column = result
                .get("column")
                .and_then(|c| c.as_u64())
                .map(|c| c as usize);
            let end_line = result
                .get("endLine")
                .and_then(|l| l.as_u64())
                .map(|l| l as usize);
            let end_column = result
                .get("endColumn")
                .and_then(|c| c.as_u64())
                .map(|c| c as usize);

            // Pylint uses type: fatal, error, warning, convention, refactor, info
            let type_str = result.get("type").and_then(|t| t.as_str()).unwrap_or("error");
            let symbol = result
                .get("symbol")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let message_id = result
                .get("message-id")
                .and_then(|m| m.as_str())
                .unwrap_or("");
            let message = result.get("message").and_then(|m| m.as_str()).unwrap_or("");

            let severity = match type_str {
                "fatal" | "error" => LintSeverity::Error,
                "warning" => LintSeverity::Warning,
                "convention" | "refactor" | "info" => LintSeverity::Info,
                _ => LintSeverity::Warning,
            };

            // Use symbol as rule, with message-id as prefix if available
            let rule = if !message_id.is_empty() {
                format!("{}/{}", message_id, symbol)
            } else {
                symbol.to_string()
            };

            issues.push(LintIssue {
                file: relative_file,
                line,
                column,
                end_line,
                end_column,
                severity,
                rule,
                message: message.to_string(),
                linter: Linter::Pylint,
                fix: None,
            });
        }
        return issues;
    }

    // Fallback to text format: file:line:column: message-id (symbol) message
    // Example: test.py:1:0: C0114 (missing-module-docstring) Missing module docstring
    for line in stdout.lines() {
        // Skip summary lines and empty lines
        if line.is_empty()
            || line.starts_with("---")
            || line.starts_with("Your code has been rated")
            || line.starts_with("*")
        {
            continue;
        }

        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let file = parts[0].trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line_num: usize = parts[1].trim().parse().unwrap_or(1);
            let _column: Option<usize> = parts.get(2).and_then(|c| c.trim().parse().ok());
            let rest = parts[3].trim();

            // Parse message ID, symbol, and message
            // Format: " C0114 (missing-module-docstring) Missing module docstring"
            let (rule, message) = if let Some(paren_start) = rest.find('(') {
                if let Some(paren_end) = rest.find(')') {
                    let msg_id = rest[..paren_start].trim();
                    let symbol = &rest[paren_start + 1..paren_end];
                    let msg = rest[paren_end + 1..].trim();
                    (format!("{}/{}", msg_id, symbol), msg.to_string())
                } else {
                    ("pylint".to_string(), rest.to_string())
                }
            } else {
                ("pylint".to_string(), rest.to_string())
            };

            // Determine severity from message ID prefix (C, R, W, E, F)
            let severity = if rule.starts_with('F') || rule.starts_with('E') {
                LintSeverity::Error
            } else if rule.starts_with('W') {
                LintSeverity::Warning
            } else {
                LintSeverity::Info
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column: _column,
                end_line: None,
                end_column: None,
                severity,
                rule,
                message,
                linter: Linter::Pylint,
                fix: None,
            });
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_pylint_json_output() {
        let output = r#"[{"type":"convention","module":"test","obj":"","line":1,"column":0,"path":"test.py","symbol":"missing-module-docstring","message":"Missing module docstring","message-id":"C0114"}]"#;

        let issues = parse_pylint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "test.py");
        assert_eq!(issues[0].line, 1);
        assert_eq!(issues[0].column, Some(0));
        assert_eq!(issues[0].severity, LintSeverity::Info);
        assert_eq!(issues[0].rule, "C0114/missing-module-docstring");
    }

    #[test]
    fn test_parse_pylint_error_output() {
        let output = r#"[{"type":"error","module":"test","obj":"foo","line":5,"column":10,"path":"test.py","symbol":"undefined-variable","message":"Undefined variable 'bar'","message-id":"E0602"}]"#;

        let issues = parse_pylint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "E0602/undefined-variable");
    }
}
