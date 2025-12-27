//! C#/.NET linter output parsers (dotnet format, Roslyn analyzers, StyleCop).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse dotnet format output
///
/// dotnet format outputs in MSBuild diagnostic format:
/// file(line,column): severity code: message
///
/// Example:
/// Program.cs(10,5): error IDE0001: Simplify name
pub fn parse_dotnet_format_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // dotnet format outputs diagnostics to both stdout and stderr
    for line in stdout.lines().chain(stderr.lines()) {
        if let Some(issue) = parse_msbuild_diagnostic_line(line, dir, Linter::DotnetFormat) {
            issues.push(issue);
        }
    }

    issues
}

/// Parse Roslyn analyzers output (via dotnet build)
///
/// Roslyn outputs in MSBuild diagnostic format:
/// path/file.cs(line,column): warning CA2000: message
pub fn parse_roslyn_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // dotnet build outputs to both stdout and stderr
    for line in stdout.lines().chain(stderr.lines()) {
        if let Some(issue) = parse_msbuild_diagnostic_line(line, dir, Linter::RoslynAnalyzers) {
            issues.push(issue);
        }
    }

    issues
}

/// Parse StyleCop output (via dotnet build with StyleCop.Analyzers)
///
/// StyleCop outputs in MSBuild diagnostic format:
/// file.cs(line,column): warning SA1000: message
pub fn parse_stylecop_output(stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Filter to only SA* rules (StyleCop rules)
    for line in stdout.lines().chain(stderr.lines()) {
        if let Some(issue) = parse_msbuild_diagnostic_line(line, dir, Linter::StyleCop) {
            // Only include StyleCop rules (SA prefix)
            if issue.rule.starts_with("SA") {
                issues.push(issue);
            }
        }
    }

    issues
}

/// Parse MSBuild-style diagnostic line
///
/// Format: file(line,column): severity code: message
/// Or: file(line,column,endline,endcolumn): severity code: message
///
/// Examples:
/// - Program.cs(10,5): error CS0001: Some error
/// - src/Util.cs(25,10,25,20): warning CA2000: Call Dispose
/// - Models/User.cs(5,1): info IDE0001: Simplify name
fn parse_msbuild_diagnostic_line(line: &str, dir: &Path, linter: Linter) -> Option<LintIssue> {
    // Skip lines that don't look like diagnostics
    if !line.contains("): ") || (!line.contains(": error ") && !line.contains(": warning ") && !line.contains(": info ")) {
        return None;
    }

    // Find the location part: file(line,column)
    let paren_open = line.find('(')?;
    let paren_close = line.find("): ")?;

    let file = &line[..paren_open];
    let location = &line[paren_open + 1..paren_close];
    let rest = &line[paren_close + 3..]; // Skip "): "

    // Parse location (line,column or line,column,endline,endcolumn)
    let loc_parts: Vec<&str> = location.split(',').collect();
    if loc_parts.is_empty() {
        return None;
    }

    let line_num: usize = loc_parts.first()?.parse().ok()?;
    let column: Option<usize> = loc_parts.get(1).and_then(|c| c.parse().ok());
    let end_line: Option<usize> = loc_parts.get(2).and_then(|c| c.parse().ok());
    let end_column: Option<usize> = loc_parts.get(3).and_then(|c| c.parse().ok());

    // Parse severity and message
    // Format: "severity code: message" or just "severity: message"
    let (severity, rest_after_severity) = if rest.starts_with("error ") {
        (LintSeverity::Error, &rest[6..])
    } else if rest.starts_with("warning ") {
        (LintSeverity::Warning, &rest[8..])
    } else if rest.starts_with("info ") {
        (LintSeverity::Info, &rest[5..])
    } else if rest.starts_with("hint ") {
        (LintSeverity::Hint, &rest[5..])
    } else {
        // Unknown severity, treat as warning
        (LintSeverity::Warning, rest)
    };

    // Extract rule code and message
    // Format: "CODE: message" or just "message"
    let (rule, message) = if let Some(colon_pos) = rest_after_severity.find(": ") {
        let code = &rest_after_severity[..colon_pos];
        let msg = &rest_after_severity[colon_pos + 2..];
        (code.trim().to_string(), msg.trim().to_string())
    } else {
        ("unknown".to_string(), rest_after_severity.trim().to_string())
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
        end_line,
        end_column,
        severity,
        rule,
        message,
        linter,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_dotnet_format_output() {
        let output = r#"Program.cs(10,5): error IDE0001: Simplify name
src/Util.cs(25,10): warning IDE0055: Fix formatting"#;

        let issues = parse_dotnet_format_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "Program.cs");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule, "IDE0001");
        assert!(issues[0].message.contains("Simplify name"));

        assert_eq!(issues[1].file, "src/Util.cs");
        assert_eq!(issues[1].line, 25);
        assert_eq!(issues[1].severity, LintSeverity::Warning);
        assert_eq!(issues[1].rule, "IDE0055");
    }

    #[test]
    fn test_parse_roslyn_output() {
        let output = r#"src/Service.cs(15,8): warning CA2000: Call System.IDisposable.Dispose on object
Models/User.cs(42,1): error CS0246: The type or namespace 'Foo' could not be found"#;

        let issues = parse_roslyn_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].file, "src/Service.cs");
        assert_eq!(issues[0].line, 15);
        assert_eq!(issues[0].rule, "CA2000");

        assert_eq!(issues[1].file, "Models/User.cs");
        assert_eq!(issues[1].severity, LintSeverity::Error);
        assert_eq!(issues[1].rule, "CS0246");
    }

    #[test]
    fn test_parse_stylecop_output() {
        let output = r#"Program.cs(1,1): warning SA1633: File should have header
src/Util.cs(10,5): warning SA1101: Prefix local calls with this
src/Util.cs(15,1): warning CA1000: Consider making static"#;

        let issues = parse_stylecop_output(output, "", Path::new("."));
        // Only SA rules should be included
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].rule, "SA1633");
        assert_eq!(issues[1].rule, "SA1101");
    }

    #[test]
    fn test_parse_msbuild_with_range() {
        let output = "Program.cs(10,5,10,20): warning IDE0001: Simplify name";

        let issues = parse_dotnet_format_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);

        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, Some(5));
        assert_eq!(issues[0].end_line, Some(10));
        assert_eq!(issues[0].end_column, Some(20));
    }

    #[test]
    fn test_skip_non_diagnostic_lines() {
        let output = r#"Build started...
Determining projects to restore...
All projects are up-to-date for restore.
Program.cs(10,5): warning IDE0001: Simplify name
Build succeeded."#;

        let issues = parse_dotnet_format_output(output, "", Path::new("."));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].rule, "IDE0001");
    }
}
