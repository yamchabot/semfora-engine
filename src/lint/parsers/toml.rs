//! TOML linter output parsers (taplo).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse taplo JSON output
///
/// taplo check --output-format json outputs:
/// ```json
/// {
///   "errors": [
///     {
///       "range": {"start": {"line": 5, "column": 0}, "end": {"line": 5, "column": 10}},
///       "message": "expected '=' after key",
///       "file": "Cargo.toml"
///     }
///   ]
/// }
/// ```
pub fn parse_taplo_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try parsing JSON output first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
            for error in errors {
                if let Some(issue) = parse_taplo_error(error, dir) {
                    issues.push(issue);
                }
            }
        }
        return issues;
    }

    // Fallback: parse text output from stderr
    // taplo outputs "error: message" or "error[file:line:col]: message"
    for line in stderr.lines() {
        if let Some(issue) = parse_taplo_text_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

fn parse_taplo_error(error: &serde_json::Value, dir: &Path) -> Option<LintIssue> {
    let message = error.get("message")?.as_str()?.to_string();
    let file = error.get("file").and_then(|f| f.as_str()).unwrap_or("");

    let range = error.get("range")?;
    let start = range.get("start")?;
    let line = start.get("line")?.as_u64()? as usize + 1; // taplo uses 0-indexed lines
    let column = start
        .get("column")
        .and_then(|c| c.as_u64())
        .map(|c| c as usize + 1);

    let end = range.get("end");
    let end_line = end
        .and_then(|e| e.get("line"))
        .and_then(|l| l.as_u64())
        .map(|l| l as usize + 1);
    let end_column = end
        .and_then(|e| e.get("column"))
        .and_then(|c| c.as_u64())
        .map(|c| c as usize + 1);

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
        end_line,
        end_column,
        severity: LintSeverity::Error,
        rule: "syntax".to_string(),
        message,
        linter: Linter::Taplo,
        fix: None,
    })
}

fn parse_taplo_text_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Format: "error[file:line:col]: message" or "error: message"
    if !line.starts_with("error") {
        return None;
    }

    let (file, line_num, col, message) = if line.starts_with("error[") {
        // Parse "error[file:line:col]: message"
        let bracket_end = line.find(']')?;
        let location = &line[6..bracket_end];
        let parts: Vec<&str> = location.split(':').collect();

        if parts.len() < 3 {
            return None;
        }

        let file = parts[0];
        let ln: usize = parts[1].parse().ok()?;
        let c: usize = parts[2].parse().ok()?;
        let msg = line[bracket_end + 2..].trim().to_string();

        (file.to_string(), ln, Some(c), msg)
    } else {
        // Parse "error: message" - no file info
        let msg = line.strip_prefix("error:")?.trim().to_string();
        ("".to_string(), 1, None, msg)
    };

    // Make path relative
    let relative_file = if !file.is_empty() {
        let file_path = PathBuf::from(&file);
        file_path
            .strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(file)
    } else {
        file
    };

    Some(LintIssue {
        file: relative_file,
        line: line_num,
        column: col,
        end_line: None,
        end_column: None,
        severity: LintSeverity::Error,
        rule: "syntax".to_string(),
        message,
        linter: Linter::Taplo,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_taplo_json_output() {
        let output = r##"{
            "errors": [
                {
                    "range": {"start": {"line": 4, "column": 0}, "end": {"line": 4, "column": 10}},
                    "message": "expected '=' after key",
                    "file": "Cargo.toml"
                }
            ]
        }"##;

        let issues = parse_taplo_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].file, "Cargo.toml");
        assert_eq!(issues[0].line, 5); // 0-indexed to 1-indexed
        assert_eq!(issues[0].column, Some(1));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert!(issues[0].message.contains("expected"));
    }

    #[test]
    fn test_parse_taplo_text_output() {
        let stderr = "error[pyproject.toml:10:5]: invalid key";

        let issues = parse_taplo_output("", stderr, Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].file, "pyproject.toml");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_taplo_output("", "", Path::new("."));
        assert!(issues.is_empty());
    }
}
