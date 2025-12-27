//! Go linter detection (golangci-lint, gofmt, go vet).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_go_version, get_golangci_version, is_command_available};

/// Detect Go linters
pub fn detect_go_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // golangci-lint
    let has_golangci_config = dir.join(".golangci.yml").exists()
        || dir.join(".golangci.yaml").exists()
        || dir.join(".golangci.toml").exists()
        || dir.join(".golangci.json").exists();

    if has_golangci_config || is_command_available("golangci-lint") {
        linters.push(DetectedLinter {
            linter: Linter::GolangciLint,
            config_path: if dir.join(".golangci.yml").exists() {
                Some(dir.join(".golangci.yml"))
            } else if dir.join(".golangci.yaml").exists() {
                Some(dir.join(".golangci.yaml"))
            } else {
                None
            },
            version: get_golangci_version(),
            available: is_command_available("golangci-lint"),
            run_command: LintCommand {
                program: "golangci-lint".to_string(),
                args: vec![
                    "run".to_string(),
                    "--out-format".to_string(),
                    "json".to_string(),
                ],
                fix_args: Some(vec!["run".to_string(), "--fix".to_string()]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // gofmt - always available if Go is installed
    if is_command_available("gofmt") {
        linters.push(DetectedLinter {
            linter: Linter::Gofmt,
            config_path: Some(dir.join("go.mod")),
            version: get_go_version(),
            available: true,
            run_command: LintCommand {
                program: "gofmt".to_string(),
                args: vec!["-l".to_string(), ".".to_string()],
                fix_args: Some(vec!["-w".to_string(), ".".to_string()]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    // go vet - always available if Go is installed
    if is_command_available("go") {
        linters.push(DetectedLinter {
            linter: Linter::GoVet,
            config_path: Some(dir.join("go.mod")),
            version: get_go_version(),
            available: true,
            run_command: LintCommand {
                program: "go".to_string(),
                args: vec!["vet".to_string(), "./...".to_string()],
                fix_args: None, // go vet doesn't auto-fix
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
