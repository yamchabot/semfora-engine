//! JSON linter output parsers (jsonlint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse jsonlint output
///
/// jsonlint outputs errors like:
/// ```text
/// file.json: line 5, col 3, found: 'EOF' - expected: '}'.
/// ```
pub fn parse_jsonlint_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // jsonlint outputs errors to stderr typically
    let output = if stderr.is_empty() { stdout } else { stderr };

    for line in output.lines() {
        if let Some(issue) = parse_jsonlint_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

fn parse_jsonlint_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Format: "file.json: line N, col M, message"
    let parts: Vec<&str> = line.splitn(2, ": line ").collect();
    if parts.len() != 2 {
        return None;
    }

    let file = parts[0].trim();
    let rest = parts[1];

    // Parse "N, col M, message"
    let mut line_num = 0usize;
    let mut col = None;
    let mut message = String::new();

    // Split by ", col "
    if let Some(col_pos) = rest.find(", col ") {
        if let Ok(n) = rest[..col_pos].parse::<usize>() {
            line_num = n;
        }

        let after_col = &rest[col_pos + 6..];
        // Split by ", " to get column and message
        if let Some(msg_pos) = after_col.find(", ") {
            if let Ok(c) = after_col[..msg_pos].parse::<usize>() {
                col = Some(c);
            }
            message = after_col[msg_pos + 2..].to_string();
        }
    }

    if line_num == 0 {
        return None;
    }

    // Make path relative
    let file_path = PathBuf::from(file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());

    Some(LintIssue {
        file: relative_file,
        line: line_num,
        column: col,
        end_line: None,
        end_column: None,
        severity: LintSeverity::Error, // JSON syntax errors are always errors
        rule: "syntax".to_string(),
        message,
        linter: Linter::JsonLint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_jsonlint_output() {
        let output = "config.json: line 5, col 3, found: 'EOF' - expected: '}'.";

        let issues = parse_jsonlint_output("", output, Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].file, "config.json");
        assert_eq!(issues[0].line, 5);
        assert_eq!(issues[0].column, Some(3));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert!(issues[0].message.contains("EOF"));
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_jsonlint_output("", "", Path::new("."));
        assert!(issues.is_empty());
    }
}
