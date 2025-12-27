//! Rust linter detection (Clippy, rustfmt).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_clippy_version, get_rustfmt_version};

/// Detect Rust linters (clippy, rustfmt)
pub fn detect_rust_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Clippy - always available if Cargo.toml exists
    linters.push(DetectedLinter {
        linter: Linter::Clippy,
        config_path: Some(dir.join("Cargo.toml")),
        version: get_clippy_version(),
        available: true,
        run_command: LintCommand {
            program: "cargo".to_string(),
            args: vec![
                "clippy".to_string(),
                "--message-format=json".to_string(),
                "--".to_string(),
                "-D".to_string(),
                "warnings".to_string(),
            ],
            fix_args: Some(vec![
                "clippy".to_string(),
                "--fix".to_string(),
                "--allow-dirty".to_string(),
            ]),
            cwd: None,
        },
        capabilities: LintCapabilities {
            can_fix: true,
            can_format: false,
            can_typecheck: false,
        },
    });

    // Rustfmt
    linters.push(DetectedLinter {
        linter: Linter::Rustfmt,
        config_path: if dir.join("rustfmt.toml").exists() {
            Some(dir.join("rustfmt.toml"))
        } else if dir.join(".rustfmt.toml").exists() {
            Some(dir.join(".rustfmt.toml"))
        } else {
            None
        },
        version: get_rustfmt_version(),
        available: true,
        run_command: LintCommand {
            program: "cargo".to_string(),
            args: vec!["fmt".to_string(), "--check".to_string()],
            fix_args: Some(vec!["fmt".to_string()]),
            cwd: None,
        },
        capabilities: LintCapabilities {
            can_fix: true,
            can_format: true,
            can_typecheck: false,
        },
    });

    linters
}
