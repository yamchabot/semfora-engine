//! Terraform linter detection (tflint, terraform validate, terraform fmt).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_terraform_version, get_tflint_version, is_command_available};

/// Detect Terraform linters in the project
pub fn detect_terraform_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut detected = Vec::new();

    // Check for TFLint
    if is_command_available("tflint") {
        let version = get_tflint_version();

        // Check for .tflint.hcl config
        let config_path = if dir.join(".tflint.hcl").exists() {
            Some(dir.join(".tflint.hcl"))
        } else {
            None
        };

        detected.push(DetectedLinter {
            linter: Linter::TfLint,
            config_path,
            version,
            available: true,
            run_command: LintCommand::new(
                "tflint",
                vec![
                    "--format=json".to_string(),
                    "--recursive".to_string(),
                    ".".to_string(),
                ],
            )
            .with_fix_args(vec![
                "--fix".to_string(),
                "--recursive".to_string(),
                ".".to_string(),
            ]),
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // Check for Terraform (validate and fmt)
    if is_command_available("terraform") {
        let version = get_terraform_version();

        // Terraform validate
        detected.push(DetectedLinter {
            linter: Linter::TerraformValidate,
            config_path: None,
            version: version.clone(),
            available: true,
            run_command: LintCommand::new(
                "terraform",
                vec!["validate".to_string(), "-json".to_string()],
            ),
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });

        // Terraform fmt
        detected.push(DetectedLinter {
            linter: Linter::TerraformFmt,
            config_path: None,
            version,
            available: true,
            run_command: LintCommand::new(
                "terraform",
                vec![
                    "fmt".to_string(),
                    "-check".to_string(),
                    "-recursive".to_string(),
                    "-diff".to_string(),
                ],
            )
            .with_fix_args(vec!["fmt".to_string(), "-recursive".to_string()]),
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    detected
}

/// Check if directory has Terraform configuration
#[allow(dead_code)]
pub fn has_terraform_config(dir: &Path) -> bool {
    dir.join(".tflint.hcl").exists()
}
