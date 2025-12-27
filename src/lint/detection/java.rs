//! Java linter detection (Checkstyle, SpotBugs, PMD).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{
    get_checkstyle_version, get_pmd_version, get_spotbugs_version, is_command_available,
};

/// Check if a Gradle build file contains a specific plugin
pub fn has_gradle_plugin(dir: &Path, plugin_name: &str) -> bool {
    let build_files = ["build.gradle", "build.gradle.kts"];

    for build_file in build_files {
        let path = dir.join(build_file);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                // Check for plugin declarations like:
                // id "pmd" or id 'pmd' or id("pmd") or id("com.github.spotbugs")
                let patterns = [
                    format!("id \"{plugin_name}\""),
                    format!("id '{plugin_name}'"),
                    format!("id(\"{plugin_name}\")"),
                    format!("id('{plugin_name}')"),
                    format!("apply plugin: '{plugin_name}'"),
                    format!("apply plugin: \"{plugin_name}\""),
                ];

                for pattern in patterns {
                    if content.contains(&pattern) {
                        return true;
                    }
                }

                // For SpotBugs, also check the full plugin name
                if plugin_name == "spotbugs" && content.contains("com.github.spotbugs") {
                    return true;
                }
            }
        }
    }
    false
}

/// Detect Java linters (Checkstyle, SpotBugs, PMD)
pub fn detect_java_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();
    let is_gradle = dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists();
    let is_maven = dir.join("pom.xml").exists();

    // Checkstyle
    let has_checkstyle_config = dir.join("checkstyle.xml").exists()
        || dir.join("config/checkstyle/checkstyle.xml").exists();

    if has_checkstyle_config || is_command_available("checkstyle") {
        let (program, args, fix_args) = if is_gradle {
            (
                "./gradlew".to_string(),
                vec!["checkstyleMain".to_string(), "checkstyleTest".to_string()],
                None,
            )
        } else if is_maven {
            (
                "mvn".to_string(),
                vec!["checkstyle:check".to_string()],
                None,
            )
        } else {
            (
                "checkstyle".to_string(),
                vec![
                    "-c".to_string(),
                    "checkstyle.xml".to_string(),
                    "src/".to_string(),
                ],
                None,
            )
        };

        linters.push(DetectedLinter {
            linter: Linter::Checkstyle,
            config_path: if dir.join("checkstyle.xml").exists() {
                Some(dir.join("checkstyle.xml"))
            } else if dir.join("config/checkstyle/checkstyle.xml").exists() {
                Some(dir.join("config/checkstyle/checkstyle.xml"))
            } else {
                None
            },
            version: get_checkstyle_version(),
            available: is_command_available("checkstyle") || is_gradle || is_maven,
            run_command: LintCommand {
                program,
                args,
                fix_args,
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // SpotBugs (successor to FindBugs)
    let has_spotbugs_config =
        dir.join("spotbugs.xml").exists() || dir.join("spotbugs-exclude.xml").exists();

    // Also detect from Gradle plugins
    let has_spotbugs_gradle_plugin = (is_gradle || is_maven) && has_gradle_plugin(dir, "spotbugs");

    if has_spotbugs_config || has_spotbugs_gradle_plugin || is_command_available("spotbugs") {
        let (program, args) = if is_gradle {
            ("./gradlew".to_string(), vec!["spotbugsMain".to_string()])
        } else if is_maven {
            ("mvn".to_string(), vec!["spotbugs:check".to_string()])
        } else {
            (
                "spotbugs".to_string(),
                vec!["-textui".to_string(), "-xml".to_string()],
            )
        };

        linters.push(DetectedLinter {
            linter: Linter::SpotBugs,
            config_path: if dir.join("spotbugs.xml").exists() {
                Some(dir.join("spotbugs.xml"))
            } else {
                None
            },
            version: get_spotbugs_version(),
            available: is_gradle || is_maven || is_command_available("spotbugs"),
            run_command: LintCommand {
                program,
                args,
                fix_args: None, // SpotBugs doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // PMD
    let has_pmd_config_dir = dir.join("config/pmd").exists()
        && dir
            .join("config/pmd")
            .read_dir()
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.path().extension().map_or(false, |ext| ext == "xml"))
            })
            .unwrap_or(false);
    let has_pmd_config = dir.join("pmd.xml").exists()
        || dir.join("pmd-ruleset.xml").exists()
        || dir.join("config/pmd/pmd.xml").exists()
        || has_pmd_config_dir;

    // Also detect from Gradle plugins
    let has_pmd_gradle_plugin = (is_gradle || is_maven) && has_gradle_plugin(dir, "pmd");

    if has_pmd_config || has_pmd_gradle_plugin || is_command_available("pmd") {
        let (program, args) = if is_gradle {
            ("./gradlew".to_string(), vec!["pmdMain".to_string()])
        } else if is_maven {
            ("mvn".to_string(), vec!["pmd:check".to_string()])
        } else {
            (
                "pmd".to_string(),
                vec![
                    "check".to_string(),
                    "-d".to_string(),
                    "src/".to_string(),
                    "-R".to_string(),
                    "pmd.xml".to_string(),
                    "-f".to_string(),
                    "json".to_string(),
                ],
            )
        };

        linters.push(DetectedLinter {
            linter: Linter::Pmd,
            config_path: if dir.join("pmd.xml").exists() {
                Some(dir.join("pmd.xml"))
            } else if dir.join("config/pmd/pmd.xml").exists() {
                Some(dir.join("config/pmd/pmd.xml"))
            } else {
                None
            },
            version: get_pmd_version(),
            available: is_gradle || is_maven || is_command_available("pmd"),
            run_command: LintCommand {
                program,
                args,
                fix_args: None, // PMD doesn't auto-fix
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
