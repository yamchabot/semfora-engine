//! YAML linter output parsers (yamllint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse yamllint parsable output
///
/// yamllint --format parsable outputs:
/// ```text
/// file.yaml:10:5: [warning] too many spaces after colon (colons)
/// file.yaml:15:1: [error] syntax error: mapping values are not allowed here
/// ```
pub fn parse_yamllint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    for line in stdout.lines() {
        if let Some(issue) = parse_yamllint_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

fn parse_yamllint_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Format: "file.yaml:line:col: [severity] message (rule)"
    // or: "file.yaml:line:col: [severity] message"

    // Split by first : to get file
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    if parts.len() < 4 {
        return None;
    }

    let file = parts[0].trim();
    let line_num: usize = parts[1].parse().ok()?;
    let col: usize = parts[2].parse().ok()?;
    let rest = parts[3].trim();

    // Parse "[severity] message (rule)" or "[severity] message"
    if !rest.starts_with('[') {
        return None;
    }

    let bracket_end = rest.find(']')?;
    let severity_str = &rest[1..bracket_end];
    let message_part = rest[bracket_end + 1..].trim();

    let severity = match severity_str {
        "error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        _ => LintSeverity::Warning,
    };

    // Extract rule from "(rule)" at end if present
    let (message, rule) = if let Some(paren_start) = message_part.rfind(" (") {
        if message_part.ends_with(')') {
            let rule_name = &message_part[paren_start + 2..message_part.len() - 1];
            let msg = message_part[..paren_start].to_string();
            (msg, rule_name.to_string())
        } else {
            (message_part.to_string(), "unknown".to_string())
        }
    } else {
        (message_part.to_string(), "unknown".to_string())
    };

    // Make path relative
    let file_path = PathBuf::from(file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());

    Some(LintIssue {
        file: relative_file,
        line: line_num,
        column: Some(col),
        end_line: None,
        end_column: None,
        severity,
        rule,
        message,
        linter: Linter::YamlLint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_yamllint_output() {
        let output = r#"config.yaml:10:5: [warning] too many spaces after colon (colons)
config.yaml:15:1: [error] syntax error: mapping values are not allowed here"#;

        let issues = parse_yamllint_output(output, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "config.yaml");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].rule, "colons");
        assert!(issues[0].message.contains("too many spaces"));

        assert_eq!(issues[1].line, 15);
        assert_eq!(issues[1].severity, LintSeverity::Error);
        assert!(issues[1].message.contains("syntax error"));
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_yamllint_output("", Path::new("."));
        assert!(issues.is_empty());
    }
}
