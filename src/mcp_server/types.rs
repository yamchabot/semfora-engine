//! Request and response types for the MCP server
//!
//! This module contains all the request/response structs used by the MCP tools.

use rmcp::schemars;
use serde::Deserialize;

// ============================================================================
// Analysis Request Types
// ============================================================================

/// Unified analysis request: analyzes file, directory, or module (auto-detects).
/// - If `module` is provided: returns module semantic info from index
/// - If `path` points to a file: analyzes the file
/// - If `path` points to a directory: analyzes all files recursively
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeRequest {
    /// Path to analyze (file or directory). Required unless module is provided.
    #[schemars(description = "Path to analyze (file or directory). Auto-detects type.")]
    pub path: Option<String>,

    /// Module name to retrieve from index (alternative to path)
    #[schemars(
        description = "Module name to retrieve from index (e.g., 'api', 'components'). Alternative to path."
    )]
    pub module: Option<String>,

    /// Output format: "toon" (default) or "json" (only for file analysis)
    #[schemars(description = "Output format: 'toon' (compact) or 'json' (structured)")]
    pub format: Option<String>,

    /// Maximum depth for directory analysis (default: 10)
    #[schemars(
        description = "Maximum directory depth to traverse (default: 10, only for directory analysis)"
    )]
    pub max_depth: Option<usize>,

    /// Whether to include only the summary overview (directory analysis only)
    #[schemars(
        description = "If true, only return the repository overview, not individual files (directory only)"
    )]
    pub summary_only: Option<bool>,

    /// File extensions to include for directory analysis
    #[schemars(
        description = "File extensions to include (e.g., ['ts', 'tsx']). If empty, all supported extensions are included."
    )]
    pub extensions: Option<Vec<String>>,

    /// Start line for focused analysis of large files (1-indexed)
    #[schemars(
        description = "Start line for focused analysis (1-indexed). Use with end_line for large files (>2000 lines)."
    )]
    pub start_line: Option<usize>,

    /// End line for focused analysis of large files (1-indexed, inclusive)
    #[schemars(
        description = "End line for focused analysis (1-indexed, inclusive). Use with start_line for large files."
    )]
    pub end_line: Option<usize>,

    /// Output mode: "full" (default), "summary", or "symbols_only"
    #[schemars(
        description = "Output mode: 'full' (default - complete TOON), 'summary' (overview only), 'symbols_only' (just symbol list with line ranges)"
    )]
    pub output_mode: Option<String>,
}

/// Request to analyze git diff
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDiffRequest {
    /// The base branch or commit to compare against (e.g., "main", "HEAD~1").
    /// Use "HEAD" with target_ref "WORKING" to see uncommitted changes.
    #[schemars(
        description = "Base branch or commit to compare against (e.g., 'main', 'HEAD~1'). Use 'HEAD' with target_ref='WORKING' for uncommitted changes."
    )]
    pub base_ref: String,

    /// The target branch or commit. Use "WORKING" to compare against uncommitted changes in the working tree.
    /// Defaults to "HEAD" for committed changes.
    #[schemars(
        description = "Target branch or commit (defaults to 'HEAD'). Use 'WORKING' to analyze uncommitted changes vs base_ref."
    )]
    pub target_ref: Option<String>,

    /// Working directory (defaults to current directory)
    #[schemars(
        description = "Working directory for git operations (defaults to current directory)"
    )]
    pub working_dir: Option<String>,

    /// Maximum files to return per page (default: 20, max: 100)
    #[schemars(description = "Maximum files to return per page (default: 20, max: 100)")]
    pub limit: Option<usize>,

    /// Skip first N files for pagination (default: 0)
    #[schemars(description = "Skip first N files for pagination (default: 0)")]
    pub offset: Option<usize>,

    /// Return summary statistics only without per-file details (default: false)
    #[schemars(
        description = "Return only summary statistics (file counts, risk breakdown, top modules) without per-file details. Use for large diffs to get overview first."
    )]
    pub summary_only: Option<bool>,
}

/// Request to get supported languages
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetLanguagesRequest {}

// ============================================================================
// Quick Context Request Type
// ============================================================================

/// Request to get quick git and project context (~200 tokens)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetContextRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

// ============================================================================
// Sharded Index Request Types
// ============================================================================

/// Request to get repository overview from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetOverviewRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Maximum modules to include (default: 30, use 0 for no limit)
    #[schemars(
        description = "Maximum modules to include in output (default: 30). Set to 0 for no limit. Modules are sorted by symbol count."
    )]
    pub max_modules: Option<usize>,

    /// Exclude test directories from module listing (default: true)
    #[schemars(
        description = "Exclude test directories (tests, __tests__, test-repos) from module listing (default: true)"
    )]
    pub exclude_test_dirs: Option<bool>,

    /// Include git context (branch, last commit) in output (default: true)
    #[schemars(
        description = "Include git context (branch, last commit) in output (default: true)"
    )]
    pub include_git_context: Option<bool>,
}

/// Request to get symbol(s) from sharded index - supports single, batch, and file+line modes
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSymbolRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Symbol hash for single symbol lookup
    #[schemars(
        description = "Symbol hash from the repo overview or module shard (for single symbol)"
    )]
    pub symbol_hash: Option<String>,

    /// Symbol hashes for batch lookup (max 20) - takes precedence over symbol_hash
    #[schemars(
        description = "Array of symbol hashes to retrieve (max 20). If provided, batch mode is used."
    )]
    pub hashes: Option<Vec<String>>,

    /// File path for location-based lookup (use with `line`)
    #[schemars(description = "File path to find symbol at (use with `line` parameter)")]
    pub file: Option<String>,

    /// Line number for location-based lookup (use with `file`)
    #[schemars(
        description = "Line number to find symbol at (1-indexed, use with `file` parameter)"
    )]
    pub line: Option<usize>,

    /// Include source code snippets (for batch mode, default: false)
    #[schemars(description = "If true, include source code for each symbol (batch mode only)")]
    pub include_source: Option<bool>,

    /// Context lines for source (batch mode, default: 3)
    #[schemars(
        description = "Context lines before/after symbol source (batch mode only, default: 3)"
    )]
    pub context: Option<usize>,
}

/// Request to generate/regenerate sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GenerateIndexRequest {
    /// Path to the repository to index
    #[schemars(description = "Path to the repository to index")]
    pub path: String,

    /// Maximum directory depth (default: 10)
    #[schemars(description = "Maximum directory depth for file collection (default: 10)")]
    pub max_depth: Option<usize>,

    /// File extensions to include (e.g., ["ts", "tsx", "rs"])
    #[schemars(
        description = "File extensions to include (e.g., ['ts', 'tsx']). If empty, all supported extensions are included."
    )]
    pub extensions: Option<Vec<String>>,

    /// Force regeneration even if index exists (default: false)
    #[schemars(description = "Force regeneration even if index appears fresh")]
    pub force: Option<bool>,
}

/// Unified call graph query and export request.
/// By default returns call graph edges. Set export="sqlite" to export to database.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCallgraphRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Filter to calls from/to symbols in this module
    #[schemars(description = "Filter to edges involving symbols in this module")]
    pub module: Option<String>,

    /// Filter to calls from/to this specific symbol
    #[schemars(description = "Filter to edges from or to this symbol (by name or hash)")]
    pub symbol: Option<String>,

    /// Maximum edges to return (default: 500, max: 2000)
    #[schemars(description = "Maximum edges to return (default: 500, max: 2000)")]
    pub limit: Option<u32>,

    /// Pagination offset (default: 0)
    #[schemars(description = "Skip first N edges for pagination (default: 0)")]
    pub offset: Option<u32>,

    /// Return summary statistics only (no edge list)
    #[schemars(
        description = "Return only statistics (edge count, top callers) without full edge list"
    )]
    pub summary_only: Option<bool>,

    /// Include local variable references that escape their scope (passed/returned)
    #[schemars(
        description = "Include local variable references that escape their scope (passed/returned)"
    )]
    pub include_escape_refs: Option<bool>,

    /// Export format: "sqlite" to export to SQLite database (expensive operation)
    #[schemars(
        description = "Export format: 'sqlite' to export call graph to SQLite database (expensive disk-writing operation)"
    )]
    pub export: Option<String>,

    /// Output path for export (only used when export is set)
    #[schemars(
        description = "Output path for export file. Defaults to cache directory. Only used when export is set."
    )]
    pub output_path: Option<String>,

    /// Batch size for export transactions (default: 5000)
    #[schemars(
        description = "Rows per transaction batch for export (default: 5000). Only used when export is set."
    )]
    pub batch_size: Option<usize>,
}

/// Request to get source code (surgical read) - supports single hash, batch hashes, or file+lines
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSourceRequest {
    /// Path to the source file (for file+lines mode)
    #[schemars(
        description = "Path to the source file (for file+lines mode, optional if using hash/hashes)"
    )]
    pub file_path: Option<String>,

    /// Start line (1-indexed). If provided with end_line, reads that range.
    #[schemars(description = "Start line number (1-indexed)")]
    pub start_line: Option<usize>,

    /// End line (1-indexed, inclusive)
    #[schemars(description = "End line number (1-indexed, inclusive)")]
    pub end_line: Option<usize>,

    /// Symbol hash to look up (alternative to line numbers)
    #[schemars(description = "Symbol hash from the index - will look up line range automatically")]
    pub symbol_hash: Option<String>,

    /// Symbol hashes for batch source extraction (max 20)
    #[schemars(
        description = "Array of symbol hashes for batch source extraction (max 20). More efficient than multiple calls."
    )]
    pub hashes: Option<Vec<String>>,

    /// Context lines to include before/after the symbol (default: 5)
    #[schemars(description = "Number of context lines before and after the symbol (default: 5)")]
    pub context: Option<usize>,
}

// ============================================================================
// Query-Driven API Types
// ============================================================================

/// Unified search request - combines symbol search, semantic search, and raw regex search.
/// By default runs BOTH symbol and semantic search (hybrid mode) - the "magic" search.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    /// Search query - for symbol/semantic modes this searches symbol names and code semantically.
    /// For raw mode, this is a regex pattern.
    #[schemars(
        description = "Search query - matches symbol names and code semantically by default. For raw mode, this is a regex pattern."
    )]
    pub query: String,

    /// Search mode: "symbols" (exact name match), "semantic" (BM25 conceptual), "raw" (regex).
    /// Default (omit or null) runs hybrid mode: BOTH symbol AND semantic search combined.
    #[schemars(
        description = "Search mode: 'symbols' (exact name match), 'semantic' (BM25 conceptual), 'raw' (regex). Default runs BOTH symbol and semantic (hybrid)."
    )]
    pub mode: Option<String>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Filter by module name
    #[schemars(description = "Filter results to a specific module")]
    pub module: Option<String>,

    /// Filter by symbol kind (fn, struct, component, enum, trait, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Symbol scope (functions, variables, both)
    #[schemars(
        description = "Symbol scope: 'functions' (non-variable symbols), 'variables', or 'both' (default: functions)"
    )]
    pub symbol_scope: Option<String>,

    /// Filter by risk level (high, medium, low) - only for symbol/hybrid modes
    #[schemars(
        description = "Filter by risk level (high, medium, low) - symbol/hybrid modes only"
    )]
    pub risk: Option<String>,

    /// Maximum results to return (default: 20)
    #[schemars(description = "Maximum results to return (default: 20)")]
    pub limit: Option<usize>,

    /// Include source code snippets in results
    #[schemars(description = "Include source code snippets in results")]
    pub include_source: Option<bool>,

    /// Context lines around source snippets (default: 3)
    #[schemars(description = "Lines of context around symbol source (default: 3)")]
    pub context: Option<usize>,

    // --- Raw mode specific options ---
    /// File types to search (for raw mode, e.g., ["rs", "ts"])
    #[schemars(description = "File extensions to search (raw mode only, e.g., ['rs', 'ts'])")]
    pub file_types: Option<Vec<String>>,

    /// Case-insensitive search (for raw mode, default: true)
    #[schemars(description = "Case-insensitive search (raw mode only, default: true)")]
    pub case_insensitive: Option<bool>,

    /// Merge adjacent matches within N lines (for raw mode, default: 3)
    #[schemars(description = "Merge adjacent matches within N lines (raw mode only, default: 3)")]
    pub merge_threshold: Option<usize>,

    /// Include local variables that escape their scope (default: false)
    #[schemars(description = "Include local variables that escape their scope (default: false)")]
    pub include_escape_refs: Option<bool>,
}

/// Unified validate request - auto-detects scope based on provided parameters.
/// Validates symbol quality: complexity metrics, duplicates, and impact radius (callers).
///
/// Scope detection (in order of priority):
/// 1. If `symbol_hash` is provided → single symbol validation
/// 2. If `file_path` + `line` is provided → single symbol at location
/// 3. If `file_path` only is provided → file-level validation (all symbols)
/// 4. If `module` is provided → module-level validation
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateRequest {
    /// Symbol hash to validate (highest priority - single symbol validation)
    #[schemars(description = "Symbol hash to validate - triggers single symbol mode")]
    pub symbol_hash: Option<String>,

    /// File path - with line number validates single symbol, without validates all symbols in file
    #[schemars(
        description = "File path - with 'line' validates single symbol at location, without validates entire file"
    )]
    pub file_path: Option<String>,

    /// Line number within file (used with file_path for single symbol validation)
    #[schemars(description = "Line number within file to find symbol (used with file_path)")]
    pub line: Option<usize>,

    /// Module name for module-level validation (lowest priority)
    #[schemars(
        description = "Module name - validates all symbols in module (e.g., 'api', 'components')"
    )]
    pub module: Option<String>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Similarity threshold for duplicate detection (default: 0.85)
    #[schemars(description = "Similarity threshold for finding duplicates (default: 0.85)")]
    pub duplicate_threshold: Option<f64>,

    /// Filter by symbol kind (fn, struct, component, etc.) - for file/module scope
    #[schemars(
        description = "Filter by symbol kind - only for file/module scope (fn, struct, etc.)"
    )]
    pub kind: Option<String>,

    /// Symbol scope (functions, variables, both)
    #[schemars(
        description = "Symbol scope: 'functions' (non-variable symbols), 'variables', or 'both' (default: functions)"
    )]
    pub symbol_scope: Option<String>,

    /// Maximum symbols to validate for file/module scope (default: 100)
    #[schemars(
        description = "Maximum symbols to validate for file/module scope (default: 100, max: 500)"
    )]
    pub limit: Option<usize>,

    /// Include source code in response (default: false)
    #[schemars(description = "Include source code snippet in response")]
    pub include_source: Option<bool>,
}

/// Unified index request - smart refresh by default (checks freshness first).
/// Use `force: true` to always regenerate regardless of freshness.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Force regeneration even if index appears fresh (default: false = smart refresh)
    #[schemars(
        description = "Force regeneration even if index appears fresh (default: false = smart refresh)"
    )]
    pub force: Option<bool>,

    /// Maximum directory depth for file collection (default: 10)
    #[schemars(description = "Maximum directory depth for file collection (default: 10)")]
    pub max_depth: Option<usize>,

    /// File extensions to include (e.g., ["ts", "tsx", "rs"])
    #[schemars(
        description = "File extensions to include. If empty, all supported extensions are included."
    )]
    pub extensions: Option<Vec<String>>,

    /// Maximum age in seconds before considered stale (default: 3600 = 1 hour)
    #[schemars(
        description = "Maximum cache age in seconds for smart refresh (default: 3600 = 1 hour)"
    )]
    pub max_age: Option<u64>,
}

/// Unified test request - runs tests by default, use detect_only=true to only detect frameworks.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TestRequest {
    /// Path to the project directory (defaults to current directory)
    #[schemars(description = "Path to the project directory (defaults to current directory)")]
    pub path: Option<String>,

    /// Only detect test frameworks without running tests (default: false)
    #[schemars(
        description = "Only detect test frameworks without running (default: false = run tests)"
    )]
    pub detect_only: Option<bool>,

    /// Force a specific test framework (pytest, cargo, npm, vitest, jest, go)
    /// Only used when running tests (not for detect_only mode)
    #[schemars(
        description = "Force a specific test framework (pytest, cargo, npm, vitest, jest, go). Auto-detects if not specified."
    )]
    pub framework: Option<String>,

    /// Filter tests by name pattern (passed to test runner)
    #[schemars(
        description = "Filter tests by name pattern (e.g., 'test_auth' for pytest, 'auth' for cargo test)"
    )]
    pub filter: Option<String>,

    /// Run tests in verbose mode
    #[schemars(description = "Run tests in verbose mode (more output)")]
    pub verbose: Option<bool>,

    /// Maximum time to run tests in seconds (default: 300)
    #[schemars(description = "Maximum time to run tests in seconds (default: 300)")]
    pub timeout: Option<u64>,
}

// ============================================================================
// Lint Request
// ============================================================================

/// Unified linter - scans for issues by default (auto-detects framework).
/// Use detect_only=true to only detect available linters without running.
/// Use mode="fix" to apply auto-fixes.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LintRequest {
    /// Path to the project directory (defaults to current directory)
    #[schemars(description = "Path to the project directory (defaults to current directory)")]
    pub path: Option<String>,

    /// Lint mode: "scan" (default), "fix", "typecheck", or "recommend"
    #[schemars(
        description = "Lint mode: 'scan' (check for issues), 'fix' (auto-fix), 'typecheck' (type check only), 'recommend' (suggest linters). Default: scan"
    )]
    pub mode: Option<String>,

    /// Only detect linters without running (default: false)
    #[schemars(
        description = "Only detect available linters without running (default: false = run linters)"
    )]
    pub detect_only: Option<bool>,

    /// Force a specific linter (clippy, eslint, ruff, golangci-lint, etc.)
    #[schemars(
        description = "Force a specific linter (clippy, eslint, ruff, golangci-lint, etc.). Auto-detects if not specified."
    )]
    pub linter: Option<String>,

    /// Filter issues by severity (error, warning, info, hint)
    #[schemars(
        description = "Filter by severity levels (e.g., ['error', 'warning']). Shows all by default."
    )]
    pub severity_filter: Option<Vec<String>>,

    /// Maximum issues to return (default: 100)
    #[schemars(description = "Maximum issues to return (default: 100)")]
    pub limit: Option<usize>,

    /// Only show issues that can be auto-fixed (default: false)
    #[schemars(description = "Only show fixable issues (default: false)")]
    pub fixable_only: Option<bool>,

    /// Dry run for fix mode - show what would be fixed without changing files
    #[schemars(
        description = "Dry run - show what would be fixed without changing files (default: false)"
    )]
    pub dry_run: Option<bool>,

    /// Only apply safe fixes in fix mode (default: false)
    #[schemars(description = "Only apply safe auto-fixes (default: false)")]
    pub safe_only: Option<bool>,
}

// ============================================================================
// SecurityRequest - HIDDEN (internal use only, not exposed via MCP)
// ============================================================================
// Kept for potential future use. See src/commands/security.rs for implementation.

// ============================================================================
// Legacy Search Types (kept for backward compatibility during transition)
// TODO: Remove after MCP handler consolidation is complete
// ============================================================================

/// Search for symbols by name across the repository (lightweight, query-driven)
/// DEPRECATED: Use SearchRequest instead
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchSymbolsRequest {
    /// Search query - supports partial match, wildcards (* for any chars, ? for single char), or empty/"*" to match all
    #[schemars(
        description = "Search query - matches symbol names (case-insensitive). Supports: 'foo' (substring), '*Manager' (glob pattern), '*' or '' (match all)"
    )]
    pub query: String,

    /// Optional: filter by module name
    #[schemars(description = "Filter results to a specific module")]
    pub module: Option<String>,

    /// Optional: filter by symbol kind (fn, struct, component, enum, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Symbol scope (functions, variables, both)
    #[schemars(
        description = "Symbol scope: 'functions' (non-variable symbols), 'variables', or 'both' (default: functions)"
    )]
    pub symbol_scope: Option<String>,

    /// Optional: filter by risk level (high, medium, low)
    #[schemars(description = "Filter by risk level (high, medium, low)")]
    pub risk: Option<String>,

    /// Maximum results to return (default: 20, max: 100)
    #[schemars(description = "Maximum results to return (default: 20, max: 100)")]
    pub limit: Option<usize>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// If true, search only uncommitted files (working overlay mode)
    #[schemars(description = "If true, search only uncommitted files (real-time working overlay)")]
    pub working_overlay: Option<bool>,
}

/// Get symbols from a file or module (mutually exclusive parameters)
/// Use file_path for file-centric view, or module for module-centric view.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetFileRequest {
    /// Path to source file (mutually exclusive with module)
    #[schemars(
        description = "Path to the source file to analyze (mutually exclusive with module)"
    )]
    pub file_path: Option<String>,

    /// Module name (mutually exclusive with file_path)
    #[schemars(
        description = "Module name to list symbols from (mutually exclusive with file_path)"
    )]
    pub module: Option<String>,

    /// Optional: filter by symbol kind
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Symbol scope (functions, variables, both)
    #[schemars(
        description = "Symbol scope: 'functions' (non-variable symbols), 'variables', or 'both' (default: functions)"
    )]
    pub symbol_scope: Option<String>,

    /// Optional: filter by risk level (only applies to module mode)
    #[schemars(description = "Filter by risk level (high, medium, low)")]
    pub risk: Option<String>,

    /// Maximum results (default: 50, max: 200, only applies to module mode)
    #[schemars(description = "Maximum results to return (default: 50, max: 200)")]
    pub limit: Option<usize>,

    /// Include source code for each symbol (default: false)
    #[schemars(description = "Include source code snippets (default: false)")]
    pub include_source: Option<bool>,

    /// Context lines around source (default: 2)
    #[schemars(description = "Lines of context around symbol source (default: 2)")]
    pub context: Option<usize>,

    /// Include local variables that escape their scope
    #[schemars(description = "Include local variables that escape their scope (default: false)")]
    pub include_escape_refs: Option<bool>,

    /// Repository path
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

/// Request to check index staleness and optionally auto-refresh
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckIndexRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root")]
    pub path: Option<String>,

    /// If true, automatically regenerate stale index (default: false)
    #[schemars(description = "Automatically regenerate if stale (default: false)")]
    pub auto_refresh: Option<bool>,

    /// Maximum age in seconds before considered stale (default: 3600 = 1 hour)
    #[schemars(description = "Maximum cache age in seconds (default: 3600 = 1 hour)")]
    pub max_age: Option<u64>,
}

// ============================================================================
// Ripgrep Raw Search Types (SEM-55)
// ============================================================================

/// Request for direct ripgrep search (bypasses semantic index)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RawSearchRequest {
    /// Search pattern (regex supported)
    #[schemars(description = "Search pattern (regex supported)")]
    pub pattern: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to search in (defaults to current directory)")]
    pub path: Option<String>,

    /// File type filters (e.g., ["rs", "ts"])
    #[schemars(
        description = "File extensions to search (e.g., ['rs', 'ts']). If empty, searches all files."
    )]
    pub file_types: Option<Vec<String>>,

    /// Maximum results (default: 50, max: 200)
    #[schemars(description = "Maximum results to return (default: 50, max: 200)")]
    pub limit: Option<usize>,

    /// Case-insensitive search (default: true)
    #[schemars(description = "Case-insensitive search (default: true)")]
    pub case_insensitive: Option<bool>,

    /// Merge adjacent matches within N lines (default: 3)
    #[schemars(description = "Merge adjacent matches within N lines (default: 3, 0 to disable)")]
    pub merge_threshold: Option<usize>,
}

// ============================================================================
// Test Runner Types (North Star - VALIDATE phase)
// ============================================================================

/// Request to run tests in a project directory
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunTestsRequest {
    /// Path to the project directory (defaults to current directory)
    #[schemars(description = "Path to the project directory (defaults to current directory)")]
    pub path: Option<String>,

    /// Force a specific test framework (pytest, cargo, npm, vitest, jest, go)
    /// If not specified, auto-detects from project files
    #[schemars(
        description = "Force a specific test framework (pytest, cargo, npm, vitest, jest, go). Auto-detects if not specified."
    )]
    pub framework: Option<String>,

    /// Filter tests by name pattern (passed to test runner)
    #[schemars(
        description = "Filter tests by name pattern (e.g., 'test_auth' for pytest, 'auth' for cargo test)"
    )]
    pub filter: Option<String>,

    /// Run tests in verbose mode
    #[schemars(description = "Run tests in verbose mode (more output)")]
    pub verbose: Option<bool>,

    /// Maximum time to run tests in seconds (default: 300)
    #[schemars(description = "Maximum time to run tests in seconds (default: 300)")]
    pub timeout: Option<u64>,
}

/// Request to detect test frameworks in a directory
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DetectTestsRequest {
    /// Path to the project directory (defaults to current directory)
    #[schemars(description = "Path to the project directory (defaults to current directory)")]
    pub path: Option<String>,

    /// Check subdirectories for monorepo setups
    #[schemars(description = "Check subdirectories for monorepo setups (default: false)")]
    pub check_subdirs: Option<bool>,
}

// ============================================================================
// Server Status Request Type
// ============================================================================

/// Unified server status request - returns server mode info and optionally layer status
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ServerStatusRequest {
    /// Include detailed layer status (default: false)
    #[schemars(
        description = "Include detailed layer status (requires persistent mode, default: false)"
    )]
    pub include_layers: Option<bool>,
}

// ============================================================================
// Duplicate Detection Request Types
// ============================================================================

/// Unified duplicate detection: codebase-wide scan or single symbol check
/// - If symbol_hash is provided: check that specific symbol for duplicates
/// - If symbol_hash is None: scan entire codebase for duplicate clusters
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindDuplicatesRequest {
    /// Symbol hash to check for duplicates (if provided, does single-symbol check instead of codebase scan)
    #[schemars(
        description = "Symbol hash to check - if provided, returns duplicates for this symbol only; if omitted, scans entire codebase"
    )]
    pub symbol_hash: Option<String>,

    /// Minimum similarity threshold (default: 0.90)
    #[schemars(description = "Minimum similarity threshold (default: 0.90)")]
    pub threshold: Option<f64>,

    /// Whether to exclude boilerplate patterns (default: true, only for codebase scan)
    #[schemars(description = "Whether to exclude boilerplate patterns (default: true)")]
    pub exclude_boilerplate: Option<bool>,

    /// Filter to specific module (only for codebase scan)
    #[schemars(description = "Filter to specific module")]
    pub module: Option<String>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Repository path")]
    pub path: Option<String>,

    /// Minimum function lines to include (default: 3, only for codebase scan)
    #[schemars(description = "Minimum function lines to include (default: 3)")]
    pub min_lines: Option<u32>,

    /// Maximum clusters to return (default: 50, max: 200, only for codebase scan)
    #[schemars(description = "Maximum clusters to return (default: 50, max: 200)")]
    pub limit: Option<u32>,

    /// Pagination offset (default: 0, only for codebase scan)
    #[schemars(description = "Skip first N clusters for pagination (default: 0)")]
    pub offset: Option<u32>,

    /// Sort clusters by: "similarity" (default), "size", or "count" (only for codebase scan)
    #[schemars(
        description = "Sort by: 'similarity' (highest first), 'size' (largest functions), 'count' (most duplicates)"
    )]
    pub sort_by: Option<String>,
}

// ============================================================================
// AI-Optimized Query Types (Combined Operations)
// ============================================================================

/// Combined search + fetch: search symbols AND return full details with source in one call.
/// Eliminates the search_symbols -> get_symbol -> get_symbol_source round-trip.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchAndGetSymbolsRequest {
    /// Search query - supports partial match, wildcards (* for any chars, ? for single char)
    #[schemars(
        description = "Search query - matches symbol names (case-insensitive, partial match)"
    )]
    pub query: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Optional: filter by module name
    #[schemars(description = "Filter results to a specific module")]
    pub module: Option<String>,

    /// Optional: filter by symbol kind (fn, struct, component, enum, trait, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Symbol scope (functions, variables, both)
    #[schemars(
        description = "Symbol scope: 'functions' (non-variable symbols), 'variables', or 'both' (default: functions)"
    )]
    pub symbol_scope: Option<String>,

    /// Optional: filter by risk level (high, medium, low)
    #[schemars(description = "Filter by risk level (high, medium, low)")]
    pub risk: Option<String>,

    /// Maximum results to return (default: 10, max: 20)
    #[schemars(
        description = "Maximum results to return (default: 10, max: 20 for token efficiency)"
    )]
    pub limit: Option<usize>,

    /// Include source code for each symbol (default: true)
    #[schemars(description = "Include source code snippets (default: true)")]
    pub include_source: Option<bool>,

    /// Context lines around source (default: 3)
    #[schemars(description = "Lines of context around symbol source (default: 3)")]
    pub context: Option<usize>,
}

/// Get callers of a symbol - reverse call graph lookup.
/// Answers "what functions call this symbol?"
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCallersRequest {
    /// Symbol hash to find callers for
    #[schemars(
        description = "Symbol hash to find callers for (from search_symbols or get_file_symbols)"
    )]
    pub symbol_hash: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Maximum depth to traverse (default: 1, max: 3)
    #[schemars(
        description = "How many levels of callers to find (default: 1 = direct callers only, max: 3)"
    )]
    pub depth: Option<usize>,

    /// Maximum callers to return (default: 20, max: 50)
    #[schemars(description = "Maximum callers to return (default: 20, max: 50)")]
    pub limit: Option<usize>,

    /// Include source snippets for callers (default: false)
    #[schemars(description = "Include source code snippets for each caller (default: false)")]
    pub include_source: Option<bool>,
}

// ============================================================================
// Validation Request Types (Phase 4)
// ============================================================================

/// Request to validate a symbol's quality (complexity, duplicates, impact)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateSymbolRequest {
    /// Symbol hash to validate (from search_symbols or get_file_symbols)
    #[schemars(description = "Symbol hash to validate")]
    pub symbol_hash: Option<String>,

    /// Alternative: file path + line to find symbol
    #[schemars(description = "File path containing the symbol (alternative to symbol_hash)")]
    pub file_path: Option<String>,

    /// Line number within file (used with file_path)
    #[schemars(description = "Line number within the file to find the symbol")]
    pub line: Option<usize>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Similarity threshold for duplicate detection (default: 0.85)
    #[schemars(description = "Similarity threshold for finding duplicates (default: 0.85)")]
    pub duplicate_threshold: Option<f64>,

    /// Include source code in response (default: false)
    #[schemars(description = "Include source code snippet in response")]
    pub include_source: Option<bool>,
}

// ============================================================================
// BM25 Semantic Search Types (Phase 3)
// ============================================================================

/// Request for semantic search using BM25 ranking
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SemanticSearchRequest {
    /// Natural language query (e.g., "authentication", "error handling", "database connection")
    #[schemars(
        description = "Natural language query - searches symbol names, comments, strings, file paths"
    )]
    pub query: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Maximum results to return (default: 20, max: 50)
    #[schemars(description = "Maximum results to return (default: 20, max: 50)")]
    pub limit: Option<usize>,

    /// Include source code snippets (default: false)
    #[schemars(description = "Include source code snippets with results")]
    pub include_source: Option<bool>,

    /// Filter by symbol kind (fn, struct, component, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Filter by module
    #[schemars(description = "Filter results to a specific module")]
    pub module: Option<String>,
}

// ============================================================================
// Batch Validation Types (Phase 5)
// ============================================================================

/// Request to validate all symbols in a file
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateFileSymbolsRequest {
    /// Path to the source file to validate
    #[schemars(description = "Path to the source file to validate")]
    pub file_path: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Similarity threshold for duplicate detection (default: 0.85)
    #[schemars(description = "Similarity threshold for finding duplicates (default: 0.85)")]
    pub duplicate_threshold: Option<f64>,

    /// Filter by symbol kind (fn, struct, component, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,
}

/// Request to validate all symbols in a module
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateModuleSymbolsRequest {
    /// Module name to validate (e.g., 'api', 'components', 'lib')
    #[schemars(description = "Module name to validate (e.g., 'api', 'components', 'lib')")]
    pub module: String,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Similarity threshold for duplicate detection (default: 0.85)
    #[schemars(description = "Similarity threshold for finding duplicates (default: 0.85)")]
    pub duplicate_threshold: Option<f64>,

    /// Filter by symbol kind (fn, struct, component, etc.)
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Maximum symbols to validate (default: 100)
    #[schemars(description = "Maximum symbols to validate (default: 100, max: 500)")]
    pub limit: Option<usize>,
}

// ============================================================================
// Security / CVE Pattern Detection Types
// ============================================================================

/// Request to scan for CVE vulnerability patterns in the codebase
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CVEScanRequest {
    /// Repository path (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Minimum similarity threshold (default: 0.75)
    /// Higher values = fewer false positives but may miss variants
    #[schemars(description = "Minimum similarity threshold (default: 0.75, range: 0.0-1.0)")]
    pub min_similarity: Option<f32>,

    /// Filter by severity level(s): CRITICAL, HIGH, MEDIUM, LOW
    #[schemars(description = "Filter by severity levels (e.g., ['CRITICAL', 'HIGH'])")]
    pub severity_filter: Option<Vec<String>>,

    /// Filter by CWE categories (e.g., ['CWE-89', 'CWE-79'])
    #[schemars(description = "Filter by CWE categories (e.g., ['CWE-89', 'CWE-79'])")]
    pub cwe_filter: Option<Vec<String>>,

    /// Maximum matches to return (default: 100)
    #[schemars(description = "Maximum matches to return (default: 100)")]
    pub limit: Option<usize>,

    /// Filter to a specific module
    #[schemars(description = "Filter to a specific module")]
    pub module: Option<String>,
}

/// Request to update security patterns at runtime
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSecurityPatternsRequest {
    /// URL to fetch patterns from (defaults to SEMFORA_PATTERN_URL env or built-in default)
    #[schemars(
        description = "URL to fetch patterns from. If not provided, uses SEMFORA_PATTERN_URL environment variable or the default pattern server."
    )]
    pub url: Option<String>,

    /// Path to a local pattern file (alternative to URL)
    #[schemars(
        description = "Path to a local security_patterns.bin file. If provided, loads from file instead of HTTP."
    )]
    pub file_path: Option<String>,

    /// Force update even if versions match
    #[schemars(
        description = "Force update even if the pattern version matches current version (default: false)"
    )]
    pub force: Option<bool>,
}

/// Request to get security pattern statistics
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSecurityPatternStatsRequest {}

// ============================================================================
// Commit Preparation Types
// ============================================================================

/// Request to prepare commit information with semantic analysis.
///
/// This tool gathers all information needed to write a meaningful commit message:
/// - Git context (branch, last commit)
/// - Staged and unstaged changes with semantic analysis
/// - Optional complexity metrics for changed symbols
///
/// **Use before committing** to get a comprehensive view of changes.
/// This tool NEVER commits - it only provides information.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrepCommitRequest {
    /// Path to the repository root (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Include complexity metrics (cognitive, cyclomatic, max nesting) for changed symbols
    #[schemars(
        description = "Include complexity metrics (cognitive, cyclomatic, max nesting) for changed symbols (default: false)"
    )]
    pub include_complexity: Option<bool>,

    /// Include all detailed metrics (complexity + fan-out, LOC, state mutations, I/O ops)
    #[schemars(
        description = "Include all detailed metrics (complexity + fan-out, LOC, state mutations, I/O ops) (default: false)"
    )]
    pub include_all_metrics: Option<bool>,

    /// Only show staged changes (default: false, shows both staged and unstaged)
    #[schemars(
        description = "Only show staged changes, ignoring unstaged modifications (default: false, shows both)"
    )]
    pub staged_only: Option<bool>,

    /// Auto-refresh the index if stale before analysis
    #[schemars(
        description = "Auto-refresh the semantic index if stale before analysis (default: true)"
    )]
    pub auto_refresh_index: Option<bool>,

    /// Show diff statistics (insertions/deletions per file)
    #[schemars(
        description = "Show diff statistics (insertions/deletions per file) (default: true)"
    )]
    pub show_diff_stats: Option<bool>,
}

// ============================================================================
// Re-exports
// ============================================================================

// SymbolIndexEntry is defined in cache.rs and re-exported from lib.rs
