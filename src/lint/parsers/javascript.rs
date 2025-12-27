//! JavaScript/TypeScript linter output parsers (ESLint, Prettier, Biome, TSC).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse ESLint JSON output
pub fn parse_eslint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result
                .get("filePath")
                .and_then(|f| f.as_str())
                .unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    let line = msg.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                    let column = msg
                        .get("column")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);
                    let end_line = msg
                        .get("endLine")
                        .and_then(|l| l.as_u64())
                        .map(|l| l as usize);
                    let end_column = msg
                        .get("endColumn")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);
                    let severity_num = msg.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                    let rule = msg
                        .get("ruleId")
                        .and_then(|r| r.as_str())
                        .unwrap_or("unknown");
                    let message = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    let fix = msg.get("fix").map(|_| "Auto-fixable".to_string());

                    let severity = match severity_num {
                        2 => LintSeverity::Error,
                        1 => LintSeverity::Warning,
                        _ => LintSeverity::Info,
                    };

                    issues.push(LintIssue {
                        file: relative_file.clone(),
                        line,
                        column,
                        end_line,
                        end_column,
                        severity,
                        rule: rule.to_string(),
                        message: message.to_string(),
                        linter: Linter::ESLint,
                        fix,
                    });
                }
            }
        }
    }

    issues
}

/// Parse Prettier output
pub fn parse_prettier_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Prettier --check outputs files that need formatting
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Checking") || line.contains("checking") {
            continue;
        }

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
            linter: Linter::Prettier,
            fix: Some("Run 'npx prettier --write' to fix".to_string()),
        });
    }

    issues
}

/// Parse Biome output
pub fn parse_biome_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Biome outputs diagnostic messages in a custom format
    // For now, parse the human-readable output
    let mut current_file = String::new();

    for line in stdout.lines() {
        // Look for file paths
        if line.contains("┌─") {
            if let Some(path_part) = line.split("┌─").nth(1) {
                current_file = path_part.trim().to_string();
            }
            continue;
        }

        // Look for error/warning lines with line:col info
        if line.contains("error") || line.contains("warning") {
            if let Some(pos) = line.find(':') {
                let (loc, rest) = line.split_at(pos);
                if let Some((line_str, _col_str)) = loc.split_once(':') {
                    if let Ok(line_num) = line_str.trim().parse::<usize>() {
                        let file_path = PathBuf::from(&current_file);
                        let relative_file = file_path
                            .strip_prefix(dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| current_file.clone());

                        let severity = if line.contains("error") {
                            LintSeverity::Error
                        } else {
                            LintSeverity::Warning
                        };

                        issues.push(LintIssue {
                            file: relative_file,
                            line: line_num,
                            column: None,
                            end_line: None,
                            end_column: None,
                            severity,
                            rule: "biome".to_string(),
                            message: rest.trim_start_matches(':').trim().to_string(),
                            linter: Linter::Biome,
                            fix: None,
                        });
                    }
                }
            }
        }
    }

    issues
}

/// Parse TypeScript compiler output
pub fn parse_tsc_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // TSC outputs errors to stdout: file(line,col): error TS1234: message
    let re = regex::Regex::new(r"^(.+?)\((\d+),(\d+)\):\s*(error|warning)\s+(TS\d+):\s*(.+)$").ok();

    for line in stdout.lines() {
        if let Some(ref re) = re {
            if let Some(caps) = re.captures(line) {
                let file = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let line_num = caps
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(1);
                let col = caps.get(3).and_then(|m| m.as_str().parse().ok());
                let level = caps.get(4).map(|m| m.as_str()).unwrap_or("error");
                let code = caps.get(5).map(|m| m.as_str()).unwrap_or("");
                let message = caps.get(6).map(|m| m.as_str()).unwrap_or("");

                let file_path = PathBuf::from(file);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let severity = if level == "error" {
                    LintSeverity::Error
                } else {
                    LintSeverity::Warning
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line: line_num,
                    column: col,
                    end_line: None,
                    end_column: None,
                    severity,
                    rule: code.to_string(),
                    message: message.to_string(),
                    linter: Linter::Tsc,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Parse Oxlint JSON output
///
/// Oxlint outputs JSON in a format similar to ESLint:
/// [{ "filePath": "...", "messages": [{ "ruleId": "...", "severity": 1|2, "message": "...", ... }] }]
pub fn parse_oxlint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try parsing as JSON array (ESLint-compatible format)
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result
                .get("filePath")
                .and_then(|f| f.as_str())
                .unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    let line = msg.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                    let column = msg
                        .get("column")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);
                    let end_line = msg
                        .get("endLine")
                        .and_then(|l| l.as_u64())
                        .map(|l| l as usize);
                    let end_column = msg
                        .get("endColumn")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);
                    let severity_num = msg.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                    let rule = msg
                        .get("ruleId")
                        .and_then(|r| r.as_str())
                        .unwrap_or("unknown");
                    let message = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    let fix = msg.get("fix").map(|_| "Auto-fixable".to_string());

                    let severity = match severity_num {
                        2 => LintSeverity::Error,
                        1 => LintSeverity::Warning,
                        _ => LintSeverity::Info,
                    };

                    issues.push(LintIssue {
                        file: relative_file.clone(),
                        line,
                        column,
                        end_line,
                        end_column,
                        severity,
                        rule: rule.to_string(),
                        message: message.to_string(),
                        linter: Linter::Oxlint,
                        fix,
                    });
                }
            }
        }
        return issues;
    }

    // Fallback: parse text output (file:line:col: severity message)
    for line in stdout.lines() {
        // Skip empty lines and summary lines
        if line.is_empty() || line.starts_with("Found") || line.starts_with("×") {
            continue;
        }

        // Try to parse: file:line:col: message
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 {
            let file = parts[0].trim();
            if let Ok(line_num) = parts[1].trim().parse::<usize>() {
                let file_path = PathBuf::from(file);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let column = parts.get(2).and_then(|c| c.trim().parse().ok());
                let message = parts.get(3).map(|m| m.trim()).unwrap_or("");

                // Determine severity from message content
                let severity = if message.contains("error") {
                    LintSeverity::Error
                } else {
                    LintSeverity::Warning
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line: line_num,
                    column,
                    end_line: None,
                    end_column: None,
                    severity,
                    rule: "oxlint".to_string(),
                    message: message.to_string(),
                    linter: Linter::Oxlint,
                    fix: None,
                });
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_oxlint_json_output() {
        let output = r#"[{"filePath":"src/index.js","messages":[{"ruleId":"no-unused-vars","severity":2,"message":"'x' is defined but never used","line":5,"column":10}]}]"#;

        let issues = parse_oxlint_output(output, Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/index.js");
        assert_eq!(issues[0].line, 5);
        assert_eq!(issues[0].column, Some(10));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "no-unused-vars");
    }
}
