//! Shell linter output parsers (shellcheck, shfmt).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse ShellCheck JSON output
///
/// shellcheck --format=json outputs:
/// ```json
/// [
///   {
///     "file": "script.sh",
///     "line": 10,
///     "endLine": 10,
///     "column": 5,
///     "endColumn": 15,
///     "level": "warning",
///     "code": 2034,
///     "message": "Variable appears unused"
///   }
/// ]
/// ```
pub fn parse_shellcheck_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(array) = json.as_array() {
            for item in array {
                if let Some(issue) = parse_shellcheck_item(item, dir) {
                    issues.push(issue);
                }
            }
        }
    }

    issues
}

fn parse_shellcheck_item(item: &serde_json::Value, dir: &Path) -> Option<LintIssue> {
    let file = item.get("file")?.as_str()?.to_string();
    let line = item.get("line")?.as_u64()? as usize;
    let column = item.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);
    let end_line = item.get("endLine").and_then(|e| e.as_u64()).map(|e| e as usize);
    let end_column = item.get("endColumn").and_then(|e| e.as_u64()).map(|e| e as usize);

    let level = item.get("level").and_then(|l| l.as_str()).unwrap_or("warning");
    let severity = match level {
        "error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        "info" => LintSeverity::Info,
        "style" => LintSeverity::Hint,
        _ => LintSeverity::Warning,
    };

    let code = item.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
    let message = item.get("message")?.as_str()?.to_string();

    // Make path relative
    let file_path = PathBuf::from(&file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(file);

    Some(LintIssue {
        file: relative_file,
        line,
        column,
        end_line,
        end_column,
        severity,
        rule: format!("SC{}", code),
        message,
        linter: Linter::ShellCheck,
        fix: None,
    })
}

/// Parse shfmt diff output
///
/// shfmt -d outputs:
/// ```text
/// --- a/script.sh
/// +++ b/script.sh
/// @@ -1,3 +1,3 @@
///  #!/bin/bash
/// -echo "hello"
/// +echo "hello"
/// ```
///
/// shfmt -l outputs just file names that need formatting:
/// ```text
/// script.sh
/// lib/utils.sh
/// ```
pub fn parse_shfmt_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // shfmt -l outputs file names that differ
    let output = if stdout.is_empty() { stderr } else { stdout };

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip diff headers
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
            continue;
        }

        // Skip diff content lines
        if line.starts_with(' ') || line.starts_with('-') || line.starts_with('+') {
            continue;
        }

        // This is a file name
        let file_path = PathBuf::from(line);
        let relative_file = file_path
            .strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| line.to_string());

        issues.push(LintIssue {
            file: relative_file,
            line: 1,
            column: None,
            end_line: None,
            end_column: None,
            severity: LintSeverity::Warning,
            rule: "formatting".to_string(),
            message: "File needs formatting".to_string(),
            linter: Linter::Shfmt,
            fix: None,
        });
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_shellcheck_output() {
        let output = r#"[
            {
                "file": "script.sh",
                "line": 10,
                "endLine": 10,
                "column": 5,
                "endColumn": 15,
                "level": "warning",
                "code": 2034,
                "message": "Variable appears unused"
            }
        ]"#;

        let issues = parse_shellcheck_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "script.sh");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].rule, "SC2034");
        assert!(issues[0].message.contains("unused"));
    }

    #[test]
    fn test_parse_shellcheck_error() {
        let output = r#"[
            {
                "file": "script.sh",
                "line": 5,
                "column": 1,
                "level": "error",
                "code": 1091,
                "message": "Not following: script.sh was not specified as input"
            }
        ]"#;

        let issues = parse_shellcheck_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_parse_shfmt_output() {
        let output = "script.sh\nlib/utils.sh";

        let issues = parse_shfmt_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].file, "script.sh");
        assert_eq!(issues[1].file, "lib/utils.sh");
        assert_eq!(issues[0].severity, LintSeverity::Warning);
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_shellcheck_output("[]", Path::new("."));
        assert!(issues.is_empty());

        let issues = parse_shfmt_output("", "", Path::new("."));
        assert!(issues.is_empty());
    }
}
