//! Output parsers for different linting tools.
//!
//! Each submodule contains parsers for a specific language's linters.

pub mod cpp;
pub mod csharp;
pub mod css;
pub mod go;
pub mod html;
pub mod java;
pub mod javascript;
pub mod json;
pub mod kotlin;
pub mod markdown;
pub mod python;
pub mod rust;
pub mod shell;
pub mod terraform;
pub mod toml;
pub mod xml;
pub mod yaml;

use std::path::Path;

use crate::lint::types::{LintIssue, Linter};

/// Parse linter output based on linter type
pub fn parse_linter_output(
    linter: Linter,
    stdout: &str,
    stderr: &str,
    dir: &Path,
) -> Vec<LintIssue> {
    match linter {
        // Rust
        Linter::Clippy => rust::parse_clippy_output(stdout, stderr, dir),
        Linter::Rustfmt => rust::parse_rustfmt_output(stdout, stderr, dir),
        // JavaScript/TypeScript
        Linter::ESLint => javascript::parse_eslint_output(stdout, dir),
        Linter::Prettier => javascript::parse_prettier_output(stdout, stderr, dir),
        Linter::Biome => javascript::parse_biome_output(stdout, dir),
        Linter::Tsc => javascript::parse_tsc_output(stdout, stderr, dir),
        Linter::Oxlint => javascript::parse_oxlint_output(stdout, dir),
        // Python
        Linter::Ruff => python::parse_ruff_output(stdout, dir),
        Linter::Black => python::parse_black_output(stdout, stderr, dir),
        Linter::Mypy => python::parse_mypy_output(stdout, dir),
        Linter::Pylint => python::parse_pylint_output(stdout, dir),
        // Go
        Linter::GolangciLint => go::parse_golangci_output(stdout, dir),
        Linter::Gofmt => go::parse_gofmt_output(stdout, dir),
        Linter::GoVet => go::parse_govet_output(stderr, dir),
        // Java
        Linter::Checkstyle => java::parse_checkstyle_output(stdout, stderr, dir),
        Linter::SpotBugs => java::parse_spotbugs_output(stdout, stderr, dir),
        Linter::Pmd => java::parse_pmd_output(stdout, dir),
        // Kotlin
        Linter::Detekt => kotlin::parse_detekt_output(stdout, dir),
        Linter::Ktlint => kotlin::parse_ktlint_output(stdout, dir),
        // C/C++
        Linter::ClangTidy => cpp::parse_clang_tidy_output(stdout, stderr, dir),
        Linter::Cppcheck => cpp::parse_cppcheck_output(stdout, stderr, dir),
        Linter::Cpplint => cpp::parse_cpplint_output(stdout, stderr, dir),
        // C#/.NET
        Linter::DotnetFormat => csharp::parse_dotnet_format_output(stdout, stderr, dir),
        Linter::RoslynAnalyzers => csharp::parse_roslyn_output(stdout, stderr, dir),
        Linter::StyleCop => csharp::parse_stylecop_output(stdout, stderr, dir),
        // HTML
        Linter::HtmlHint => html::parse_htmlhint_output(stdout, dir),
        Linter::HtmlValidate => html::parse_html_validate_output(stdout, dir),
        // CSS/SCSS/SASS
        Linter::Stylelint => css::parse_stylelint_output(stdout, dir),
        // Config/Data
        Linter::JsonLint => json::parse_jsonlint_output(stdout, stderr, dir),
        Linter::YamlLint => yaml::parse_yamllint_output(stdout, dir),
        Linter::Taplo => toml::parse_taplo_output(stdout, stderr, dir),
        Linter::XmlLint => xml::parse_xmllint_output(stdout, stderr, dir),
        // Infrastructure
        Linter::TfLint => terraform::parse_tflint_output(stdout, dir),
        Linter::TerraformValidate => terraform::parse_terraform_validate_output(stdout, dir),
        Linter::TerraformFmt => terraform::parse_terraform_fmt_output(stdout, dir),
        Linter::ShellCheck => shell::parse_shellcheck_output(stdout, dir),
        Linter::Shfmt => shell::parse_shfmt_output(stdout, stderr, dir),
        // Documentation
        Linter::MarkdownLint => markdown::parse_markdownlint_output(stdout, stderr, dir),
        // Unknown
        _ => Vec::new(),
    }
}
