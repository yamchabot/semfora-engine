//! Python linter detection (Ruff, Black, Mypy, Pylint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{
    get_black_version, get_mypy_version, get_pylint_version, get_ruff_version,
    has_pyproject_section, is_command_available,
};

/// Detect Python linters
pub fn detect_python_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Check for Ruff
    let has_ruff_config = dir.join("ruff.toml").exists()
        || dir.join(".ruff.toml").exists()
        || has_pyproject_section(dir, "tool.ruff");

    if has_ruff_config || is_command_available("ruff") {
        linters.push(DetectedLinter {
            linter: Linter::Ruff,
            config_path: if dir.join("ruff.toml").exists() {
                Some(dir.join("ruff.toml"))
            } else if dir.join(".ruff.toml").exists() {
                Some(dir.join(".ruff.toml"))
            } else {
                None
            },
            version: get_ruff_version(),
            available: is_command_available("ruff"),
            run_command: LintCommand {
                program: "ruff".to_string(),
                args: vec![
                    "check".to_string(),
                    "--output-format".to_string(),
                    "json".to_string(),
                    ".".to_string(),
                ],
                fix_args: Some(vec![
                    "check".to_string(),
                    "--fix".to_string(),
                    ".".to_string(),
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

    // Check for Black
    let has_black_config = has_pyproject_section(dir, "tool.black");
    if has_black_config || is_command_available("black") {
        linters.push(DetectedLinter {
            linter: Linter::Black,
            config_path: None,
            version: get_black_version(),
            available: is_command_available("black"),
            run_command: LintCommand {
                program: "black".to_string(),
                args: vec!["--check".to_string(), ".".to_string()],
                fix_args: Some(vec![".".to_string()]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    // Check for Mypy
    let has_mypy_config = dir.join("mypy.ini").exists()
        || dir.join(".mypy.ini").exists()
        || has_pyproject_section(dir, "tool.mypy");

    if has_mypy_config || is_command_available("mypy") {
        linters.push(DetectedLinter {
            linter: Linter::Mypy,
            config_path: if dir.join("mypy.ini").exists() {
                Some(dir.join("mypy.ini"))
            } else if dir.join(".mypy.ini").exists() {
                Some(dir.join(".mypy.ini"))
            } else {
                None
            },
            version: get_mypy_version(),
            available: is_command_available("mypy"),
            run_command: LintCommand {
                program: "mypy".to_string(),
                args: vec!["--output".to_string(), "json".to_string(), ".".to_string()],
                fix_args: None, // Mypy doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: true,
            },
        });
    }

    // Check for Pylint
    let has_pylint_config = dir.join(".pylintrc").exists()
        || dir.join("pylintrc").exists()
        || dir.join("pyproject.toml").exists() && has_pyproject_section(dir, "tool.pylint");

    if has_pylint_config || is_command_available("pylint") {
        linters.push(DetectedLinter {
            linter: Linter::Pylint,
            config_path: if dir.join(".pylintrc").exists() {
                Some(dir.join(".pylintrc"))
            } else if dir.join("pylintrc").exists() {
                Some(dir.join("pylintrc"))
            } else {
                None
            },
            version: get_pylint_version(),
            available: is_command_available("pylint"),
            run_command: LintCommand {
                program: "pylint".to_string(),
                args: vec!["--output-format=json".to_string(), ".".to_string()],
                fix_args: None, // Pylint doesn't auto-fix
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
