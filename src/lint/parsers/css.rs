//! CSS/SCSS/SASS linter output parsers (Stylelint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse Stylelint JSON output
///
/// Stylelint JSON format:
/// ```json
/// [
///   {
///     "source": "file.css",
///     "warnings": [
///       {
///         "line": 1,
///         "column": 5,
///         "rule": "color-no-invalid-hex",
///         "severity": "error",
///         "text": "Unexpected invalid hex color"
///       }
///     ]
///   }
/// ]
/// ```
pub fn parse_stylelint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try to parse as JSON array
    let json: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return issues,
    };

    // Stylelint outputs an array of file results
    if let Some(results) = json.as_array() {
        for result in results {
            let file = result.get("source").and_then(|s| s.as_str()).unwrap_or("");

            if let Some(warnings) = result.get("warnings").and_then(|w| w.as_array()) {
                for warning in warnings {
                    if let Some(lint_issue) = parse_stylelint_warning(warning, file, dir) {
                        issues.push(lint_issue);
                    }
                }
            }
        }
    }

    issues
}

fn parse_stylelint_warning(
    warning: &serde_json::Value,
    file: &str,
    dir: &Path,
) -> Option<LintIssue> {
    let message = warning.get("text")?.as_str()?.to_string();
    let line = warning.get("line")?.as_u64()? as usize;
    let column = warning
        .get("column")
        .and_then(|c| c.as_u64())
        .map(|c| c as usize);

    // Get severity from string value
    let severity = match warning.get("severity").and_then(|s| s.as_str()) {
        Some("error") => LintSeverity::Error,
        Some("warning") => LintSeverity::Warning,
        _ => LintSeverity::Warning,
    };

    let rule = warning
        .get("rule")
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
        linter: Linter::Stylelint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_stylelint_output() {
        let output = r##"[
            {
                "source": "src/styles.css",
                "warnings": [
                    {
                        "line": 10,
                        "column": 5,
                        "rule": "color-no-invalid-hex",
                        "severity": "error",
                        "text": "Unexpected invalid hex color '#fff1az'"
                    },
                    {
                        "line": 15,
                        "column": 3,
                        "rule": "indentation",
                        "severity": "warning",
                        "text": "Expected indentation of 2 spaces"
                    }
                ]
            }
        ]"##;

        let issues = parse_stylelint_output(output, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/styles.css");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "color-no-invalid-hex");
        assert!(issues[0].message.contains("invalid hex color"));

        assert_eq!(issues[1].line, 15);
        assert_eq!(issues[1].severity, LintSeverity::Warning);
        assert_eq!(issues[1].rule, "indentation");
    }

    #[test]
    fn test_parse_scss_output() {
        let output = r##"[
            {
                "source": "src/app.scss",
                "warnings": [
                    {
                        "line": 5,
                        "column": 1,
                        "rule": "scss/at-rule-no-unknown",
                        "severity": "error",
                        "text": "Unexpected unknown at-rule '@unknown'"
                    }
                ]
            }
        ]"##;

        let issues = parse_stylelint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].file, "src/app.scss");
        assert_eq!(issues[0].rule, "scss/at-rule-no-unknown");
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_stylelint_output("[]", Path::new("."));
        assert!(issues.is_empty());

        // File with no warnings
        let output = r#"[{"source": "clean.css", "warnings": []}]"#;
        let issues = parse_stylelint_output(output, Path::new("."));
        assert!(issues.is_empty());
    }

    #[test]
    fn test_parse_multiple_files() {
        let output = r#"[
            {
                "source": "a.css",
                "warnings": [
                    {"line": 1, "column": 1, "rule": "rule-a", "severity": "error", "text": "Error A"}
                ]
            },
            {
                "source": "b.scss",
                "warnings": [
                    {"line": 2, "column": 2, "rule": "rule-b", "severity": "warning", "text": "Warning B"}
                ]
            }
        ]"#;

        let issues = parse_stylelint_output(output, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "a.css");
        assert_eq!(issues[1].file, "b.scss");
    }
}
