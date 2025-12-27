//! Shell linter detection (shellcheck, shfmt).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_shellcheck_version, get_shfmt_version, is_command_available};

/// Detect shell linters in the project
pub fn detect_shell_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut detected = Vec::new();

    // Check for ShellCheck
    if is_command_available("shellcheck") {
        let version = get_shellcheck_version();

        // Check for .shellcheckrc config
        let config_path = if dir.join(".shellcheckrc").exists() {
            Some(dir.join(".shellcheckrc"))
        } else {
            None
        };

        detected.push(DetectedLinter {
            linter: Linter::ShellCheck,
            config_path,
            version,
            available: true,
            run_command: LintCommand::new(
                "shellcheck",
                vec![
                    "--format=json".to_string(),
                    "--shell=bash".to_string(),
                    // Files will be added by the runner based on glob
                ],
            ),
            capabilities: LintCapabilities {
                can_fix: false, // ShellCheck can suggest fixes but not apply them
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // Check for shfmt
    if is_command_available("shfmt") {
        let version = get_shfmt_version();

        // Check for .editorconfig (shfmt uses it)
        let config_path = if dir.join(".editorconfig").exists() {
            Some(dir.join(".editorconfig"))
        } else {
            None
        };

        detected.push(DetectedLinter {
            linter: Linter::Shfmt,
            config_path,
            version,
            available: true,
            run_command: LintCommand::new(
                "shfmt",
                vec![
                    "-d".to_string(), // diff mode
                    "-l".to_string(), // list files that differ
                    ".".to_string(),
                ],
            )
            .with_fix_args(vec![
                "-w".to_string(), // write mode
                ".".to_string(),
            ]),
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    detected
}

/// Check if directory has shell script configuration
#[allow(dead_code)]
pub fn has_shell_config(dir: &Path) -> bool {
    dir.join(".shellcheckrc").exists()
}
