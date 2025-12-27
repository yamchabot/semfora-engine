//! JSON linter detection (jsonlint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_jsonlint_version, is_command_available};

/// Check if package.json has a specific dev dependency
fn has_npm_dev_dependency(dir: &Path, package: &str) -> bool {
    let package_json = dir.join("package.json");
    if !package_json.exists() {
        return false;
    }

    std::fs::read_to_string(package_json)
        .map(|content| {
            // Simple check - look for package in devDependencies or dependencies
            content.contains(&format!("\"{}\"", package))
        })
        .unwrap_or(false)
}

/// Detect JSON linters (jsonlint)
pub fn detect_json_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    let npx_available = is_command_available("npx");

    // jsonlint - detectable via npm dependency
    let has_jsonlint = has_npm_dev_dependency(dir, "jsonlint")
        || has_npm_dev_dependency(dir, "@prantlf/jsonlint");

    if npx_available && has_jsonlint {
        linters.push(DetectedLinter {
            linter: Linter::JsonLint,
            config_path: None, // jsonlint doesn't use config files
            version: get_jsonlint_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec![
                    "jsonlint".to_string(),
                    "--quiet".to_string(),
                    "**/*.json".to_string(),
                ],
                fix_args: None, // jsonlint doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    linters
}
