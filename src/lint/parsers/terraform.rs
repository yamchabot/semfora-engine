//! Terraform linter output parsers (tflint, terraform validate, terraform fmt).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse TFLint JSON output
///
/// TFLint --format=json outputs:
/// ```json
/// {
///   "issues": [
///     {
///       "rule": {"name": "aws_instance_invalid_type", "severity": "error"},
///       "message": "Invalid instance type",
///       "range": {"filename": "main.tf", "start": {"line": 10, "column": 5}, "end": {"line": 10, "column": 20}},
///       "callers": []
///     }
///   ]
/// }
/// ```
pub fn parse_tflint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(issue_array) = json.get("issues").and_then(|i| i.as_array()) {
            for issue in issue_array {
                if let Some(parsed) = parse_tflint_issue(issue, dir) {
                    issues.push(parsed);
                }
            }
        }
    }

    issues
}

fn parse_tflint_issue(issue: &serde_json::Value, dir: &Path) -> Option<LintIssue> {
    let message = issue.get("message")?.as_str()?.to_string();

    let rule = issue.get("rule")?;
    let rule_name = rule.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
    let severity_str = rule.get("severity").and_then(|s| s.as_str()).unwrap_or("error");

    let severity = match severity_str {
        "error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        "notice" => LintSeverity::Info,
        _ => LintSeverity::Warning,
    };

    let range = issue.get("range")?;
    let filename = range.get("filename").and_then(|f| f.as_str()).unwrap_or("");
    let start = range.get("start")?;
    let line = start.get("line")?.as_u64()? as usize;
    let column = start.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);

    let end = range.get("end");
    let end_line = end.and_then(|e| e.get("line")).and_then(|l| l.as_u64()).map(|l| l as usize);
    let end_column = end.and_then(|e| e.get("column")).and_then(|c| c.as_u64()).map(|c| c as usize);

    // Make path relative
    let file_path = PathBuf::from(filename);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| filename.to_string());

    Some(LintIssue {
        file: relative_file,
        line,
        column,
        end_line,
        end_column,
        severity,
        rule: rule_name.to_string(),
        message,
        linter: Linter::TfLint,
        fix: None,
    })
}

/// Parse terraform validate JSON output
///
/// terraform validate -json outputs:
/// ```json
/// {
///   "valid": false,
///   "diagnostics": [
///     {
///       "severity": "error",
///       "summary": "Missing required argument",
///       "detail": "The argument \"name\" is required...",
///       "range": {"filename": "main.tf", "start": {"line": 1, "column": 1}, "end": {"line": 1, "column": 10}}
///     }
///   ]
/// }
/// ```
pub fn parse_terraform_validate_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(diagnostics) = json.get("diagnostics").and_then(|d| d.as_array()) {
            for diagnostic in diagnostics {
                if let Some(parsed) = parse_terraform_diagnostic(diagnostic, dir, Linter::TerraformValidate) {
                    issues.push(parsed);
                }
            }
        }
    }

    issues
}

fn parse_terraform_diagnostic(diagnostic: &serde_json::Value, dir: &Path, linter: Linter) -> Option<LintIssue> {
    let summary = diagnostic.get("summary")?.as_str()?.to_string();
    let detail = diagnostic.get("detail").and_then(|d| d.as_str()).unwrap_or("");
    let message = if detail.is_empty() {
        summary
    } else {
        format!("{}: {}", summary, detail)
    };

    let severity_str = diagnostic.get("severity").and_then(|s| s.as_str()).unwrap_or("error");
    let severity = match severity_str {
        "error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        _ => LintSeverity::Warning,
    };

    let range = diagnostic.get("range")?;
    let filename = range.get("filename").and_then(|f| f.as_str()).unwrap_or("");
    let start = range.get("start")?;
    let line = start.get("line")?.as_u64()? as usize;
    let column = start.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);

    let end = range.get("end");
    let end_line = end.and_then(|e| e.get("line")).and_then(|l| l.as_u64()).map(|l| l as usize);
    let end_column = end.and_then(|e| e.get("column")).and_then(|c| c.as_u64()).map(|c| c as usize);

    // Make path relative
    let file_path = PathBuf::from(filename);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| filename.to_string());

    Some(LintIssue {
        file: relative_file,
        line,
        column,
        end_line,
        end_column,
        severity,
        rule: "terraform".to_string(),
        message,
        linter,
        fix: None,
    })
}

/// Parse terraform fmt -check -diff output
///
/// terraform fmt -check -diff outputs:
/// ```text
/// main.tf
/// --- old
/// +++ new
/// @@ -1,3 +1,3 @@
///  resource "aws_instance" "example" {
/// -  ami           = "ami-12345678"
/// +  ami = "ami-12345678"
///  }
/// ```
pub fn parse_terraform_fmt_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    for line in stdout.lines() {
        // File name lines don't start with special characters
        if !line.starts_with("---")
            && !line.starts_with("+++")
            && !line.starts_with("@@")
            && !line.starts_with(' ')
            && !line.starts_with('-')
            && !line.starts_with('+')
            && !line.is_empty()
        {
            let filename = line.trim().to_string();

            // Each file that needs formatting is an issue
            let file_path = PathBuf::from(&filename);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| filename.clone());

            issues.push(LintIssue {
                file: relative_file,
                line: 1,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "formatting".to_string(),
                message: "File needs formatting".to_string(),
                linter: Linter::TerraformFmt,
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
    fn test_parse_tflint_output() {
        let output = r#"{
            "issues": [
                {
                    "rule": {"name": "aws_instance_invalid_type", "severity": "error"},
                    "message": "\"t2.micro\" is an invalid instance type",
                    "range": {"filename": "main.tf", "start": {"line": 10, "column": 5}, "end": {"line": 10, "column": 20}},
                    "callers": []
                }
            ]
        }"#;

        let issues = parse_tflint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "main.tf");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "aws_instance_invalid_type");
    }

    #[test]
    fn test_parse_terraform_validate_output() {
        let output = r#"{
            "valid": false,
            "diagnostics": [
                {
                    "severity": "error",
                    "summary": "Missing required argument",
                    "detail": "The argument 'name' is required",
                    "range": {"filename": "main.tf", "start": {"line": 1, "column": 1}, "end": {"line": 1, "column": 10}}
                }
            ]
        }"#;

        let issues = parse_terraform_validate_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert!(issues[0].message.contains("Missing required argument"));
    }

    #[test]
    fn test_parse_terraform_fmt_output() {
        let output = "main.tf\nvariables.tf";

        let issues = parse_terraform_fmt_output(output, Path::new("."));
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].file, "main.tf");
        assert_eq!(issues[1].file, "variables.tf");
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_tflint_output("", Path::new("."));
        assert!(issues.is_empty());
    }
}
