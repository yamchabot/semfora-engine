//! Unified linting module for semfora-engine.
//!
//! This module provides a unified interface for running linters across
//! multiple programming languages. It supports:
//!
//! ## Core 4 Languages
//! - **Rust**: Clippy, rustfmt
//! - **JavaScript/TypeScript**: ESLint, Prettier, Biome, TSC, Oxlint
//! - **Python**: Ruff, Black, mypy, Pylint
//! - **Go**: golangci-lint, gofmt, go vet
//!
//! ## JVM Languages
//! - **Java**: Checkstyle, SpotBugs, PMD
//! - **Kotlin**: detekt, ktlint
//!
//! ## Systems Languages
//! - **C/C++**: clang-tidy, cppcheck, cpplint
//!
//! ## .NET Languages
//! - **C#**: dotnet format, Roslyn Analyzers, StyleCop
//!
//! ## Web Frontend
//! - **HTML**: HTMLHint, html-validate
//! - **CSS/SCSS/SASS**: Stylelint
//!
//! ## Config/Data
//! - **JSON**: jsonlint
//! - **YAML**: yamllint
//! - **TOML**: taplo
//! - **XML**: xmllint
//!
//! ## Infrastructure
//! - **Terraform**: TFLint, terraform validate, terraform fmt
//! - **Shell**: ShellCheck, shfmt
//!
//! ## Documentation
//! - **Markdown**: markdownlint
//!
//! ## Usage
//!
//! ```rust,ignore
//! use semfora_engine::lint::{detect_linters, run_lint, LintRunOptions};
//! use std::path::Path;
//!
//! let dir = Path::new("/path/to/project");
//!
//! // Detect available linters
//! let linters = detect_linters(dir);
//!
//! // Run all detected linters
//! let options = LintRunOptions::default();
//! let results = run_lint(dir, &options)?;
//!
//! println!("Found {} errors, {} warnings", results.error_count, results.warning_count);
//! ```

// Submodules
mod cache;
pub mod detection;
pub mod parsers;
mod runner;
mod types;
mod version;

// Re-export types for public API
pub use types::{
    ConfigHash, DetectedLinter, LintCache, LintCapabilities, LintCategory, LintCommand, LintIssue,
    LintRecommendation, LintResults, LintRunOptions, LintSeverity, Linter, SingleLinterResult,
};

// Re-export core functions
pub use cache::{collect_config_hashes, get_recommendations};
pub use detection::detect_linters;
pub use runner::{run_lint, run_single_linter};
pub use version::{
    get_biome_version, get_black_version, get_checkstyle_version, get_clang_tidy_version,
    get_clippy_version, get_cppcheck_version, get_cpplint_version, get_detekt_version,
    get_dotnet_format_version, get_dotnet_version, get_eslint_version, get_go_version,
    get_golangci_version, get_html_validate_version, get_htmlhint_version, get_jsonlint_version,
    get_ktlint_version, get_markdownlint_version, get_mypy_version, get_oxlint_version,
    get_pmd_version, get_prettier_version, get_pylint_version, get_roslyn_version,
    get_ruff_version, get_rustfmt_version, get_shellcheck_version, get_shfmt_version,
    get_spotbugs_version, get_stylecop_version, get_stylelint_version, get_taplo_version,
    get_terraform_version, get_tflint_version, get_tsc_version, get_xmllint_version,
    get_yamllint_version, has_css_files, has_dotnet_project, has_html_files, has_json_files,
    has_markdown_files, has_nuget_package, has_pyproject_section, has_shell_files,
    has_terraform_files, has_toml_files, has_xml_files, has_yaml_files, is_command_available,
};
