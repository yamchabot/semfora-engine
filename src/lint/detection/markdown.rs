//! Markdown linter detection (markdownlint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_markdownlint_version, is_command_available};

/// Detect markdown linters in the project
pub fn detect_markdown_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut detected = Vec::new();

    // Check for markdownlint configs
    let config_files = [
        ".markdownlint.json",
        ".markdownlint.yaml",
        ".markdownlint.yml",
        ".markdownlint-cli2.yaml",
        ".markdownlint-cli2.jsonc",
    ];

    let config_path = config_files
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.exists());

    // Check if markdownlint is available via npx (common for npm projects)
    let has_npm_project = dir.join("package.json").exists();
    let has_markdownlint_config = config_path.is_some();

    // Try markdownlint-cli2 first (newer, more features)
    let (use_cli2, version) = if is_command_available("npx") {
        let version = get_markdownlint_version();
        // Use CLI2 if the config suggests it or if available
        let use_cli2 = dir.join(".markdownlint-cli2.yaml").exists()
            || dir.join(".markdownlint-cli2.jsonc").exists();
        (use_cli2, version)
    } else {
        (false, None)
    };

    // Only detect if there's a config or npm project with potential markdown
    if has_markdownlint_config || (has_npm_project && version.is_some()) {
        if use_cli2 {
            detected.push(DetectedLinter {
                linter: Linter::MarkdownLint,
                config_path: config_path.clone(),
                version: version.clone(),
                available: true,
                run_command: LintCommand::new(
                    "npx",
                    vec![
                        "markdownlint-cli2".to_string(),
                        "**/*.md".to_string(),
                        "#node_modules".to_string(),
                    ],
                )
                .with_fix_args(vec![
                    "markdownlint-cli2".to_string(),
                    "--fix".to_string(),
                    "**/*.md".to_string(),
                    "#node_modules".to_string(),
                ]),
                capabilities: LintCapabilities {
                    can_fix: true,
                    can_format: false,
                    can_typecheck: false,
                },
            });
        } else {
            detected.push(DetectedLinter {
                linter: Linter::MarkdownLint,
                config_path,
                version,
                available: true,
                run_command: LintCommand::new(
                    "npx",
                    vec![
                        "markdownlint".to_string(),
                        "--json".to_string(),
                        "**/*.md".to_string(),
                        "--ignore".to_string(),
                        "node_modules".to_string(),
                    ],
                )
                .with_fix_args(vec![
                    "markdownlint".to_string(),
                    "--fix".to_string(),
                    "**/*.md".to_string(),
                    "--ignore".to_string(),
                    "node_modules".to_string(),
                ]),
                capabilities: LintCapabilities {
                    can_fix: true,
                    can_format: false,
                    can_typecheck: false,
                },
            });
        }
    }

    detected
}

/// Check if directory has markdownlint configuration
#[allow(dead_code)]
pub fn has_markdownlint_config(dir: &Path) -> bool {
    let config_files = [
        ".markdownlint.json",
        ".markdownlint.yaml",
        ".markdownlint.yml",
        ".markdownlint-cli2.yaml",
        ".markdownlint-cli2.jsonc",
    ];

    config_files.iter().any(|f| dir.join(f).exists())
}
