//! XML linter output parsers (xmllint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse xmllint output
///
/// xmllint --noout outputs errors like:
/// ```text
/// config.xml:10: parser error : Opening and ending tag mismatch: div line 5 and span
/// config.xml:15: parser error : Premature end of data in tag root line 1
/// ```
pub fn parse_xmllint_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // xmllint outputs errors to stderr
    let output = if stderr.is_empty() { stdout } else { stderr };

    for line in output.lines() {
        if let Some(issue) = parse_xmllint_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

fn parse_xmllint_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Format: "file:line: error type : message"
    // Or: "file:line: parser error : message"
    // Or: "file:line: validity error : message"

    // Skip empty lines and header lines
    if line.trim().is_empty() || line.starts_with('^') {
        return None;
    }

    // Split by first colon to get file
    let parts: Vec<&str> = line.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }

    let file = parts[0].trim();
    let line_num: usize = match parts[1].trim().parse() {
        Ok(n) => n,
        Err(_) => return None,
    };
    let rest = parts[2].trim();

    // Parse "error type : message"
    let (severity, message) = if rest.contains("parser error") {
        let msg = rest
            .strip_prefix("parser error :")
            .or_else(|| rest.strip_prefix(" parser error :"))
            .unwrap_or(rest)
            .trim();
        (LintSeverity::Error, msg.to_string())
    } else if rest.contains("validity error") {
        let msg = rest
            .strip_prefix("validity error :")
            .or_else(|| rest.strip_prefix(" validity error :"))
            .unwrap_or(rest)
            .trim();
        (LintSeverity::Error, msg.to_string())
    } else if rest.contains("warning") {
        let msg = rest
            .strip_prefix("warning :")
            .or_else(|| rest.strip_prefix(" warning :"))
            .unwrap_or(rest)
            .trim();
        (LintSeverity::Warning, msg.to_string())
    } else {
        // Unknown format, treat as error
        (LintSeverity::Error, rest.to_string())
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
        column: None, // xmllint doesn't provide column info
        end_line: None,
        end_column: None,
        severity,
        rule: "xml-syntax".to_string(),
        message,
        linter: Linter::XmlLint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_xmllint_output() {
        let output = r#"config.xml:10: parser error : Opening and ending tag mismatch: div line 5 and span
config.xml:15: parser error : Premature end of data in tag root line 1"#;

        let issues = parse_xmllint_output("", output, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "config.xml");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert!(issues[0].message.contains("tag mismatch"));

        assert_eq!(issues[1].line, 15);
        assert!(issues[1].message.contains("Premature end"));
    }

    #[test]
    fn test_parse_validity_error() {
        let output =
            "schema.xml:5: validity error : Element 'root': No matching global declaration";

        let issues = parse_xmllint_output("", output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_parse_empty_output() {
        let issues = parse_xmllint_output("", "", Path::new("."));
        assert!(issues.is_empty());
    }
}
