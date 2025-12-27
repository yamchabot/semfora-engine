//! Linter detection for various programming languages.
//!
//! This module provides functions to detect available linters for a project
//! by examining configuration files and checking for installed tools.
//!
//! ## Supported Languages
//!
//! ### Core 4 Languages
//! - **Rust**: Clippy, rustfmt
//! - **JavaScript/TypeScript**: ESLint, Prettier, Biome, TSC, Oxlint
//! - **Python**: Ruff, Black, Mypy, Pylint
//! - **Go**: golangci-lint, gofmt, go vet
//!
//! ### JVM Languages
//! - **Java**: Checkstyle, SpotBugs, PMD
//! - **Kotlin**: detekt, ktlint
//!
//! ### Systems Languages
//! - **C/C++**: clang-tidy, cppcheck, cpplint
//!
//! ### .NET Languages
//! - **C#**: dotnet format, Roslyn Analyzers, StyleCop
//!
//! ### Web Frontend
//! - **HTML**: HTMLHint, html-validate
//! - **CSS/SCSS/SASS**: Stylelint
//!
//! ### Config/Data
//! - **JSON**: jsonlint
//! - **YAML**: yamllint
//! - **TOML**: taplo
//! - **XML**: xmllint

mod cpp;
mod csharp;
mod css;
mod go;
mod html;
mod java;
mod javascript;
mod json;
mod kotlin;
mod markdown;
mod python;
mod rust;
mod shell;
mod terraform;
mod toml;
mod xml;
mod yaml;

use std::path::Path;

use crate::lint::types::DetectedLinter;
use crate::lint::version::{has_markdown_files, has_shell_files, has_terraform_files};

// Re-export per-language detection functions
pub use cpp::{detect_cpp_linters, has_cpp_build_system, has_cpp_sources};
pub use csharp::detect_csharp_linters;
pub use css::detect_css_linters;
pub use go::detect_go_linters;
pub use html::detect_html_linters;
pub use java::{detect_java_linters, has_gradle_plugin};
pub use javascript::detect_js_linters;
pub use json::detect_json_linters;
pub use kotlin::{detect_kotlin_linters, has_kotlin_sources};
pub use markdown::detect_markdown_linters;
pub use python::detect_python_linters;
pub use rust::detect_rust_linters;
pub use shell::detect_shell_linters;
pub use terraform::detect_terraform_linters;
pub use toml::detect_toml_linters;
pub use xml::detect_xml_linters;
pub use yaml::detect_yaml_linters;

/// Detect available linters for a project directory
pub fn detect_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut detected = Vec::new();

    // Detect Rust linters
    if dir.join("Cargo.toml").exists() {
        detected.extend(detect_rust_linters(dir));
    }

    // Detect JavaScript/TypeScript linters
    if dir.join("package.json").exists() {
        detected.extend(detect_js_linters(dir));
    }

    // Detect Python linters
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
    {
        detected.extend(detect_python_linters(dir));
    }

    // Detect Go linters
    if dir.join("go.mod").exists() {
        detected.extend(detect_go_linters(dir));
    }

    // Detect Java linters
    if dir.join("pom.xml").exists()
        || dir.join("build.gradle").exists()
        || dir.join("build.gradle.kts").exists()
    {
        detected.extend(detect_java_linters(dir));
    }

    // Detect Kotlin linters
    if dir.join("build.gradle.kts").exists() || has_kotlin_sources(dir) {
        detected.extend(detect_kotlin_linters(dir));
    }

    // Detect C/C++ linters
    if has_cpp_sources(dir) || has_cpp_build_system(dir) {
        detected.extend(detect_cpp_linters(dir));
    }

    // Detect C#/.NET linters
    if has_dotnet_project(dir) {
        detected.extend(detect_csharp_linters(dir));
    }

    // Detect HTML linters (check for HTML files or package.json with html linter deps)
    if has_html_files(dir) || dir.join("package.json").exists() {
        detected.extend(detect_html_linters(dir));
    }

    // Detect CSS/SCSS/SASS linters (check for CSS files or package.json with stylelint)
    if has_css_files(dir) || dir.join("package.json").exists() {
        detected.extend(detect_css_linters(dir));
    }

    // Detect JSON linters (check for package.json with jsonlint dependency)
    if dir.join("package.json").exists() {
        detected.extend(detect_json_linters(dir));
    }

    // Detect YAML linters (check for yamllint config)
    detected.extend(detect_yaml_linters(dir));

    // Detect TOML linters (check for taplo config or Cargo.toml)
    detected.extend(detect_toml_linters(dir));

    // Detect XML linters (check for XML files)
    detected.extend(detect_xml_linters(dir));

    // Detect Terraform linters (check for .tf files)
    if has_terraform_files(dir) {
        detected.extend(detect_terraform_linters(dir));
    }

    // Detect Shell linters (check for .sh files)
    if has_shell_files(dir) {
        detected.extend(detect_shell_linters(dir));
    }

    // Detect Markdown linters (check for .md files or config)
    if has_markdown_files(dir) || dir.join("package.json").exists() {
        detected.extend(detect_markdown_linters(dir));
    }

    detected
}

/// Check if directory contains HTML files
fn has_html_files(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "html" || ext == "htm" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains CSS/SCSS/SASS files
fn has_css_files(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "css" || ext == "scss" || ext == "sass" || ext == "less" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains .NET project files
fn has_dotnet_project(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "sln" || ext == "csproj" || ext == "fsproj" || ext == "vbproj" {
                    return true;
                }
            }
        }
    }
    false
}
