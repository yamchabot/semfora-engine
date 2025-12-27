//! HTML linter output parsers (HTMLHint, html-validate).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse HTMLHint JSON output
///
/// HTMLHint JSON format:
/// ```json
/// {
///   "file.html": [
///     {
///       "type": "error",
///       "message": "...",
///       "line": 1,
///       "col": 1,
///       "rule": { "id": "tag-pair", "description": "..." }
///     }
///   ]
/// }
/// ```
pub fn parse_htmlhint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try to parse as JSON
    let json: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return issues,
    };

    // HTMLHint outputs an object with file paths as keys
    if let Some(obj) = json.as_object() {
        for (file, file_issues) in obj {
            if let Some(issue_array) = file_issues.as_array() {
                for issue in issue_array {
                    if let Some(lint_issue) = parse_htmlhint_issue(issue, file, dir) {
                        issues.push(lint_issue);
                    }
                }
            }
        }
    }

    issues
}

fn parse_htmlhint_issue(issue: &serde_json::Value, file: &str, dir: &Path) -> Option<LintIssue> {
    let message = issue.get("message")?.as_str()?.to_string();
    let line = issue.get("line")?.as_u64()? as usize;
    let column = issue.get("col").and_then(|c| c.as_u64()).map(|c| c as usize);

    // Get severity from "type" field
    let severity = match issue.get("type").and_then(|t| t.as_str()) {
        Some("error") => LintSeverity::Error,
        Some("warning") => LintSeverity::Warning,
        _ => LintSeverity::Warning,
    };

    // Get rule ID from nested rule object
    let rule = issue
        .get("rule")
        .and_then(|r| r.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Make path relative
    let file_path = PathBuf::from(file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());

    Some(LintIssue {
        file: relative_file,
        line,
        column,
        end_line: None,
        end_column: None,
        severity,
        rule,
        message,
        linter: Linter::HtmlHint,
        fix: None,
    })
}

/// Parse html-validate JSON output
///
/// html-validate JSON format:
/// ```json
/// {
///   "results": [
///     {
///       "filePath": "file.html",
///       "messages": [
///         {
///           "ruleId": "close-order",
///           "severity": 2,
///           "message": "...",
///           "line": 1,
///           "column": 1
///         }
///       ]
///     }
///   ]
/// }
/// ```
pub fn parse_html_validate_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try to parse as JSON
    let json: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return issues,
    };

    // html-validate outputs an object with "results" array
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        for result in results {
            let file = result
                .get("filePath")
                .and_then(|f| f.as_str())
                .unwrap_or("");

            if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    if let Some(lint_issue) = parse_html_validate_message(msg, file, dir) {
                        issues.push(lint_issue);
                    }
                }
            }
        }
    }

    issues
}

fn parse_html_validate_message(
    msg: &serde_json::Value,
    file: &str,
    dir: &Path,
) -> Option<LintIssue> {
    let message = msg.get("message")?.as_str()?.to_string();
    let line = msg.get("line")?.as_u64()? as usize;
    let column = msg.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);

    // Get severity from numeric value (1=warn, 2=error)
    let severity = match msg.get("severity").and_then(|s| s.as_u64()) {
        Some(2) => LintSeverity::Error,
        Some(1) => LintSeverity::Warning,
        _ => LintSeverity::Warning,
    };

    let rule = msg
        .get("ruleId")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Make path relative
    let file_path = PathBuf::from(file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());

    Some(LintIssue {
        file: relative_file,
        line,
        column,
        end_line: None,
        end_column: None,
        severity,
        rule,
        message,
        linter: Linter::HtmlValidate,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_htmlhint_output() {
        let output = r#"{
            "src/index.html": [
                {
                    "type": "error",
                    "message": "Tag must be paired, no matching closing tag",
                    "raw": "<div>",
                    "line": 10,
                    "col": 5,
                    "rule": {
                        "id": "tag-pair",
                        "description": "Tag must be paired"
                    }
                }
            ]
        }"#;

        let issues = parse_htmlhint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].file, "src/index.html");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "tag-pair");
        assert!(issues[0].message.contains("Tag must be paired"));
    }

    #[test]
    fn test_parse_html_validate_output() {
        let output = r#"{
            "results": [
                {
                    "filePath": "src/page.html",
                    "messages": [
                        {
                            "ruleId": "close-order",
                            "severity": 2,
                            "message": "Incorrect order of close tags",
                            "line": 15,
                            "column": 10
                        },
                        {
                            "ruleId": "no-deprecated-attr",
                            "severity": 1,
                            "message": "Attribute 'align' is deprecated",
                            "line": 20,
                            "column": 5
                        }
                    ]
                }
            ]
        }"#;

        let issues = parse_html_validate_output(output, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/page.html");
        assert_eq!(issues[0].line, 15);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "close-order");

        assert_eq!(issues[1].line, 20);
        assert_eq!(issues[1].severity, LintSeverity::Warning);
        assert_eq!(issues[1].rule, "no-deprecated-attr");
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_htmlhint_output("{}", Path::new("."));
        assert!(issues.is_empty());

        let issues = parse_html_validate_output(r#"{"results": []}"#, Path::new("."));
        assert!(issues.is_empty());
    }
}
