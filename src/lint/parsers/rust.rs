//! Rust linter output parsers (Clippy, Rustfmt).

use std::path::{Path, PathBuf};

use crate::lint::types::{LintIssue, LintSeverity, Linter};

/// Parse clippy JSON output (cargo clippy --message-format=json)
pub fn parse_clippy_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON message
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            // Only process compiler messages
            if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
                continue;
            }

            if let Some(message) = msg.get("message") {
                let level = message
                    .get("level")
                    .and_then(|l| l.as_str())
                    .unwrap_or("warning");
                let text = message
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                let code = message
                    .get("code")
                    .and_then(|c| c.get("code"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("unknown");

                // Skip certain message types
                if level == "note" || level == "help" || text.starts_with("aborting") {
                    continue;
                }

                // Get primary span
                if let Some(spans) = message.get("spans").and_then(|s| s.as_array()) {
                    for span in spans {
                        if span.get("is_primary").and_then(|p| p.as_bool()) != Some(true) {
                            continue;
                        }

                        let file = span.get("file_name").and_then(|f| f.as_str()).unwrap_or("");
                        let line =
                            span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                        let column = span
                            .get("column_start")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        let end_line = span
                            .get("line_end")
                            .and_then(|l| l.as_u64())
                            .map(|l| l as usize);
                        let end_column = span
                            .get("column_end")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        let suggested_replacement =
                            span.get("suggested_replacement").and_then(|s| s.as_str());

                        // Make path relative to dir
                        let file_path = PathBuf::from(file);
                        let relative_file = file_path
                            .strip_prefix(dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| file.to_string());

                        let severity = match level {
                            "error" => LintSeverity::Error,
                            "warning" => LintSeverity::Warning,
                            _ => LintSeverity::Info,
                        };

                        issues.push(LintIssue {
                            file: relative_file,
                            line,
                            column,
                            end_line,
                            end_column,
                            severity,
                            rule: code.to_string(),
                            message: text.to_string(),
                            linter: Linter::Clippy,
                            fix: suggested_replacement.map(|s| s.to_string()),
                        });

                        break; // Only process first primary span
                    }
                }
            }
        }
    }

    issues
}

/// Parse rustfmt output (cargo fmt --check)
pub fn parse_rustfmt_output(_stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // rustfmt outputs "Diff in <file>" messages
    for line in stderr.lines() {
        if line.starts_with("Diff in ") {
            let file = line.trim_start_matches("Diff in ").trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            issues.push(LintIssue {
                file: relative_file,
                line: 1,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "formatting".to_string(),
                message: "File needs formatting".to_string(),
                linter: Linter::Rustfmt,
                fix: Some("Run 'cargo fmt' to fix".to_string()),
            });
        }
    }

    issues
}
