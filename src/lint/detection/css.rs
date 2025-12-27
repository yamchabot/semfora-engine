//! CSS/SCSS/SASS linter detection (Stylelint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_stylelint_version, is_command_available};

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

/// Check if directory has Stylelint config
fn has_stylelint_config(dir: &Path) -> bool {
    // Stylelint supports many config file formats
    dir.join(".stylelintrc").exists()
        || dir.join(".stylelintrc.json").exists()
        || dir.join(".stylelintrc.yaml").exists()
        || dir.join(".stylelintrc.yml").exists()
        || dir.join(".stylelintrc.js").exists()
        || dir.join(".stylelintrc.cjs").exists()
        || dir.join(".stylelintrc.mjs").exists()
        || dir.join("stylelint.config.js").exists()
        || dir.join("stylelint.config.cjs").exists()
        || dir.join("stylelint.config.mjs").exists()
        || has_stylelint_in_package_json(dir)
}

/// Check if stylelint config is in package.json
fn has_stylelint_in_package_json(dir: &Path) -> bool {
    let package_json = dir.join("package.json");
    if !package_json.exists() {
        return false;
    }

    std::fs::read_to_string(package_json)
        .map(|content| content.contains("\"stylelint\""))
        .unwrap_or(false)
}

/// Get the Stylelint config path if it exists
fn get_stylelint_config_path(dir: &Path) -> Option<std::path::PathBuf> {
    let configs = [
        ".stylelintrc",
        ".stylelintrc.json",
        ".stylelintrc.yaml",
        ".stylelintrc.yml",
        ".stylelintrc.js",
        ".stylelintrc.cjs",
        ".stylelintrc.mjs",
        "stylelint.config.js",
        "stylelint.config.cjs",
        "stylelint.config.mjs",
    ];

    for config in configs {
        let path = dir.join(config);
        if path.exists() {
            return Some(path);
        }
    }

    // Check if config is in package.json
    if has_stylelint_in_package_json(dir) {
        return Some(dir.join("package.json"));
    }

    None
}

/// Detect CSS/SCSS/SASS linters (Stylelint)
pub fn detect_css_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    let npx_available = is_command_available("npx");

    // Stylelint - detectable via config file or npm dependency
    let has_stylelint = has_stylelint_config(dir) || has_npm_dev_dependency(dir, "stylelint");

    if npx_available && has_stylelint {
        linters.push(DetectedLinter {
            linter: Linter::Stylelint,
            config_path: get_stylelint_config_path(dir),
            version: get_stylelint_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec![
                    "stylelint".to_string(),
                    "--formatter".to_string(),
                    "json".to_string(),
                    "**/*.{css,scss,sass,less}".to_string(),
                ],
                fix_args: Some(vec![
                    "stylelint".to_string(),
                    "--fix".to_string(),
                    "**/*.{css,scss,sass,less}".to_string(),
                ]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    linters
}
