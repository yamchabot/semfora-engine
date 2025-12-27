//! Cache management and recommendations for linting.
//!
//! This module provides functions for caching linter configurations
//! and generating recommendations for projects.

use std::path::{Path, PathBuf};

use crate::error::{McpDiffError, Result};
use crate::lint::detection::detect_linters;
use crate::lint::types::{ConfigHash, LintCache, LintRecommendation, Linter};

/// Collect config file hashes for cache invalidation
pub fn collect_config_hashes(dir: &Path) -> Vec<ConfigHash> {
    let config_patterns = [
        // Rust
        "Cargo.toml",
        "clippy.toml",
        ".clippy.toml",
        "rustfmt.toml",
        ".rustfmt.toml",
        // JS/TS
        "package.json",
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.json",
        ".eslintrc.yaml",
        ".eslintrc.yml",
        "eslint.config.js",
        "eslint.config.mjs",
        ".prettierrc",
        ".prettierrc.js",
        ".prettierrc.json",
        "prettier.config.js",
        "biome.json",
        "biome.jsonc",
        "tsconfig.json",
        // Python
        "pyproject.toml",
        "ruff.toml",
        ".ruff.toml",
        "mypy.ini",
        ".mypy.ini",
        "setup.cfg",
        // Go
        "go.mod",
        ".golangci.yml",
        ".golangci.yaml",
        ".golangci.json",
        // Java/Kotlin (JVM)
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "checkstyle.xml",
        "pmd.xml",
        "spotbugs.xml",
        "detekt.yml",
        "detekt.yaml",
        ".editorconfig", // ktlint uses this
        // C/C++
        "CMakeLists.txt",
        "Makefile",
        "meson.build",
        ".clang-tidy",
        ".clang-format",
        "compile_commands.json",
        "CPPLINT.cfg",
        ".cppcheck",
        "cppcheck.cfg",
    ];

    let mut hashes = Vec::new();
    for pattern in &config_patterns {
        let path = dir.join(pattern);
        if let Ok(metadata) = std::fs::metadata(&path) {
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            hashes.push(ConfigHash {
                path: PathBuf::from(pattern),
                mtime,
                size: metadata.len(),
            });
        }
    }

    hashes
}

/// Get linting recommendations for a project
pub fn get_recommendations(dir: &Path) -> Vec<LintRecommendation> {
    let mut recommendations = Vec::new();
    let detected = detect_linters(dir);
    let detected_types: Vec<Linter> = detected.iter().map(|d| d.linter).collect();

    // Rust recommendations
    if dir.join("Cargo.toml").exists() {
        if !detected_types.contains(&Linter::Clippy) {
            recommendations.push(LintRecommendation {
                linter: Linter::Clippy,
                reason:
                    "Rust's official linter catches common mistakes and suggests idiomatic code"
                        .to_string(),
                install_command: "rustup component add clippy".to_string(),
                priority: 9,
            });
        }
    }

    // JS/TS recommendations
    if dir.join("package.json").exists() {
        if !detected_types.contains(&Linter::ESLint) && !detected_types.contains(&Linter::Biome) {
            recommendations.push(LintRecommendation {
                linter: Linter::ESLint,
                reason: "Industry-standard JavaScript/TypeScript linter".to_string(),
                install_command: "npm install -D eslint".to_string(),
                priority: 8,
            });

            recommendations.push(LintRecommendation {
                linter: Linter::Biome,
                reason: "Fast all-in-one linter and formatter (ESLint + Prettier alternative)"
                    .to_string(),
                install_command: "npm install -D @biomejs/biome".to_string(),
                priority: 7,
            });
        }

        if dir.join("tsconfig.json").exists() && !detected_types.contains(&Linter::Tsc) {
            recommendations.push(LintRecommendation {
                linter: Linter::Tsc,
                reason: "TypeScript type checker for catching type errors".to_string(),
                install_command: "npm install -D typescript".to_string(),
                priority: 9,
            });
        }
    }

    // Python recommendations
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
    {
        if !detected_types.contains(&Linter::Ruff) {
            recommendations.push(LintRecommendation {
                linter: Linter::Ruff,
                reason: "Extremely fast Python linter (10-100x faster than Flake8)".to_string(),
                install_command: "pip install ruff".to_string(),
                priority: 9,
            });
        }

        if !detected_types.contains(&Linter::Mypy) {
            recommendations.push(LintRecommendation {
                linter: Linter::Mypy,
                reason: "Python's most popular static type checker".to_string(),
                install_command: "pip install mypy".to_string(),
                priority: 7,
            });
        }
    }

    // Go recommendations
    if dir.join("go.mod").exists() {
        if !detected_types.contains(&Linter::GolangciLint) {
            recommendations.push(LintRecommendation {
                linter: Linter::GolangciLint,
                reason: "Meta-linter that runs many Go linters in parallel".to_string(),
                install_command:
                    "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest"
                        .to_string(),
                priority: 8,
            });
        }
    }

    // C/C++ recommendations
    let has_cpp_project = dir.join("CMakeLists.txt").exists()
        || dir.join("Makefile").exists()
        || dir.join("meson.build").exists()
        || dir.join("compile_commands.json").exists();

    if has_cpp_project {
        if !detected_types.contains(&Linter::ClangTidy) {
            recommendations.push(LintRecommendation {
                linter: Linter::ClangTidy,
                reason: "Powerful static analyzer based on Clang with auto-fix support".to_string(),
                install_command: "apt install clang-tidy  # or brew install llvm".to_string(),
                priority: 9,
            });
        }

        if !detected_types.contains(&Linter::Cppcheck) {
            recommendations.push(LintRecommendation {
                linter: Linter::Cppcheck,
                reason: "Fast static analysis for C/C++ with low false-positive rate".to_string(),
                install_command: "apt install cppcheck  # or brew install cppcheck".to_string(),
                priority: 7,
            });
        }

        if !detected_types.contains(&Linter::Cpplint) {
            recommendations.push(LintRecommendation {
                linter: Linter::Cpplint,
                reason: "Google C++ style guide checker".to_string(),
                install_command: "pip install cpplint".to_string(),
                priority: 5,
            });
        }
    }

    // Sort by priority (highest first)
    recommendations.sort_by(|a, b| b.priority.cmp(&a.priority));

    recommendations
}

impl LintCache {
    /// Load lint cache from .semfora/lint.json
    pub fn load(dir: &Path) -> Option<LintCache> {
        let cache_path = dir.join(".semfora").join("lint.json");
        if !cache_path.exists() {
            return None;
        }

        std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    }

    /// Save lint cache to .semfora/lint.json
    pub fn save(&self, dir: &Path) -> Result<()> {
        let semfora_dir = dir.join(".semfora");
        if !semfora_dir.exists() {
            std::fs::create_dir_all(&semfora_dir).map_err(|e| McpDiffError::IoError {
                path: semfora_dir.clone(),
                message: format!("Failed to create .semfora directory: {}", e),
            })?;
        }

        let cache_path = semfora_dir.join("lint.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| McpDiffError::IoError {
            path: cache_path.clone(),
            message: format!("Failed to serialize lint cache: {}", e),
        })?;

        std::fs::write(&cache_path, content).map_err(|e| McpDiffError::IoError {
            path: cache_path.clone(),
            message: format!("Failed to write lint cache: {}", e),
        })?;

        Ok(())
    }

    /// Check if cache is still valid based on config file modifications
    pub fn is_valid(&self, dir: &Path) -> bool {
        for hash in &self.config_hashes {
            let path = dir.join(&hash.path);
            if !path.exists() {
                return false; // Config file was deleted
            }

            if let Ok(metadata) = std::fs::metadata(&path) {
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let size = metadata.len();

                if mtime != hash.mtime || size != hash.size {
                    return false; // Config file was modified
                }
            }
        }

        true
    }
}
