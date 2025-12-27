//! C/C++ linter output parsers (clang-tidy, cppcheck, cpplint).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse clang-tidy output
///
/// clang-tidy outputs in the format:
/// filename:line:column: severity: message [check-name]
///
/// Example:
/// src/main.cpp:10:5: warning: use nullptr [modernize-use-nullptr]
pub fn parse_clang_tidy_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // clang-tidy outputs diagnostics to stdout or stderr depending on configuration
    let output = if !stdout.is_empty() { stdout } else { stderr };

    for line in output.lines() {
        // Match pattern: file:line:column: severity: message [check-name]
        if let Some(issue) = parse_clang_diagnostic_line(line, dir, Linter::ClangTidy) {
            issues.push(issue);
        }
    }

    issues
}

/// Parse cppcheck output (using --template=gcc format)
///
/// Default template outputs:
/// filename:line: severity: message
///
/// With --template=gcc:
/// filename:line:column: severity: message [check-id]
pub fn parse_cppcheck_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // cppcheck outputs diagnostics to stderr
    let output = if !stderr.is_empty() { stderr } else { stdout };

    for line in output.lines() {
        // Skip progress messages and info lines
        if line.starts_with("Checking")
            || line.starts_with("nofile:")
            || line.contains("(information)")
            || line.is_empty()
        {
            continue;
        }

        // Parse gcc-style format: file:line:column: severity: message
        if let Some(issue) = parse_gcc_style_diagnostic(line, dir, Linter::Cppcheck) {
            issues.push(issue);
        }
    }

    issues
}

/// Parse cpplint output (using --output=eclipse format for easier parsing)
///
/// Eclipse format outputs:
/// filename:line:  message  [category/rule]
///
/// Standard format:
/// filename:line:  message  \[category/rule\] \[confidence\]
pub fn parse_cpplint_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // cpplint outputs to stderr
    let output = if !stderr.is_empty() { stderr } else { stdout };

    for line in output.lines() {
        // Skip empty lines and summary lines
        if line.is_empty() || line.starts_with("Done processing") || line.starts_with("Total errors") {
            continue;
        }

        // Parse cpplint format: file:line:  message  [category] [confidence]
        if let Some(issue) = parse_cpplint_line(line, dir) {
            issues.push(issue);
        }
    }

    issues
}

/// Parse a clang-style diagnostic line
/// Format: file:line:column: severity: message [check-name]
fn parse_clang_diagnostic_line(line: &str, dir: &Path, linter: Linter) -> Option<LintIssue> {
    // Split on first ": " to get file:line:column part
    let parts: Vec<&str> = line.splitn(2, ": ").collect();
    if parts.len() < 2 {
        return None;
    }

    let location = parts[0];
    let rest = parts[1];

    // Parse location (file:line:column or file:line)
    let loc_parts: Vec<&str> = location.split(':').collect();
    if loc_parts.len() < 2 {
        return None;
    }

    let file = loc_parts[0];
    let line_num: usize = loc_parts.get(1)?.parse().ok()?;
    let column: Option<usize> = loc_parts.get(2).and_then(|c| c.parse().ok());

    // Parse severity and message
    // Format: "severity: message [check-name]"
    let severity_parts: Vec<&str> = rest.splitn(2, ": ").collect();
    if severity_parts.is_empty() {
        return None;
    }

    let severity_str = severity_parts[0].trim();
    let message_with_check = severity_parts.get(1).unwrap_or(&"");

    // Extract check name from brackets at end
    let (message, rule) = if let Some(bracket_start) = message_with_check.rfind('[') {
        if let Some(bracket_end) = message_with_check.rfind(']') {
            let msg = message_with_check[..bracket_start].trim();
            let check = &message_with_check[bracket_start + 1..bracket_end];
            (msg, check.to_string())
        } else {
            (message_with_check.trim(), "unknown".to_string())
        }
    } else {
        (message_with_check.trim(), "unknown".to_string())
    };

    let severity = match severity_str {
        "error" | "fatal error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        "note" => LintSeverity::Info,
        _ => LintSeverity::Warning,
    };

    // Make path relative to dir
    let file_path = PathBuf::from(file);
    let relative_file = file_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());

    Some(LintIssue {
        file: relative_file,
        line: line_num,
        column,
        end_line: None,
        end_column: None,
        severity,
        rule,
        message: message.to_string(),
        linter,
        fix: None,
    })
}

/// Parse gcc-style diagnostic line (used by cppcheck with --template=gcc)
/// Format: file:line:column: severity: message
fn parse_gcc_style_diagnostic(line: &str, dir: &Path, linter: Linter) -> Option<LintIssue> {
    // Similar to clang format but may have different severity names
    // cppcheck uses: error, warning, style, performance, portability, information

    let parts: Vec<&str> = line.splitn(2, ": ").collect();
    if parts.len() < 2 {
        return None;
    }

    let location = parts[0];
    let rest = parts[1];

    // Parse location
    let loc_parts: Vec<&str> = location.split(':').collect();
    if loc_parts.len() < 2 {
        return None;
    }

    let file = loc_parts[0];
    let line_num: usize = loc_parts.get(1)?.parse().ok()?;
    let column: Option<usize> = loc_parts.get(2).and_then(|c| c.parse().ok());

    // Parse severity and message
    let severity_parts: Vec<&str> = rest.splitn(2, ": ").collect();
    if severity_parts.is_empty() {
        return None;
    }

    let severity_str = severity_parts[0].trim();
    let message_with_id = severity_parts.get(1).unwrap_or(&rest);

    // Extract check ID from brackets
    let (message, rule) = if let Some(bracket_start) = message_with_id.rfind('[') {
        if let Some(bracket_end) = message_with_id.rfind(']') {
            let msg = message_with_id[..bracket_start].trim();
            let check = &message_with_id[bracket_start + 1..bracket_end];
            (msg, check.to_string())
        } else {
            (message_with_id.trim(), severity_str.to_string())
        }
    } else {
        (message_with_id.trim(), severity_str.to_string())
    };

    let severity = match severity_str {
        "error" => LintSeverity::Error,
        "warning" => LintSeverity::Warning,
        "style" | "performance" | "portability" => LintSeverity::Info,
        _ => LintSeverity::Warning,
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
        column,
        end_line: None,
        end_column: None,
        severity,
        rule,
        message: message.to_string(),
        linter,
        fix: None,
    })
}

/// Parse cpplint output line
/// Format: file:line:  message  \[category/rule\] \[confidence\]
fn parse_cpplint_line(line: &str, dir: &Path) -> Option<LintIssue> {
    // Split on ":  " (note: two spaces) to separate location from message
    // But first extract file:line
    let first_colon = line.find(':')?;
    let file = &line[..first_colon];

    let rest = &line[first_colon + 1..];
    let second_colon = rest.find(':');

    let (line_num, message_part) = if let Some(colon_pos) = second_colon {
        let line_str = &rest[..colon_pos];
        let msg = &rest[colon_pos + 1..];
        (line_str.trim().parse::<usize>().ok()?, msg.trim())
    } else {
        // Try parsing the whole rest as line number + message
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() >= 2 {
            (parts[0].parse::<usize>().ok()?, parts[1].trim())
        } else {
            return None;
        }
    };

    // Extract category and confidence from brackets
    let (message, rule, confidence) = if let Some(bracket_start) = message_part.find('[') {
        let msg = message_part[..bracket_start].trim();
        let bracket_content = &message_part[bracket_start + 1..];

        // Find matching close bracket
        if let Some(bracket_end) = bracket_content.find(']') {
            let category = &bracket_content[..bracket_end];

            // Check for confidence level in second bracket
            let remaining = &bracket_content[bracket_end + 1..];
            let conf = if remaining.contains('[') {
                if let Some(conf_start) = remaining.find('[') {
                    if let Some(conf_end) = remaining[conf_start..].find(']') {
                        remaining[conf_start + 1..conf_start + conf_end]
                            .parse::<u8>()
                            .ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            (msg, category.to_string(), conf)
        } else {
            (message_part, "unknown".to_string(), None)
        }
    } else {
        (message_part, "unknown".to_string(), None)
    };

    // Map confidence to severity (cpplint uses 1-5, higher = more confident)
    let severity = match confidence {
        Some(5) => LintSeverity::Error,
        Some(4) | Some(3) => LintSeverity::Warning,
        _ => LintSeverity::Info,
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
        column: None,
        end_line: None,
        end_column: None,
        severity,
        rule,
        message: message.to_string(),
        linter: Linter::Cpplint,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_clang_tidy_output() {
        let output = r#"src/main.cpp:10:5: warning: use nullptr [modernize-use-nullptr]
src/util.cpp:25:10: error: variable 'x' is not initialized [cppcoreguidelines-init-variables]"#;

        let issues = parse_clang_tidy_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/main.cpp");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].rule, "modernize-use-nullptr");

        assert_eq!(issues[1].file, "src/util.cpp");
        assert_eq!(issues[1].line, 25);
        assert_eq!(issues[1].severity, LintSeverity::Error);
    }

    #[test]
    fn test_parse_cppcheck_output() {
        let stderr = r#"src/main.cpp:15:0: style: The scope of the variable 'result' can be reduced. [variableScope]
src/util.cpp:42:8: error: Null pointer dereference [nullPointer]"#;

        let issues = parse_cppcheck_output("", stderr, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/main.cpp");
        assert_eq!(issues[0].line, 15);
        assert_eq!(issues[0].severity, LintSeverity::Info); // style -> Info
        assert_eq!(issues[0].rule, "variableScope");

        assert_eq!(issues[1].severity, LintSeverity::Error);
        assert_eq!(issues[1].rule, "nullPointer");
    }

    #[test]
    fn test_parse_cpplint_output() {
        let stderr = r#"src/main.cpp:10:  Missing space after , [whitespace/comma] [3]
src/util.cpp:25:  Include the directory when naming header files [build/include_subdir] [4]"#;

        let issues = parse_cpplint_output("", stderr, Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/main.cpp");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].rule, "whitespace/comma");

        assert_eq!(issues[1].file, "src/util.cpp");
        assert_eq!(issues[1].line, 25);
    }
}
