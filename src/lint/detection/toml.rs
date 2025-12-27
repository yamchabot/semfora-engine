//! TOML linter detection (taplo).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_taplo_version, is_command_available};

/// Check if directory has taplo config
fn has_taplo_config(dir: &Path) -> bool {
    dir.join(".taplo.toml").exists() || dir.join("taplo.toml").exists()
}

/// Get the taplo config path if it exists
fn get_taplo_config_path(dir: &Path) -> Option<std::path::PathBuf> {
    if dir.join(".taplo.toml").exists() {
        Some(dir.join(".taplo.toml"))
    } else if dir.join("taplo.toml").exists() {
        Some(dir.join("taplo.toml"))
    } else {
        None
    }
}

/// Detect TOML linters (taplo)
pub fn detect_toml_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // taplo can be installed via cargo or npm
    let taplo_available = is_command_available("taplo");
    let npx_available = is_command_available("npx");

    // Detect if taplo is installed and either has config or TOML files exist
    if taplo_available || npx_available {
        // Only detect if there's a config file or this is a Rust project (has Cargo.toml)
        if has_taplo_config(dir) || dir.join("Cargo.toml").exists() {
            let (program, mut args) = if taplo_available {
                ("taplo".to_string(), vec!["check".to_string()])
            } else {
                (
                    "npx".to_string(),
                    vec!["@taplo/cli".to_string(), "check".to_string()],
                )
            };

            // Add output format
            args.push("--output-format".to_string());
            args.push("json".to_string());

            let fix_args = if taplo_available {
                Some(vec!["taplo".to_string(), "fmt".to_string()])
            } else {
                Some(vec![
                    "npx".to_string(),
                    "@taplo/cli".to_string(),
                    "fmt".to_string(),
                ])
            };

            linters.push(DetectedLinter {
                linter: Linter::Taplo,
                config_path: get_taplo_config_path(dir),
                version: get_taplo_version(),
                available: true,
                run_command: LintCommand {
                    program,
                    args,
                    fix_args,
                    cwd: None,
                },
                capabilities: LintCapabilities {
                    can_fix: true,
                    can_format: true,
                    can_typecheck: false,
                },
            });
        }
    }

    linters
}
