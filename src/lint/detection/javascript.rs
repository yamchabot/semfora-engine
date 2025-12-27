//! JavaScript/TypeScript linter detection (ESLint, Prettier, Biome, TSC, Oxlint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{
    get_biome_version, get_eslint_version, get_oxlint_version, get_prettier_version,
    get_tsc_version,
};

/// Detect JavaScript/TypeScript linters
pub fn detect_js_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Check for ESLint
    let eslint_configs = [
        ".eslintrc.js",
        ".eslintrc.cjs",
        ".eslintrc.json",
        ".eslintrc.yml",
        ".eslintrc.yaml",
        "eslint.config.js",
        "eslint.config.mjs",
        "eslint.config.cjs",
    ];

    for config in eslint_configs {
        if dir.join(config).exists() {
            linters.push(DetectedLinter {
                linter: Linter::ESLint,
                config_path: Some(dir.join(config)),
                version: get_eslint_version(),
                available: true,
                run_command: LintCommand {
                    program: "npx".to_string(),
                    args: vec![
                        "eslint".to_string(),
                        "--format".to_string(),
                        "json".to_string(),
                        ".".to_string(),
                    ],
                    fix_args: Some(vec![
                        "eslint".to_string(),
                        "--fix".to_string(),
                        ".".to_string(),
                    ]),
                    cwd: None,
                },
                capabilities: LintCapabilities {
                    can_fix: true,
                    can_format: false,
                    can_typecheck: false,
                },
            });
            break;
        }
    }

    // Check for Prettier
    let prettier_configs = [
        ".prettierrc",
        ".prettierrc.js",
        ".prettierrc.cjs",
        ".prettierrc.json",
        ".prettierrc.yml",
        ".prettierrc.yaml",
        "prettier.config.js",
        "prettier.config.cjs",
    ];

    for config in prettier_configs {
        if dir.join(config).exists() {
            linters.push(DetectedLinter {
                linter: Linter::Prettier,
                config_path: Some(dir.join(config)),
                version: get_prettier_version(),
                available: true,
                run_command: LintCommand {
                    program: "npx".to_string(),
                    args: vec![
                        "prettier".to_string(),
                        "--check".to_string(),
                        ".".to_string(),
                    ],
                    fix_args: Some(vec![
                        "prettier".to_string(),
                        "--write".to_string(),
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
            break;
        }
    }

    // Check for Biome
    if dir.join("biome.json").exists() || dir.join("biome.jsonc").exists() {
        linters.push(DetectedLinter {
            linter: Linter::Biome,
            config_path: if dir.join("biome.json").exists() {
                Some(dir.join("biome.json"))
            } else {
                Some(dir.join("biome.jsonc"))
            },
            version: get_biome_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec!["biome".to_string(), "check".to_string(), ".".to_string()],
                fix_args: Some(vec![
                    "biome".to_string(),
                    "check".to_string(),
                    "--write".to_string(),
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

    // Check for TypeScript
    if dir.join("tsconfig.json").exists() {
        linters.push(DetectedLinter {
            linter: Linter::Tsc,
            config_path: Some(dir.join("tsconfig.json")),
            version: get_tsc_version(),
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec!["tsc".to_string(), "--noEmit".to_string()],
                fix_args: None, // TSC doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: true,
            },
        });
    }

    // Check for Oxlint (fast Rust-based linter)
    // Oxlint can be detected via oxlint.json config or package.json devDependencies
    let has_oxlint_config = dir.join("oxlint.json").exists() || dir.join(".oxlintrc.json").exists();

    // Check if oxlint is in package.json devDependencies
    let has_oxlint_in_package = if dir.join("package.json").exists() {
        std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .map(|content| content.contains("\"oxlint\"") || content.contains("\"@oxlint/"))
            .unwrap_or(false)
    } else {
        false
    };

    if has_oxlint_config || has_oxlint_in_package {
        linters.push(DetectedLinter {
            linter: Linter::Oxlint,
            config_path: if dir.join("oxlint.json").exists() {
                Some(dir.join("oxlint.json"))
            } else if dir.join(".oxlintrc.json").exists() {
                Some(dir.join(".oxlintrc.json"))
            } else {
                None
            },
            version: get_oxlint_version(),
            available: true, // npx will handle installation
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec![
                    "oxlint".to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                    ".".to_string(),
                ],
                fix_args: Some(vec![
                    "oxlint".to_string(),
                    "--fix".to_string(),
                    ".".to_string(),
                ]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    linters
}
