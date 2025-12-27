//! HTML linter detection (HTMLHint, html-validate).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_html_validate_version, get_htmlhint_version, is_command_available};

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

/// Check if directory has HTMLHint config
fn has_htmlhint_config(dir: &Path) -> bool {
    dir.join(".htmlhintrc").exists()
        || dir.join(".htmlhintrc.json").exists()
        || dir.join("htmlhint.config.js").exists()
}

/// Check if directory has html-validate config
fn has_html_validate_config(dir: &Path) -> bool {
    dir.join(".htmlvalidate.json").exists()
        || dir.join(".htmlvalidate.js").exists()
        || dir.join(".htmlvalidate.cjs").exists()
        || dir.join("htmlvalidate.config.js").exists()
        || dir.join("htmlvalidate.config.mjs").exists()
}

/// Detect HTML linters (HTMLHint, html-validate)
pub fn detect_html_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    let npx_available = is_command_available("npx");

    // HTMLHint - detectable via config file or npm dependency
    let has_htmlhint = has_htmlhint_config(dir) || has_npm_dev_dependency(dir, "htmlhint");

    if npx_available && has_htmlhint {
        linters.push(DetectedLinter {
            linter: Linter::HtmlHint,
            config_path: if dir.join(".htmlhintrc").exists() {
                Some(dir.join(".htmlhintrc"))
            } else if dir.join(".htmlhintrc.json").exists() {
                Some(dir.join(".htmlhintrc.json"))
            } else if dir.join("htmlhint.config.js").exists() {
                Some(dir.join("htmlhint.config.js"))
            } else {
                None
            },
            version: get_htmlhint_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec![
                    "htmlhint".to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                    "**/*.html".to_string(),
                ],
                fix_args: None, // HTMLHint doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // html-validate - more comprehensive HTML validator
    let has_html_validate =
        has_html_validate_config(dir) || has_npm_dev_dependency(dir, "html-validate");

    if npx_available && has_html_validate {
        linters.push(DetectedLinter {
            linter: Linter::HtmlValidate,
            config_path: if dir.join(".htmlvalidate.json").exists() {
                Some(dir.join(".htmlvalidate.json"))
            } else if dir.join(".htmlvalidate.js").exists() {
                Some(dir.join(".htmlvalidate.js"))
            } else if dir.join(".htmlvalidate.cjs").exists() {
                Some(dir.join(".htmlvalidate.cjs"))
            } else {
                None
            },
            version: get_html_validate_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec![
                    "html-validate".to_string(),
                    "--formatter".to_string(),
                    "json".to_string(),
                    "**/*.html".to_string(),
                ],
                fix_args: None, // html-validate doesn't auto-fix
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
