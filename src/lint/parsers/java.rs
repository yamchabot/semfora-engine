//! Java linter output parsers (Checkstyle, SpotBugs, PMD).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse Checkstyle XML output
pub fn parse_checkstyle_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Checkstyle outputs XML format by default
    // <file name="..."><error line="..." column="..." severity="..." message="..." source="..."/></file>
    let content = if stdout.contains("<checkstyle") {
        stdout
    } else {
        stderr
    };

    // Simple XML parsing - look for error elements
    for line in content.lines() {
        let line = line.trim();

        // Extract file name from <file name="...">
        if line.starts_with("<file name=") {
            // We'll handle this through the error parsing
            continue;
        }

        // Parse error lines
        if line.starts_with("<error") || line.contains("<error ") {
            // Extract attributes using simple parsing
            let extract_attr = |attr: &str| -> Option<String> {
                let prefix = format!("{}=\"", attr);
                line.find(&prefix).and_then(|start| {
                    let value_start = start + prefix.len();
                    line[value_start..]
                        .find('"')
                        .map(|end| line[value_start..value_start + end].to_string())
                })
            };

            let line_num = extract_attr("line")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let column = extract_attr("column").and_then(|s| s.parse().ok());
            let severity_str = extract_attr("severity").unwrap_or_default();
            let message = extract_attr("message").unwrap_or_default();
            let source = extract_attr("source").unwrap_or_else(|| "checkstyle".to_string());

            let severity = match severity_str.as_str() {
                "error" => LintSeverity::Error,
                "warning" => LintSeverity::Warning,
                "info" => LintSeverity::Info,
                _ => LintSeverity::Warning,
            };

            // Try to get file from parent context (simplified - use source)
            let rule = source
                .rsplit('.')
                .next()
                .unwrap_or("checkstyle")
                .to_string();

            issues.push(LintIssue {
                file: "".to_string(), // File context not easily available in line-by-line parse
                line: line_num,
                column,
                end_line: None,
                end_column: None,
                severity,
                rule,
                message: html_decode(&message),
                linter: Linter::Checkstyle,
                fix: None,
            });
        }
    }

    // For Gradle output, parse console format: [severity] file:line:col: message
    if issues.is_empty() {
        for line in content.lines() {
            // Format: [WARN] /path/to/File.java:10:5: Message [RuleName]
            if line.starts_with("[WARN]") || line.starts_with("[ERROR]") {
                let severity = if line.starts_with("[ERROR]") {
                    LintSeverity::Error
                } else {
                    LintSeverity::Warning
                };

                let rest = line
                    .trim_start_matches("[WARN]")
                    .trim_start_matches("[ERROR]")
                    .trim();

                // Parse file:line:col: message [Rule]
                if let Some(colon_pos) = rest.find(':') {
                    let file = &rest[..colon_pos];
                    let file_path = PathBuf::from(file);
                    let relative_file = file_path
                        .strip_prefix(dir)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| file.to_string());

                    let remainder = &rest[colon_pos + 1..];
                    let parts: Vec<&str> = remainder.splitn(3, ':').collect();

                    if parts.len() >= 2 {
                        let line_num = parts[0].parse().unwrap_or(1);
                        let message = parts.get(2).unwrap_or(&"").trim();

                        // Extract rule from [RuleName] at end
                        let (msg, rule) = if let Some(bracket_start) = message.rfind('[') {
                            (
                                message[..bracket_start].trim(),
                                message[bracket_start + 1..]
                                    .trim_end_matches(']')
                                    .to_string(),
                            )
                        } else {
                            (message, "checkstyle".to_string())
                        };

                        issues.push(LintIssue {
                            file: relative_file,
                            line: line_num,
                            column: parts.get(1).and_then(|s| s.parse().ok()),
                            end_line: None,
                            end_column: None,
                            severity,
                            rule,
                            message: msg.to_string(),
                            linter: Linter::Checkstyle,
                            fix: None,
                        });
                    }
                }
            }
        }
    }

    issues
}

/// Parse SpotBugs XML output
pub fn parse_spotbugs_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    let content = if stdout.contains("<BugCollection") || stdout.contains("<BugInstance") {
        stdout
    } else {
        stderr
    };

    // Parse SpotBugs XML format
    // <BugInstance type="..." priority="..." category="...">
    //   <SourceLine classname="..." start="..." end="..." sourcefile="..." sourcepath="..."/>
    //   <ShortMessage>...</ShortMessage>
    // </BugInstance>

    let mut current_bug_type = String::new();
    let mut current_priority = 2u8;
    let mut current_message = String::new();

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("<BugInstance") {
            // Extract type and priority
            let extract_attr = |attr: &str| -> Option<String> {
                let prefix = format!("{}=\"", attr);
                line.find(&prefix).and_then(|start| {
                    let value_start = start + prefix.len();
                    line[value_start..]
                        .find('"')
                        .map(|end| line[value_start..value_start + end].to_string())
                })
            };

            current_bug_type = extract_attr("type").unwrap_or_default();
            current_priority = extract_attr("priority")
                .and_then(|s| s.parse().ok())
                .unwrap_or(2);
        } else if line.contains("<ShortMessage>") {
            // Extract message between tags
            if let Some(start) = line.find('>') {
                if let Some(end) = line.rfind("</") {
                    current_message = line[start + 1..end].to_string();
                }
            }
        } else if line.starts_with("<SourceLine") {
            let extract_attr = |attr: &str| -> Option<String> {
                let prefix = format!("{}=\"", attr);
                line.find(&prefix).and_then(|start| {
                    let value_start = start + prefix.len();
                    line[value_start..]
                        .find('"')
                        .map(|end| line[value_start..value_start + end].to_string())
                })
            };

            let sourcefile = extract_attr("sourcefile").unwrap_or_default();
            let start_line = extract_attr("start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let end_line = extract_attr("end").and_then(|s| s.parse().ok());

            if !sourcefile.is_empty() && !current_bug_type.is_empty() {
                let file_path = PathBuf::from(&sourcefile);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(sourcefile);

                let severity = match current_priority {
                    1 => LintSeverity::Error,   // High priority
                    2 => LintSeverity::Warning, // Normal priority
                    _ => LintSeverity::Info,    // Low priority
                };

                let message = if current_message.is_empty() {
                    current_bug_type.clone()
                } else {
                    current_message.clone()
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line: start_line,
                    column: None,
                    end_line,
                    end_column: None,
                    severity,
                    rule: current_bug_type.clone(),
                    message,
                    linter: Linter::SpotBugs,
                    fix: None,
                });
            }
        } else if line.starts_with("</BugInstance") {
            // Reset for next bug
            current_bug_type.clear();
            current_message.clear();
            current_priority = 2;
        }
    }

    issues
}

/// Parse PMD JSON output
pub fn parse_pmd_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format first
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(files) = result.get("files").and_then(|f| f.as_array()) {
            for file_obj in files {
                let filename = file_obj
                    .get("filename")
                    .and_then(|f| f.as_str())
                    .unwrap_or("");
                let file_path = PathBuf::from(filename);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| filename.to_string());

                if let Some(violations) = file_obj.get("violations").and_then(|v| v.as_array()) {
                    for violation in violations {
                        let begin_line = violation
                            .get("beginline")
                            .and_then(|l| l.as_u64())
                            .unwrap_or(1) as usize;
                        let begin_column = violation
                            .get("begincolumn")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        let end_line = violation
                            .get("endline")
                            .and_then(|l| l.as_u64())
                            .map(|l| l as usize);
                        let end_column = violation
                            .get("endcolumn")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        let rule = violation
                            .get("rule")
                            .and_then(|r| r.as_str())
                            .unwrap_or("pmd");
                        let message = violation
                            .get("description")
                            .and_then(|m| m.as_str())
                            .unwrap_or("");
                        let priority = violation
                            .get("priority")
                            .and_then(|p| p.as_u64())
                            .unwrap_or(3);

                        let severity = match priority {
                            1 | 2 => LintSeverity::Error,
                            3 => LintSeverity::Warning,
                            _ => LintSeverity::Info,
                        };

                        issues.push(LintIssue {
                            file: relative_file.clone(),
                            line: begin_line,
                            column: begin_column,
                            end_line,
                            end_column,
                            severity,
                            rule: rule.to_string(),
                            message: message.to_string(),
                            linter: Linter::Pmd,
                            fix: None,
                        });
                    }
                }
            }
        }
        return issues;
    }

    // Fallback to text format: file:line: message
    for line in stdout.lines() {
        if line.contains(":") {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let file = parts[0].trim();
                let file_path = PathBuf::from(file);
                let relative_file = file_path
                    .strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let line_num = parts[1].trim().parse().unwrap_or(1);
                let message = parts[2].trim();

                issues.push(LintIssue {
                    file: relative_file,
                    line: line_num,
                    column: None,
                    end_line: None,
                    end_column: None,
                    severity: LintSeverity::Warning,
                    rule: "pmd".to_string(),
                    message: message.to_string(),
                    linter: Linter::Pmd,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Simple HTML entity decoding for XML output
pub fn html_decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
