//! YAML linter detection (yamllint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_yamllint_version, is_command_available};

/// Check if directory has yamllint config
fn has_yamllint_config(dir: &Path) -> bool {
    dir.join(".yamllint").exists()
        || dir.join(".yamllint.yaml").exists()
        || dir.join(".yamllint.yml").exists()
}

/// Get the yamllint config path if it exists
fn get_yamllint_config_path(dir: &Path) -> Option<std::path::PathBuf> {
    let configs = [".yamllint", ".yamllint.yaml", ".yamllint.yml"];

    for config in configs {
        let path = dir.join(config);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Detect YAML linters (yamllint)
pub fn detect_yaml_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // yamllint is typically installed via pip, not npm
    let yamllint_available = is_command_available("yamllint");

    // Detect if yamllint is installed and either has config or YAML files
    if yamllint_available && has_yamllint_config(dir) {
        linters.push(DetectedLinter {
            linter: Linter::YamlLint,
            config_path: get_yamllint_config_path(dir),
            version: get_yamllint_version(),
            available: true,
            run_command: LintCommand {
                program: "yamllint".to_string(),
                args: vec![
                    "--format".to_string(),
                    "parsable".to_string(),
                    ".".to_string(),
                ],
                fix_args: None, // yamllint doesn't auto-fix
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
