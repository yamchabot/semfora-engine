//! Kotlin linter detection (detekt, ktlint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_detekt_version, get_ktlint_version, is_command_available};

/// Check if a directory contains Kotlin source files
pub fn has_kotlin_sources(dir: &Path) -> bool {
    // Check common Kotlin source directories
    let kotlin_dirs = [
        "src/main/kotlin",
        "src/test/kotlin",
        "src/commonMain/kotlin",
        "src/jvmMain/kotlin",
        "app/src/main/kotlin",
    ];

    for kotlin_dir in kotlin_dirs {
        if dir.join(kotlin_dir).exists() {
            return true;
        }
    }

    // Fallback: check for any .kt files in src directory
    if let Ok(entries) = std::fs::read_dir(dir.join("src")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |ext| ext == "kt" || ext == "kts")
            {
                return true;
            }
            // Check one level deeper
            if path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        if sub_entry
                            .path()
                            .extension()
                            .map_or(false, |ext| ext == "kt" || ext == "kts")
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Detect Kotlin linters (detekt, ktlint)
pub fn detect_kotlin_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();
    let is_gradle = dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists();

    // detekt - Kotlin static analyzer
    let has_detekt_config = dir.join("detekt.yml").exists()
        || dir.join("detekt.yaml").exists()
        || dir.join("config/detekt/detekt.yml").exists();

    if has_detekt_config || is_command_available("detekt") {
        let (program, args, fix_args) = if is_gradle {
            (
                "./gradlew".to_string(),
                vec!["detekt".to_string()],
                Some(vec!["detekt".to_string(), "--auto-correct".to_string()]),
            )
        } else {
            (
                "detekt".to_string(),
                vec![
                    "--report".to_string(),
                    "json:build/reports/detekt.json".to_string(),
                ],
                Some(vec!["--auto-correct".to_string()]),
            )
        };

        linters.push(DetectedLinter {
            linter: Linter::Detekt,
            config_path: if dir.join("detekt.yml").exists() {
                Some(dir.join("detekt.yml"))
            } else if dir.join("detekt.yaml").exists() {
                Some(dir.join("detekt.yaml"))
            } else if dir.join("config/detekt/detekt.yml").exists() {
                Some(dir.join("config/detekt/detekt.yml"))
            } else {
                None
            },
            version: get_detekt_version(),
            available: is_gradle || is_command_available("detekt"),
            run_command: LintCommand {
                program,
                args,
                fix_args,
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // ktlint - Kotlin linter and formatter
    // ktlint can be detected via .editorconfig or presence of Kotlin files
    let has_kotlin_files = dir.join("src").exists(); // Simplified check
    let has_ktlint_config = dir.join(".editorconfig").exists();

    if has_ktlint_config || has_kotlin_files || is_command_available("ktlint") {
        let (program, args, fix_args) = if is_gradle {
            (
                "./gradlew".to_string(),
                vec!["ktlintCheck".to_string()],
                Some(vec!["ktlintFormat".to_string()]),
            )
        } else {
            (
                "ktlint".to_string(),
                vec!["--reporter=json".to_string()],
                Some(vec!["-F".to_string()]),
            )
        };

        linters.push(DetectedLinter {
            linter: Linter::Ktlint,
            config_path: if dir.join(".editorconfig").exists() {
                Some(dir.join(".editorconfig"))
            } else {
                None
            },
            version: get_ktlint_version(),
            available: is_gradle || is_command_available("ktlint"),
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

    linters
}
