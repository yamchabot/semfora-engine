//! Markdown linter output parsers (markdownlint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse markdownlint JSON output
///
/// markdownlint --json outputs:
/// ```json
/// [
///   {
///     "fileName": "README.md",
///     "lineNumber": 10,
///     "ruleNames": ["MD012", "no-multiple-blanks"],
///     "ruleDescription": "Multiple consecutive blank lines",
///     "ruleInformation": "https://github.com/...",
///     "errorDetail": null,
///     "errorContext": null,
///     "errorRange": [1, 1]
///   }
/// ]
/// ```
pub fn parse_markdownlint_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format first
    let output = if stdout.is_empty() { stderr } else { stdout };

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(array) = json.as_array() {
            for item in array {
                if let Some(issue) = parse_markdownlint_json_item(item, dir) {
                    issues.push(issue);
                }
            }
            return issues;
        }
    }

    // Fall back to text format: file:line:column rule message
    // or: file:line rule message
    for line in output.lines() {
        if let Some(issue) = parse_markdownlint_text_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

fn parse_markdownlint_json_item(item: &serde_json::Value, dir: &Path) -> Option<LintIssue> {
    let file = item.get("fileName")?.as_str()?.to_string();
    let line = item.get("lineNumber")?.as_u64()? as usize;

    // Get column from errorRange if available
    let column = item
        .get("errorRange")
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.as_u64())
        .map(|c| c as usize);

    // Get rule name from ruleNames array
    let rule = item
        .get("ruleNames")
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .and_then(|r| r.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = item
        .get("ruleDescription")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    let detail = item
        .get("errorDetail")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    let context = item
        .get("errorContext")
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let message = if !detail.is_empty() {
        format!("{}: {}", description, detail)
    } else if !context.is_empty() {
        format!("{} [{}]", description, context)
    } else {
        description.to_string()
    };

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
        end_line: None,
        end_column: None,
        severity: LintSeverity::Warning, // markdownlint issues are warnings
        rule,
        message,
        linter: Linter::MarkdownLint,
        fix: None,
    })
}

fn parse_markdownlint_text_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Format: "file:line:column rule message" or "file:line rule message"
    // Example: "README.md:10:1 MD012/no-multiple-blanks Multiple consecutive blank lines"

    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let location = parts[0];
    let rest = parts[1];

    // Parse location (file:line or file:line:column)
    let loc_parts: Vec<&str> = location.split(':').collect();
    if loc_parts.len() < 2 {
        return None;
    }

    let file = loc_parts[0].to_string();
    let line_num: usize = loc_parts[1].parse().ok()?;
    let column = if loc_parts.len() > 2 {
        loc_parts[2].parse().ok()
    } else {
        None
    };

    // Parse rule and message
    let rule_parts: Vec<&str> = rest.splitn(2, ' ').collect();
    let rule = rule_parts[0].to_string();
    let message = if rule_parts.len() > 1 {
        rule_parts[1].to_string()
    } else {
        rule.clone()
    };

    // Make path relative
    let file_path = PathBuf::from(&file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(file);

    Some(LintIssue {
        file: relative_file,
        line: line_num,
        column,
        end_line: None,
        end_column: None,
        severity: LintSeverity::Warning,
        rule,
        message,
        linter: Linter::MarkdownLint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_markdownlint_json_output() {
        let output = r#"[
            {
                "fileName": "README.md",
                "lineNumber": 10,
                "ruleNames": ["MD012", "no-multiple-blanks"],
                "ruleDescription": "Multiple consecutive blank lines",
                "ruleInformation": "https://example.com",
                "errorDetail": null,
                "errorContext": null,
                "errorRange": [1, 1]
            }
        ]"#;

        let issues = parse_markdownlint_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "README.md");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(1));
        assert_eq!(issues[0].rule, "MD012");
        assert!(issues[0].message.contains("blank lines"));
    }

    #[test]
    fn test_parse_markdownlint_text_output() {
        let output = "README.md:10:1 MD012/no-multiple-blanks Multiple consecutive blank lines";

        let issues = parse_markdownlint_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "README.md");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(1));
        assert_eq!(issues[0].rule, "MD012/no-multiple-blanks");
    }

    #[test]
    fn test_parse_markdownlint_with_error_detail() {
        let output = r#"[
            {
                "fileName": "doc.md",
                "lineNumber": 5,
                "ruleNames": ["MD013", "line-length"],
                "ruleDescription": "Line length",
                "errorDetail": "Expected: 80; Actual: 120",
                "errorContext": null,
                "errorRange": null
            }
        ]"#;

        let issues = parse_markdownlint_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("Expected: 80"));
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_markdownlint_output("", "", Path::new("."));
        assert!(issues.is_empty());

        let issues = parse_markdownlint_output("[]", "", Path::new("."));
        assert!(issues.is_empty());
    }
}
