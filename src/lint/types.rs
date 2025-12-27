//! Core types for the lint module.
//!
//! This module contains all the fundamental types used throughout the linting system:
//! - `Linter` - Enum of supported linting tools
//! - `DetectedLinter` - A linter detected in a project with its configuration
//! - `LintIssue` - A single issue found by a linter
//! - `LintResults` - Combined results from running linters
//! - `LintCache` - Cached linter configuration for performance

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ============================================================================
// Core Types
// ============================================================================

/// Supported linting tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Linter {
    // Rust
    Clippy,
    Rustfmt,

    // JavaScript/TypeScript
    ESLint,
    Prettier,
    Biome,
    Oxlint,
    Tsc,

    // Python
    Ruff,
    Black,
    Mypy,
    Pylint,

    // Go
    GolangciLint,
    Gofmt,
    GoVet,

    // Java
    Checkstyle,
    SpotBugs,
    Pmd,

    // Kotlin
    Detekt,
    Ktlint,

    // C/C++
    ClangTidy,
    Cppcheck,
    Cpplint,

    // C#/.NET
    DotnetFormat,
    RoslynAnalyzers,
    StyleCop,

    // HTML
    HtmlHint,
    HtmlValidate,

    // CSS/SCSS/SASS
    Stylelint,

    // Config/Data
    JsonLint,
    YamlLint,
    Taplo,
    XmlLint,

    // Infrastructure
    TfLint,
    TerraformValidate,
    TerraformFmt,
    ShellCheck,
    Shfmt,

    // Documentation
    MarkdownLint,

    // Unknown/custom
    Unknown,
}

impl Linter {
    pub fn as_str(&self) -> &'static str {
        match self {
            Linter::Clippy => "clippy",
            Linter::Rustfmt => "rustfmt",
            Linter::ESLint => "eslint",
            Linter::Prettier => "prettier",
            Linter::Biome => "biome",
            Linter::Oxlint => "oxlint",
            Linter::Tsc => "tsc",
            Linter::Ruff => "ruff",
            Linter::Black => "black",
            Linter::Mypy => "mypy",
            Linter::Pylint => "pylint",
            Linter::GolangciLint => "golangci-lint",
            Linter::Gofmt => "gofmt",
            Linter::GoVet => "go-vet",
            Linter::Checkstyle => "checkstyle",
            Linter::SpotBugs => "spotbugs",
            Linter::Pmd => "pmd",
            Linter::Detekt => "detekt",
            Linter::Ktlint => "ktlint",
            Linter::ClangTidy => "clang-tidy",
            Linter::Cppcheck => "cppcheck",
            Linter::Cpplint => "cpplint",
            Linter::DotnetFormat => "dotnet-format",
            Linter::RoslynAnalyzers => "roslyn",
            Linter::StyleCop => "stylecop",
            Linter::HtmlHint => "htmlhint",
            Linter::HtmlValidate => "html-validate",
            Linter::Stylelint => "stylelint",
            Linter::JsonLint => "jsonlint",
            Linter::YamlLint => "yamllint",
            Linter::Taplo => "taplo",
            Linter::XmlLint => "xmllint",
            Linter::TfLint => "tflint",
            Linter::TerraformValidate => "terraform-validate",
            Linter::TerraformFmt => "terraform-fmt",
            Linter::ShellCheck => "shellcheck",
            Linter::Shfmt => "shfmt",
            Linter::MarkdownLint => "markdownlint",
            Linter::Unknown => "unknown",
        }
    }

    /// Get the human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Linter::Clippy => "Clippy",
            Linter::Rustfmt => "rustfmt",
            Linter::ESLint => "ESLint",
            Linter::Prettier => "Prettier",
            Linter::Biome => "Biome",
            Linter::Oxlint => "Oxlint",
            Linter::Tsc => "TypeScript",
            Linter::Ruff => "Ruff",
            Linter::Black => "Black",
            Linter::Mypy => "mypy",
            Linter::Pylint => "Pylint",
            Linter::GolangciLint => "golangci-lint",
            Linter::Gofmt => "gofmt",
            Linter::GoVet => "go vet",
            Linter::Checkstyle => "Checkstyle",
            Linter::SpotBugs => "SpotBugs",
            Linter::Pmd => "PMD",
            Linter::Detekt => "detekt",
            Linter::Ktlint => "ktlint",
            Linter::ClangTidy => "clang-tidy",
            Linter::Cppcheck => "cppcheck",
            Linter::Cpplint => "cpplint",
            Linter::DotnetFormat => "dotnet format",
            Linter::RoslynAnalyzers => "Roslyn Analyzers",
            Linter::StyleCop => "StyleCop",
            Linter::HtmlHint => "HTMLHint",
            Linter::HtmlValidate => "html-validate",
            Linter::Stylelint => "Stylelint",
            Linter::JsonLint => "jsonlint",
            Linter::YamlLint => "yamllint",
            Linter::Taplo => "taplo",
            Linter::XmlLint => "xmllint",
            Linter::TfLint => "TFLint",
            Linter::TerraformValidate => "terraform validate",
            Linter::TerraformFmt => "terraform fmt",
            Linter::ShellCheck => "ShellCheck",
            Linter::Shfmt => "shfmt",
            Linter::MarkdownLint => "markdownlint",
            Linter::Unknown => "Unknown",
        }
    }

    /// Get the language this linter is for
    pub fn language(&self) -> &'static str {
        match self {
            Linter::Clippy | Linter::Rustfmt => "rust",
            Linter::ESLint | Linter::Prettier | Linter::Biome | Linter::Oxlint | Linter::Tsc => {
                "javascript"
            }
            Linter::Ruff | Linter::Black | Linter::Mypy | Linter::Pylint => "python",
            Linter::GolangciLint | Linter::Gofmt | Linter::GoVet => "go",
            Linter::Checkstyle | Linter::SpotBugs | Linter::Pmd => "java",
            Linter::Detekt | Linter::Ktlint => "kotlin",
            Linter::ClangTidy | Linter::Cppcheck | Linter::Cpplint => "cpp",
            Linter::DotnetFormat | Linter::RoslynAnalyzers | Linter::StyleCop => "csharp",
            Linter::HtmlHint | Linter::HtmlValidate => "html",
            Linter::Stylelint => "css",
            Linter::JsonLint => "json",
            Linter::YamlLint => "yaml",
            Linter::Taplo => "toml",
            Linter::XmlLint => "xml",
            Linter::TfLint | Linter::TerraformValidate | Linter::TerraformFmt => "terraform",
            Linter::ShellCheck | Linter::Shfmt => "shell",
            Linter::MarkdownLint => "markdown",
            Linter::Unknown => "unknown",
        }
    }
}

impl std::str::FromStr for Linter {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "clippy" => Ok(Linter::Clippy),
            "rustfmt" => Ok(Linter::Rustfmt),
            "eslint" => Ok(Linter::ESLint),
            "prettier" => Ok(Linter::Prettier),
            "biome" => Ok(Linter::Biome),
            "oxlint" => Ok(Linter::Oxlint),
            "tsc" | "typescript" => Ok(Linter::Tsc),
            "ruff" => Ok(Linter::Ruff),
            "black" => Ok(Linter::Black),
            "mypy" => Ok(Linter::Mypy),
            "pylint" => Ok(Linter::Pylint),
            "golangci-lint" | "golangci" => Ok(Linter::GolangciLint),
            "gofmt" => Ok(Linter::Gofmt),
            "go-vet" | "govet" | "vet" => Ok(Linter::GoVet),
            "checkstyle" => Ok(Linter::Checkstyle),
            "spotbugs" | "findbugs" => Ok(Linter::SpotBugs),
            "pmd" => Ok(Linter::Pmd),
            "detekt" => Ok(Linter::Detekt),
            "ktlint" => Ok(Linter::Ktlint),
            "clang-tidy" | "clangtidy" => Ok(Linter::ClangTidy),
            "cppcheck" => Ok(Linter::Cppcheck),
            "cpplint" => Ok(Linter::Cpplint),
            "dotnet-format" | "dotnetformat" | "dotnet format" => Ok(Linter::DotnetFormat),
            "roslyn" | "roslyn-analyzers" | "roslynanalyzers" => Ok(Linter::RoslynAnalyzers),
            "stylecop" | "stylecop.analyzers" => Ok(Linter::StyleCop),
            "htmlhint" | "html-hint" => Ok(Linter::HtmlHint),
            "html-validate" | "htmlvalidate" => Ok(Linter::HtmlValidate),
            "stylelint" => Ok(Linter::Stylelint),
            "jsonlint" | "json-lint" => Ok(Linter::JsonLint),
            "yamllint" | "yaml-lint" => Ok(Linter::YamlLint),
            "taplo" => Ok(Linter::Taplo),
            "xmllint" | "xml-lint" => Ok(Linter::XmlLint),
            "tflint" | "tf-lint" => Ok(Linter::TfLint),
            "terraform-validate" | "terraformvalidate" => Ok(Linter::TerraformValidate),
            "terraform-fmt" | "terraformfmt" => Ok(Linter::TerraformFmt),
            "shellcheck" | "shell-check" => Ok(Linter::ShellCheck),
            "shfmt" => Ok(Linter::Shfmt),
            "markdownlint" | "markdown-lint" | "mdlint" => Ok(Linter::MarkdownLint),
            _ => Err(()),
        }
    }
}

/// Category of linting tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintCategory {
    /// Code linting (style, bugs, complexity)
    Linting,
    /// Code formatting
    Formatting,
    /// Type checking
    TypeChecking,
}

/// Capabilities of a linter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LintCapabilities {
    /// Can automatically fix issues
    pub can_fix: bool,
    /// Can format code
    pub can_format: bool,
    /// Can perform type checking
    pub can_typecheck: bool,
}

/// Severity level for lint issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    /// Informational hint
    Hint,
    /// Style suggestion
    Info,
    /// Warning (should fix)
    Warning,
    /// Error (must fix)
    Error,
}

impl LintSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            LintSeverity::Hint => "hint",
            LintSeverity::Info => "info",
            LintSeverity::Warning => "warning",
            LintSeverity::Error => "error",
        }
    }

    /// Short code for TOON output
    pub fn code(&self) -> char {
        match self {
            LintSeverity::Hint => 'H',
            LintSeverity::Info => 'I',
            LintSeverity::Warning => 'W',
            LintSeverity::Error => 'E',
        }
    }
}

impl std::str::FromStr for LintSeverity {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hint" => Ok(LintSeverity::Hint),
            "info" | "note" => Ok(LintSeverity::Info),
            "warning" | "warn" => Ok(LintSeverity::Warning),
            "error" | "err" => Ok(LintSeverity::Error),
            _ => Err(()),
        }
    }
}

// ============================================================================
// Command Types
// ============================================================================

/// Cached command to run a linter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintCommand {
    /// The executable to run (e.g., "npx", "cargo", "python")
    pub program: String,

    /// Arguments for scan/check mode
    pub args: Vec<String>,

    /// Arguments for fix mode (None if not supported)
    pub fix_args: Option<Vec<String>>,

    /// Working directory override (None = project root)
    pub cwd: Option<PathBuf>,
}

impl LintCommand {
    /// Create a new lint command
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            fix_args: None,
            cwd: None,
        }
    }

    /// Set fix arguments
    pub fn with_fix_args(mut self, args: Vec<String>) -> Self {
        self.fix_args = Some(args);
        self
    }

    /// Set working directory
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}

/// A detected linter with its configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedLinter {
    /// The linter type
    pub linter: Linter,

    /// Path to the config file (if found)
    pub config_path: Option<PathBuf>,

    /// Detected version (if available)
    pub version: Option<String>,

    /// Whether the linter is available to run
    pub available: bool,

    /// Command to run this linter
    pub run_command: LintCommand,

    /// What this linter can do
    pub capabilities: LintCapabilities,
}

// ============================================================================
// Issue Types
// ============================================================================

/// A single lint issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    /// File path where the issue was found
    pub file: String,

    /// Line number (1-based)
    pub line: usize,

    /// Column number (1-based, optional)
    pub column: Option<usize>,

    /// End line (for multi-line issues)
    pub end_line: Option<usize>,

    /// End column
    pub end_column: Option<usize>,

    /// Severity of the issue
    pub severity: LintSeverity,

    /// Rule ID/code (e.g., "no-unused-vars", "E501")
    pub rule: String,

    /// Human-readable message
    pub message: String,

    /// Which linter reported this
    pub linter: Linter,

    /// Suggested fix (if available)
    pub fix: Option<String>,
}

// ============================================================================
// Result Types
// ============================================================================

/// Results from running a single linter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleLinterResult {
    /// Which linter was run
    pub linter: Linter,

    /// Whether linting succeeded (no errors)
    pub success: bool,

    /// Number of errors found
    pub error_count: usize,

    /// Number of warnings found
    pub warning_count: usize,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Issues found
    pub issues: Vec<LintIssue>,

    /// Raw stdout (for debugging)
    pub stdout: String,

    /// Raw stderr (for debugging)
    pub stderr: String,

    /// Exit code
    pub exit_code: Option<i32>,
}

impl Default for SingleLinterResult {
    fn default() -> Self {
        Self {
            linter: Linter::Unknown,
            success: true,
            error_count: 0,
            warning_count: 0,
            duration_ms: 0,
            issues: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        }
    }
}

/// Combined results from running multiple linters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResults {
    /// Overall success (no errors from any linter)
    pub success: bool,

    /// Total error count across all linters
    pub error_count: usize,

    /// Total warning count across all linters
    pub warning_count: usize,

    /// Number of files with issues
    pub files_with_issues: usize,

    /// Total duration in milliseconds
    pub duration_ms: u64,

    /// Results from individual linters
    pub linters: Vec<SingleLinterResult>,

    /// All issues combined and sorted by file/line
    pub issues: Vec<LintIssue>,
}

impl Default for LintResults {
    fn default() -> Self {
        Self {
            success: true,
            error_count: 0,
            warning_count: 0,
            files_with_issues: 0,
            duration_ms: 0,
            linters: Vec::new(),
            issues: Vec::new(),
        }
    }
}

// ============================================================================
// Run Options
// ============================================================================

/// Options for running linters
#[derive(Debug, Clone, Default)]
pub struct LintRunOptions {
    /// Only run this specific linter
    pub linter: Option<Linter>,

    /// Filter to specific file or directory
    pub path_filter: Option<PathBuf>,

    /// Severity filter (only show issues >= this level)
    pub severity_filter: Option<LintSeverity>,

    /// Maximum issues to return
    pub limit: Option<usize>,

    /// Only show fixable issues
    pub fixable_only: bool,

    /// Run in fix mode (apply fixes)
    pub fix: bool,

    /// Dry run (show what would be fixed without changing files)
    pub dry_run: bool,

    /// Only apply safe fixes
    pub safe_only: bool,
}

// ============================================================================
// Recommendation Types
// ============================================================================

/// A recommendation for adding a linter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintRecommendation {
    /// The linter being recommended
    pub linter: Linter,

    /// Why this linter is recommended
    pub reason: String,

    /// How to install (e.g., "npm install -D eslint")
    pub install_command: String,

    /// Priority (higher = more important)
    pub priority: u8,
}

// ============================================================================
// Cache Types
// ============================================================================

/// Cached linter configuration stored in .semfora/lint.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintCache {
    /// Schema version for future compatibility
    pub schema_version: String,

    /// When this cache was generated
    pub generated_at: String,

    /// Detected linters with their run commands
    pub detected_linters: Vec<DetectedLinter>,

    /// User overrides for linter commands
    #[serde(default)]
    pub custom_commands: HashMap<String, LintCommand>,

    /// Config file hashes for cache invalidation
    #[serde(default)]
    pub config_hashes: Vec<ConfigHash>,
}

/// Hash of a config file for cache invalidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigHash {
    /// Path to the config file
    pub path: PathBuf,

    /// Modification time (Unix timestamp)
    pub mtime: u64,

    /// File size in bytes
    pub size: u64,
}

impl Default for LintCache {
    fn default() -> Self {
        Self {
            schema_version: "1.0".to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            detected_linters: Vec::new(),
            custom_commands: HashMap::new(),
            config_hashes: Vec::new(),
        }
    }
}

impl LintCache {
    /// Get the cache file path for a project directory
    pub fn cache_path(dir: &Path) -> PathBuf {
        dir.join(".semfora").join("lint.json")
    }
}
