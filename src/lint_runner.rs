//! Multi-language lint runner for code quality analysis.
//!
//! Detects and runs linters, formatters, and type checkers, returning structured
//! results that can be used by the CLI and MCP interfaces.
//!
//! Supported tools (Core 4 languages):
//!
//! **Rust:**
//! - clippy - Rust linter
//! - rustfmt - Rust formatter
//!
//! **JavaScript/TypeScript:**
//! - ESLint - JS/TS linter
//! - Prettier - Code formatter
//! - Biome - All-in-one linter/formatter
//! - TSC - TypeScript type checker
//!
//! **Python:**
//! - ruff - Fast Python linter (replaces flake8, isort, etc.)
//! - black - Python formatter
//! - mypy - Python type checker
//!
//! **Go:**
//! - golangci-lint - Go meta-linter
//! - gofmt - Go formatter
//! - go vet - Go static analyzer

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::McpDiffError;

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

    /// Load cache from disk if it exists and is valid
    pub fn load(dir: &Path) -> Option<Self> {
        let cache_path = Self::cache_path(dir);
        if !cache_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&cache_path).ok()?;
        let cache: LintCache = serde_json::from_str(&content).ok()?;

        // Validate schema version
        if cache.schema_version != "1.0" {
            return None;
        }

        // Check if cache is stale by comparing config file hashes
        if cache.is_stale(dir) {
            return None;
        }

        Some(cache)
    }

    /// Save cache to disk
    pub fn save(&self, dir: &Path) -> Result<()> {
        let cache_path = Self::cache_path(dir);

        // Create .semfora directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| McpDiffError::ParseFailure { message: e.to_string() })?;
        std::fs::write(&cache_path, content)?;

        Ok(())
    }

    /// Check if cache is stale by comparing config file modification times
    pub fn is_stale(&self, dir: &Path) -> bool {
        for hash in &self.config_hashes {
            let full_path = dir.join(&hash.path);
            if let Ok(metadata) = std::fs::metadata(&full_path) {
                // Check mtime and size
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let size = metadata.len();

                if mtime != hash.mtime || size != hash.size {
                    return true;
                }
            } else {
                // Config file was deleted
                return true;
            }
        }

        // Check for new config files that weren't in the cache
        let current_configs = collect_config_hashes(dir);
        if current_configs.len() != self.config_hashes.len() {
            return true;
        }

        false
    }
}

/// Collect config file hashes for cache validation
fn collect_config_hashes(dir: &Path) -> Vec<ConfigHash> {
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

// ============================================================================
// Detection Functions
// ============================================================================

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

    detected
}

/// Detect Rust linters (clippy, rustfmt)
fn detect_rust_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Clippy - always available if Cargo.toml exists
    linters.push(DetectedLinter {
        linter: Linter::Clippy,
        config_path: Some(dir.join("Cargo.toml")),
        version: get_clippy_version(),
        available: true,
        run_command: LintCommand {
            program: "cargo".to_string(),
            args: vec![
                "clippy".to_string(),
                "--message-format=json".to_string(),
                "--".to_string(),
                "-D".to_string(),
                "warnings".to_string(),
            ],
            fix_args: Some(vec![
                "clippy".to_string(),
                "--fix".to_string(),
                "--allow-dirty".to_string(),
            ]),
            cwd: None,
        },
        capabilities: LintCapabilities {
            can_fix: true,
            can_format: false,
            can_typecheck: false,
        },
    });

    // Rustfmt
    linters.push(DetectedLinter {
        linter: Linter::Rustfmt,
        config_path: if dir.join("rustfmt.toml").exists() {
            Some(dir.join("rustfmt.toml"))
        } else if dir.join(".rustfmt.toml").exists() {
            Some(dir.join(".rustfmt.toml"))
        } else {
            None
        },
        version: get_rustfmt_version(),
        available: true,
        run_command: LintCommand {
            program: "cargo".to_string(),
            args: vec!["fmt".to_string(), "--check".to_string()],
            fix_args: Some(vec!["fmt".to_string()]),
            cwd: None,
        },
        capabilities: LintCapabilities {
            can_fix: true,
            can_format: true,
            can_typecheck: false,
        },
    });

    linters
}

/// Detect JavaScript/TypeScript linters
fn detect_js_linters(dir: &Path) -> Vec<DetectedLinter> {
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
                version: None, // TODO: detect version
                available: true,
                run_command: LintCommand {
                    program: "npx".to_string(),
                    args: vec!["eslint".to_string(), "--format".to_string(), "json".to_string(), ".".to_string()],
                    fix_args: Some(vec!["eslint".to_string(), "--fix".to_string(), ".".to_string()]),
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
                version: None,
                available: true,
                run_command: LintCommand {
                    program: "npx".to_string(),
                    args: vec!["prettier".to_string(), "--check".to_string(), ".".to_string()],
                    fix_args: Some(vec!["prettier".to_string(), "--write".to_string(), ".".to_string()]),
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
            version: None,
            available: true,
            run_command: LintCommand {
                program: "npx".to_string(),
                args: vec!["biome".to_string(), "check".to_string(), ".".to_string()],
                fix_args: Some(vec!["biome".to_string(), "check".to_string(), "--write".to_string(), ".".to_string()]),
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
            version: None,
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

    linters
}

/// Detect Python linters
fn detect_python_linters(dir: &Path) -> Vec<DetectedLinter> {
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
            version: None,
            available: is_command_available("ruff"),
            run_command: LintCommand {
                program: "ruff".to_string(),
                args: vec!["check".to_string(), "--output-format".to_string(), "json".to_string(), ".".to_string()],
                fix_args: Some(vec!["check".to_string(), "--fix".to_string(), ".".to_string()]),
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
            version: None,
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
            version: None,
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

    linters
}

/// Detect Go linters
fn detect_go_linters(dir: &Path) -> Vec<DetectedLinter> {
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
            version: None,
            available: is_command_available("golangci-lint"),
            run_command: LintCommand {
                program: "golangci-lint".to_string(),
                args: vec!["run".to_string(), "--out-format".to_string(), "json".to_string()],
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
            version: None,
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
            version: None,
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

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if a command is available in PATH
fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get clippy version
fn get_clippy_version() -> Option<String> {
    Command::new("cargo")
        .args(["clippy", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        })
}

/// Get rustfmt version
fn get_rustfmt_version() -> Option<String> {
    Command::new("rustfmt")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        })
}

/// Check if pyproject.toml has a specific section
fn has_pyproject_section(dir: &Path, section: &str) -> bool {
    let pyproject = dir.join("pyproject.toml");
    if !pyproject.exists() {
        return false;
    }

    std::fs::read_to_string(pyproject)
        .map(|content| content.contains(&format!("[{}]", section)))
        .unwrap_or(false)
}

// ============================================================================
// Recommendation Engine
// ============================================================================

/// Get recommendations for missing linters
pub fn get_recommendations(dir: &Path) -> Vec<LintRecommendation> {
    let mut recommendations = Vec::new();
    let detected = detect_linters(dir);
    let detected_types: Vec<Linter> = detected.iter().map(|d| d.linter).collect();

    // Rust recommendations
    if dir.join("Cargo.toml").exists() {
        if !detected_types.contains(&Linter::Clippy) {
            recommendations.push(LintRecommendation {
                linter: Linter::Clippy,
                reason: "Rust's official linter catches common mistakes and suggests idiomatic code"
                    .to_string(),
                install_command: "rustup component add clippy".to_string(),
                priority: 9,
            });
        }
    }

    // JS/TS recommendations
    if dir.join("package.json").exists() {
        if !detected_types.contains(&Linter::ESLint)
            && !detected_types.contains(&Linter::Biome)
        {
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
                install_command: "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest".to_string(),
                priority: 8,
            });
        }
    }

    // Sort by priority (highest first)
    recommendations.sort_by(|a, b| b.priority.cmp(&a.priority));

    recommendations
}

// ============================================================================
// Cache Management
// ============================================================================

/// Load lint cache from .semfora/lint.json
pub fn load_cache(dir: &Path) -> Option<LintCache> {
    let cache_path = dir.join(".semfora").join("lint.json");
    if !cache_path.exists() {
        return None;
    }

    std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

/// Save lint cache to .semfora/lint.json
pub fn save_cache(dir: &Path, cache: &LintCache) -> Result<()> {
    let semfora_dir = dir.join(".semfora");
    if !semfora_dir.exists() {
        std::fs::create_dir_all(&semfora_dir).map_err(|e| {
            McpDiffError::IoError {
                path: semfora_dir.clone(),
                message: format!("Failed to create .semfora directory: {}", e),
            }
        })?;
    }

    let cache_path = semfora_dir.join("lint.json");
    let content = serde_json::to_string_pretty(cache).map_err(|e| {
        McpDiffError::IoError {
            path: cache_path.clone(),
            message: format!("Failed to serialize lint cache: {}", e),
        }
    })?;

    std::fs::write(&cache_path, content).map_err(|e| McpDiffError::IoError {
        path: cache_path.clone(),
        message: format!("Failed to write lint cache: {}", e),
    })?;

    Ok(())
}

/// Check if cache is still valid based on config file modifications
pub fn is_cache_valid(dir: &Path, cache: &LintCache) -> bool {
    for hash in &cache.config_hashes {
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

// ============================================================================
// Run Functions
// ============================================================================

/// Run linters with the given options
pub fn run_lint(dir: &Path, options: &LintRunOptions) -> Result<LintResults> {
    let start = std::time::Instant::now();

    // Try to load cached linter detection
    let (mut detected, used_cache) = if let Some(cache) = LintCache::load(dir) {
        (cache.detected_linters, true)
    } else {
        // Detect available linters and save to cache
        let linters = detect_linters(dir);
        let cache = LintCache {
            schema_version: "1.0".to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            detected_linters: linters.clone(),
            custom_commands: HashMap::new(),
            config_hashes: collect_config_hashes(dir),
        };
        // Best-effort save - don't fail if we can't write cache
        let _ = cache.save(dir);
        (linters, false)
    };

    // Log cache usage in verbose mode
    if !used_cache {
        // Cache was regenerated
    }

    // Filter to specific linter if requested
    if let Some(ref target_linter) = options.linter {
        detected.retain(|d| d.linter == *target_linter);
    }

    // Filter to only available linters
    detected.retain(|d| d.available);

    if detected.is_empty() {
        return Ok(LintResults {
            success: true,
            error_count: 0,
            warning_count: 0,
            files_with_issues: 0,
            duration_ms: start.elapsed().as_millis() as u64,
            linters: Vec::new(),
            issues: Vec::new(),
        });
    }

    // Run linters in parallel using rayon
    let dir_owned = dir.to_path_buf();
    let options_clone = options.clone();

    let parallel_results: Vec<Result<SingleLinterResult>> = detected
        .par_iter()
        .map(|linter| run_single_linter(linter, &dir_owned, &options_clone))
        .collect();

    // Collect results, handling any errors
    let mut linter_results = Vec::new();
    let mut all_issues = Vec::new();

    for result in parallel_results {
        match result {
            Ok(r) => {
                all_issues.extend(r.issues.clone());
                linter_results.push(r);
            }
            Err(e) => {
                // Log error but continue with other linters
                eprintln!("Warning: Linter failed: {}", e);
            }
        }
    }

    // Apply severity filter
    if let Some(min_severity) = options.severity_filter {
        all_issues.retain(|issue| issue.severity >= min_severity);
    }

    // Apply fixable filter
    if options.fixable_only {
        all_issues.retain(|issue| issue.fix.is_some());
    }

    // Sort issues by file, then line
    all_issues.sort_by(|a, b| {
        match a.file.cmp(&b.file) {
            std::cmp::Ordering::Equal => a.line.cmp(&b.line),
            other => other,
        }
    });

    // Apply limit
    if let Some(limit) = options.limit {
        all_issues.truncate(limit);
    }

    // Calculate totals
    let error_count = all_issues.iter().filter(|i| i.severity == LintSeverity::Error).count();
    let warning_count = all_issues.iter().filter(|i| i.severity == LintSeverity::Warning).count();
    let files_with_issues: std::collections::HashSet<&str> =
        all_issues.iter().map(|i| i.file.as_str()).collect();

    Ok(LintResults {
        success: error_count == 0,
        error_count,
        warning_count,
        files_with_issues: files_with_issues.len(),
        duration_ms: start.elapsed().as_millis() as u64,
        linters: linter_results,
        issues: all_issues,
    })
}

/// Run a single linter and parse its output
pub fn run_single_linter(
    linter: &DetectedLinter,
    dir: &Path,
    options: &LintRunOptions,
) -> Result<SingleLinterResult> {
    let start = std::time::Instant::now();

    // Choose args based on fix mode
    let args = if options.fix && !options.dry_run {
        linter.run_command.fix_args.as_ref().unwrap_or(&linter.run_command.args)
    } else {
        &linter.run_command.args
    };

    // Build and execute command
    let mut cmd = Command::new(&linter.run_command.program);
    cmd.args(args);

    // Set working directory
    let dir_buf = dir.to_path_buf();
    let cwd = linter.run_command.cwd.as_ref().unwrap_or(&dir_buf);
    cmd.current_dir(cwd);

    // Capture output
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::IoError {
        path: dir.to_path_buf(),
        message: format!("Failed to run {}: {}", linter.linter.display_name(), e),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    // Parse output based on linter type
    let issues = parse_linter_output(linter.linter, &stdout, &stderr, dir);

    let error_count = issues.iter().filter(|i| i.severity == LintSeverity::Error).count();
    let warning_count = issues.iter().filter(|i| i.severity == LintSeverity::Warning).count();

    Ok(SingleLinterResult {
        linter: linter.linter,
        success: exit_code == Some(0) && error_count == 0,
        error_count,
        warning_count,
        duration_ms: start.elapsed().as_millis() as u64,
        issues,
        stdout,
        stderr,
        exit_code,
    })
}

// ============================================================================
// Output Parsing
// ============================================================================

/// Parse linter output based on linter type
fn parse_linter_output(linter: Linter, stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    match linter {
        Linter::Clippy => parse_clippy_output(stdout, stderr, dir),
        Linter::Rustfmt => parse_rustfmt_output(stdout, stderr, dir),
        Linter::ESLint => parse_eslint_output(stdout, dir),
        Linter::Prettier => parse_prettier_output(stdout, stderr, dir),
        Linter::Biome => parse_biome_output(stdout, dir),
        Linter::Tsc => parse_tsc_output(stdout, stderr, dir),
        Linter::Ruff => parse_ruff_output(stdout, dir),
        Linter::Black => parse_black_output(stdout, stderr, dir),
        Linter::Mypy => parse_mypy_output(stdout, dir),
        Linter::GolangciLint => parse_golangci_output(stdout, dir),
        Linter::Gofmt => parse_gofmt_output(stdout, dir),
        Linter::GoVet => parse_govet_output(stderr, dir),
        _ => Vec::new(),
    }
}

/// Parse clippy JSON output (cargo clippy --message-format=json)
fn parse_clippy_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON message
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            // Only process compiler messages
            if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
                continue;
            }

            if let Some(message) = msg.get("message") {
                let level = message.get("level").and_then(|l| l.as_str()).unwrap_or("warning");
                let text = message.get("message").and_then(|m| m.as_str()).unwrap_or("");
                let code = message.get("code")
                    .and_then(|c| c.get("code"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("unknown");

                // Skip certain message types
                if level == "note" || level == "help" || text.starts_with("aborting") {
                    continue;
                }

                // Get primary span
                if let Some(spans) = message.get("spans").and_then(|s| s.as_array()) {
                    for span in spans {
                        if span.get("is_primary").and_then(|p| p.as_bool()) != Some(true) {
                            continue;
                        }

                        let file = span.get("file_name").and_then(|f| f.as_str()).unwrap_or("");
                        let line = span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                        let column = span.get("column_start").and_then(|c| c.as_u64()).map(|c| c as usize);
                        let end_line = span.get("line_end").and_then(|l| l.as_u64()).map(|l| l as usize);
                        let end_column = span.get("column_end").and_then(|c| c.as_u64()).map(|c| c as usize);
                        let suggested_replacement = span.get("suggested_replacement").and_then(|s| s.as_str());

                        // Make path relative to dir
                        let file_path = PathBuf::from(file);
                        let relative_file = file_path.strip_prefix(dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| file.to_string());

                        let severity = match level {
                            "error" => LintSeverity::Error,
                            "warning" => LintSeverity::Warning,
                            _ => LintSeverity::Info,
                        };

                        issues.push(LintIssue {
                            file: relative_file,
                            line,
                            column,
                            end_line,
                            end_column,
                            severity,
                            rule: code.to_string(),
                            message: text.to_string(),
                            linter: Linter::Clippy,
                            fix: suggested_replacement.map(|s| s.to_string()),
                        });

                        break; // Only process first primary span
                    }
                }
            }
        }
    }

    issues
}

/// Parse rustfmt output (cargo fmt --check)
fn parse_rustfmt_output(_stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // rustfmt outputs "Diff in <file>" messages
    for line in stderr.lines() {
        if line.starts_with("Diff in ") {
            let file = line.trim_start_matches("Diff in ").trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            issues.push(LintIssue {
                file: relative_file,
                line: 1,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "formatting".to_string(),
                message: "File needs formatting".to_string(),
                linter: Linter::Rustfmt,
                fix: Some("Run 'cargo fmt' to fix".to_string()),
            });
        }
    }

    issues
}

/// Parse ESLint JSON output
fn parse_eslint_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("filePath").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    let line = msg.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
                    let column = msg.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);
                    let end_line = msg.get("endLine").and_then(|l| l.as_u64()).map(|l| l as usize);
                    let end_column = msg.get("endColumn").and_then(|c| c.as_u64()).map(|c| c as usize);
                    let severity_num = msg.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                    let rule = msg.get("ruleId").and_then(|r| r.as_str()).unwrap_or("unknown");
                    let message = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    let fix = msg.get("fix").map(|_| "Auto-fixable".to_string());

                    let severity = match severity_num {
                        2 => LintSeverity::Error,
                        1 => LintSeverity::Warning,
                        _ => LintSeverity::Info,
                    };

                    issues.push(LintIssue {
                        file: relative_file.clone(),
                        line,
                        column,
                        end_line,
                        end_column,
                        severity,
                        rule: rule.to_string(),
                        message: message.to_string(),
                        linter: Linter::ESLint,
                        fix,
                    });
                }
            }
        }
    }

    issues
}

/// Parse Prettier output
fn parse_prettier_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Prettier --check outputs files that need formatting
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Checking") || line.contains("checking") {
            continue;
        }

        let file_path = PathBuf::from(line);
        let relative_file = file_path.strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| line.to_string());

        issues.push(LintIssue {
            file: relative_file,
            line: 1,
            column: None,
            end_line: None,
            end_column: None,
            severity: LintSeverity::Warning,
            rule: "formatting".to_string(),
            message: "File needs formatting".to_string(),
            linter: Linter::Prettier,
            fix: Some("Run 'npx prettier --write' to fix".to_string()),
        });
    }

    issues
}

/// Parse Biome output
fn parse_biome_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Biome outputs diagnostic messages in a custom format
    // For now, parse the human-readable output
    let mut current_file = String::new();

    for line in stdout.lines() {
        // Look for file paths
        if line.contains("┌─") {
            if let Some(path_part) = line.split("┌─").nth(1) {
                current_file = path_part.trim().to_string();
            }
            continue;
        }

        // Look for error/warning lines with line:col info
        if line.contains("error") || line.contains("warning") {
            if let Some(pos) = line.find(':') {
                let (loc, rest) = line.split_at(pos);
                if let Some((line_str, _col_str)) = loc.split_once(':') {
                    if let Ok(line_num) = line_str.trim().parse::<usize>() {
                        let file_path = PathBuf::from(&current_file);
                        let relative_file = file_path.strip_prefix(dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| current_file.clone());

                        let severity = if line.contains("error") {
                            LintSeverity::Error
                        } else {
                            LintSeverity::Warning
                        };

                        issues.push(LintIssue {
                            file: relative_file,
                            line: line_num,
                            column: None,
                            end_line: None,
                            end_column: None,
                            severity,
                            rule: "biome".to_string(),
                            message: rest.trim_start_matches(':').trim().to_string(),
                            linter: Linter::Biome,
                            fix: None,
                        });
                    }
                }
            }
        }
    }

    issues
}

/// Parse TypeScript compiler output
fn parse_tsc_output(_stdout: &str, stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // TSC outputs: file(line,col): error TS1234: message
    let re = regex::Regex::new(r"^(.+?)\((\d+),(\d+)\):\s*(error|warning)\s+(TS\d+):\s*(.+)$").ok();

    for line in stderr.lines() {
        if let Some(ref re) = re {
            if let Some(caps) = re.captures(line) {
                let file = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let line_num = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
                let col = caps.get(3).and_then(|m| m.as_str().parse().ok());
                let level = caps.get(4).map(|m| m.as_str()).unwrap_or("error");
                let code = caps.get(5).map(|m| m.as_str()).unwrap_or("");
                let message = caps.get(6).map(|m| m.as_str()).unwrap_or("");

                let file_path = PathBuf::from(file);
                let relative_file = file_path.strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let severity = if level == "error" {
                    LintSeverity::Error
                } else {
                    LintSeverity::Warning
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line: line_num,
                    column: col,
                    end_line: None,
                    end_column: None,
                    severity,
                    rule: code.to_string(),
                    message: message.to_string(),
                    linter: Linter::Tsc,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Parse Ruff JSON output
fn parse_ruff_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("filename").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line = result.get("location")
                .and_then(|l| l.get("row"))
                .and_then(|r| r.as_u64())
                .unwrap_or(1) as usize;
            let column = result.get("location")
                .and_then(|l| l.get("column"))
                .and_then(|c| c.as_u64())
                .map(|c| c as usize);

            let code = result.get("code").and_then(|c| c.as_str()).unwrap_or("");
            let message = result.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let fix = result.get("fix").map(|_| "Auto-fixable".to_string());

            issues.push(LintIssue {
                file: relative_file,
                line,
                column,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Error,
                rule: code.to_string(),
                message: message.to_string(),
                linter: Linter::Ruff,
                fix,
            });
        }
    }

    issues
}

/// Parse Black output
fn parse_black_output(stdout: &str, _stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Black --check outputs: "would reformat <file>"
    for line in stdout.lines() {
        if line.starts_with("would reformat ") {
            let file = line.trim_start_matches("would reformat ").trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            issues.push(LintIssue {
                file: relative_file,
                line: 1,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "formatting".to_string(),
                message: "File needs formatting".to_string(),
                linter: Linter::Black,
                fix: Some("Run 'black' to fix".to_string()),
            });
        }
    }

    issues
}

/// Parse mypy output
fn parse_mypy_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // Try JSON format first
    if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for result in results {
            let file = result.get("file").and_then(|f| f.as_str()).unwrap_or("");
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line = result.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
            let column = result.get("column").and_then(|c| c.as_u64()).map(|c| c as usize);
            let severity_str = result.get("severity").and_then(|s| s.as_str()).unwrap_or("error");
            let message = result.get("message").and_then(|m| m.as_str()).unwrap_or("");

            let severity = match severity_str {
                "error" => LintSeverity::Error,
                "warning" => LintSeverity::Warning,
                "note" => LintSeverity::Info,
                _ => LintSeverity::Error,
            };

            issues.push(LintIssue {
                file: relative_file,
                line,
                column,
                end_line: None,
                end_column: None,
                severity,
                rule: "type-error".to_string(),
                message: message.to_string(),
                linter: Linter::Mypy,
                fix: None,
            });
        }
        return issues;
    }

    // Fallback to text format: file:line:col: severity: message
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let file = parts[0].trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line_num = parts[1].trim().parse().unwrap_or(1);
            let rest = parts[3].trim();

            let (severity, message) = if rest.starts_with("error:") {
                (LintSeverity::Error, rest.trim_start_matches("error:").trim())
            } else if rest.starts_with("warning:") {
                (LintSeverity::Warning, rest.trim_start_matches("warning:").trim())
            } else if rest.starts_with("note:") {
                (LintSeverity::Info, rest.trim_start_matches("note:").trim())
            } else {
                (LintSeverity::Error, rest)
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column: None,
                end_line: None,
                end_column: None,
                severity,
                rule: "type-error".to_string(),
                message: message.to_string(),
                linter: Linter::Mypy,
                fix: None,
            });
        }
    }

    issues
}

/// Parse golangci-lint JSON output
fn parse_golangci_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    if let Ok(result) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(lint_issues) = result.get("Issues").and_then(|i| i.as_array()) {
            for issue in lint_issues {
                let file = issue.get("Pos")
                    .and_then(|p| p.get("Filename"))
                    .and_then(|f| f.as_str())
                    .unwrap_or("");
                let file_path = PathBuf::from(file);
                let relative_file = file_path.strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.to_string());

                let line = issue.get("Pos")
                    .and_then(|p| p.get("Line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(1) as usize;
                let column = issue.get("Pos")
                    .and_then(|p| p.get("Column"))
                    .and_then(|c| c.as_u64())
                    .map(|c| c as usize);

                let rule = issue.get("FromLinter").and_then(|r| r.as_str()).unwrap_or("");
                let message = issue.get("Text").and_then(|m| m.as_str()).unwrap_or("");
                let severity_str = issue.get("Severity").and_then(|s| s.as_str()).unwrap_or("");

                let severity = match severity_str {
                    "error" => LintSeverity::Error,
                    "warning" => LintSeverity::Warning,
                    _ => LintSeverity::Warning,
                };

                issues.push(LintIssue {
                    file: relative_file,
                    line,
                    column,
                    end_line: None,
                    end_column: None,
                    severity,
                    rule: rule.to_string(),
                    message: message.to_string(),
                    linter: Linter::GolangciLint,
                    fix: None,
                });
            }
        }
    }

    issues
}

/// Parse gofmt output
fn parse_gofmt_output(stdout: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // gofmt -l outputs list of files that need formatting
    for line in stdout.lines() {
        let file = line.trim();
        if file.is_empty() {
            continue;
        }

        let file_path = PathBuf::from(file);
        let relative_file = file_path.strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file.to_string());

        issues.push(LintIssue {
            file: relative_file,
            line: 1,
            column: None,
            end_line: None,
            end_column: None,
            severity: LintSeverity::Warning,
            rule: "formatting".to_string(),
            message: "File needs formatting".to_string(),
            linter: Linter::Gofmt,
            fix: Some("Run 'gofmt -w' to fix".to_string()),
        });
    }

    issues
}

/// Parse go vet output
fn parse_govet_output(stderr: &str, dir: &Path) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    // go vet outputs: file:line:col: message
    for line in stderr.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 {
            let file = parts[0].trim();
            let file_path = PathBuf::from(file);
            let relative_file = file_path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string());

            let line_num = parts[1].trim().parse().unwrap_or(1);
            let message = if parts.len() >= 4 {
                parts[3].trim()
            } else {
                parts[2].trim()
            };

            issues.push(LintIssue {
                file: relative_file,
                line: line_num,
                column: None,
                end_line: None,
                end_column: None,
                severity: LintSeverity::Warning,
                rule: "vet".to_string(),
                message: message.to_string(),
                linter: Linter::GoVet,
                fix: None,
            });
        }
    }

    issues
}
