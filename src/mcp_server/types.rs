//! Request and response types for the MCP server
//!
//! This module contains all the request/response structs used by the MCP tools.

use rmcp::schemars;
use serde::Deserialize;

// ============================================================================
// Analysis Request Types
// ============================================================================

/// Request to analyze a single file
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeFileRequest {
    /// The absolute or relative path to the file to analyze
    #[schemars(description = "Path to the source file to analyze")]
    pub path: String,

    /// Output format: "toon" (default) or "json"
    #[schemars(description = "Output format: 'toon' (compact) or 'json' (structured)")]
    pub format: Option<String>,
}

/// Request to analyze a directory
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDirectoryRequest {
    /// The path to the directory to analyze
    #[schemars(description = "Path to the directory to analyze")]
    pub path: String,

    /// Maximum depth for recursive analysis (default: 10)
    #[schemars(description = "Maximum directory depth to traverse (default: 10)")]
    pub max_depth: Option<usize>,

    /// Whether to include only the summary overview
    #[schemars(description = "If true, only return the repository overview, not individual files")]
    pub summary_only: Option<bool>,

    /// File extensions to include (e.g., ["ts", "tsx", "js"])
    #[schemars(
        description = "File extensions to include (e.g., ['ts', 'tsx']). If empty, all supported extensions are included."
    )]
    pub extensions: Option<Vec<String>>,
}

/// Request to analyze git diff
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDiffRequest {
    /// The base branch or commit to compare against (e.g., "main", "HEAD~1").
    /// Use "HEAD" with target_ref "WORKING" to see uncommitted changes.
    #[schemars(description = "Base branch or commit to compare against (e.g., 'main', 'HEAD~1'). Use 'HEAD' with target_ref='WORKING' for uncommitted changes.")]
    pub base_ref: String,

    /// The target branch or commit. Use "WORKING" to compare against uncommitted changes in the working tree.
    /// Defaults to "HEAD" for committed changes.
    #[schemars(description = "Target branch or commit (defaults to 'HEAD'). Use 'WORKING' to analyze uncommitted changes vs base_ref.")]
    pub target_ref: Option<String>,

    /// Working directory (defaults to current directory)
    #[schemars(description = "Working directory for git operations (defaults to current directory)")]
    pub working_dir: Option<String>,
}

/// Request to list supported languages
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLanguagesRequest {}

// ============================================================================
// Sharded Index Request Types
// ============================================================================

/// Request to get repository overview from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetRepoOverviewRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

/// Request to get a module from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetModuleRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Name of the module to retrieve (e.g., "api", "components", "lib")
    #[schemars(description = "Module name (e.g., 'api', 'components', 'lib', 'tests')")]
    pub module_name: String,
}

/// Request to get a symbol from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSymbolRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Symbol hash (from repo_overview or module listing)
    #[schemars(description = "Symbol hash from the repo overview or module shard")]
    pub symbol_hash: String,
}

/// Request to list modules in a sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListModulesRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
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

/// Request to get call graph from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCallGraphRequest {
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
    #[schemars(description = "Return only statistics (edge count, top callers) without full edge list")]
    pub summary_only: Option<bool>,
}

/// Request to get source code for a symbol (surgical read)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSymbolSourceRequest {
    /// Path to the source file
    #[schemars(description = "Path to the source file containing the symbol")]
    pub file_path: String,

    /// Start line (1-indexed). If provided with end_line, reads that range.
    #[schemars(description = "Start line number (1-indexed)")]
    pub start_line: Option<usize>,

    /// End line (1-indexed, inclusive)
    #[schemars(description = "End line number (1-indexed, inclusive)")]
    pub end_line: Option<usize>,

    /// Symbol hash to look up (alternative to line numbers)
    #[schemars(description = "Symbol hash from the index - will look up line range automatically")]
    pub symbol_hash: Option<String>,

    /// Context lines to include before/after the symbol (default: 5)
    #[schemars(description = "Number of context lines before and after the symbol (default: 5)")]
    pub context: Option<usize>,
}

// ============================================================================
// Query-Driven API Types
// ============================================================================

/// Search for symbols by name across the repository (lightweight, query-driven)
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

/// List all symbols in a specific module (lightweight index only)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSymbolsRequest {
    /// Module name to list symbols from
    #[schemars(description = "Module name to list symbols from (e.g., 'api', 'components')")]
    pub module: String,

    /// Optional: filter by symbol kind
    #[schemars(description = "Filter by symbol kind (fn, struct, component, enum, trait, etc.)")]
    pub kind: Option<String>,

    /// Optional: filter by risk level
    #[schemars(description = "Filter by risk level (high, medium, low)")]
    pub risk: Option<String>,

    /// Maximum results (default: 50, max: 200)
    #[schemars(description = "Maximum results to return (default: 50, max: 200)")]
    pub limit: Option<usize>,

    /// Repository path
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

// ============================================================================
// Batch Operation Types
// ============================================================================

/// Request to get multiple symbols at once (batch operation)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSymbolsRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Symbol hashes to retrieve (max 20)
    #[schemars(description = "Array of symbol hashes to retrieve (max 20)")]
    pub hashes: Vec<String>,

    /// If true, include source code snippets (default: false)
    #[schemars(description = "If true, include source code for each symbol")]
    pub include_source: Option<bool>,

    /// Context lines for source (default: 3)
    #[schemars(description = "Context lines before/after symbol source (default: 3)")]
    pub context: Option<usize>,
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
    #[schemars(description = "File extensions to search (e.g., ['rs', 'ts']). If empty, searches all files.")]
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
    #[schemars(description = "Force a specific test framework (pytest, cargo, npm, vitest, jest, go). Auto-detects if not specified.")]
    pub framework: Option<String>,

    /// Filter tests by name pattern (passed to test runner)
    #[schemars(description = "Filter tests by name pattern (e.g., 'test_auth' for pytest, 'auth' for cargo test)")]
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
// Layer Management Request Types (SEM-98, SEM-99, SEM-101, SEM-102, SEM-104)
// ============================================================================

/// Request to get layer status
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetLayerStatusRequest {
    // No parameters needed - returns status of all layers
}

/// Request to check server mode
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckServerModeRequest {
    // No parameters needed - returns server mode info
}

// ============================================================================
// Duplicate Detection Request Types
// ============================================================================

/// Request to find all duplicate function clusters in repository
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindDuplicatesRequest {
    /// Minimum similarity threshold (default: 0.90)
    #[schemars(description = "Minimum similarity threshold (default: 0.90)")]
    pub threshold: Option<f64>,

    /// Whether to exclude boilerplate patterns (default: true)
    #[schemars(description = "Whether to exclude boilerplate patterns (default: true)")]
    pub exclude_boilerplate: Option<bool>,

    /// Filter to specific module
    #[schemars(description = "Filter to specific module")]
    pub module: Option<String>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Repository path")]
    pub path: Option<String>,

    /// Minimum function lines to include (default: 3, filters out trivial functions)
    #[schemars(description = "Minimum function lines to include (default: 3)")]
    pub min_lines: Option<u32>,

    /// Maximum clusters to return (default: 50, max: 200)
    #[schemars(description = "Maximum clusters to return (default: 50, max: 200)")]
    pub limit: Option<u32>,

    /// Pagination offset (default: 0)
    #[schemars(description = "Skip first N clusters for pagination (default: 0)")]
    pub offset: Option<u32>,

    /// Sort clusters by: "similarity" (default), "size", or "count"
    #[schemars(description = "Sort by: 'similarity' (highest first), 'size' (largest functions), 'count' (most duplicates)")]
    pub sort_by: Option<String>,
}

/// Request to check if a specific function has duplicates
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckDuplicatesRequest {
    /// Symbol hash to check
    #[schemars(description = "Symbol hash to check")]
    pub symbol_hash: String,

    /// Minimum similarity threshold (default: 0.90)
    #[schemars(description = "Minimum similarity threshold (default: 0.90)")]
    pub threshold: Option<f64>,

    /// Repository path (defaults to current directory)
    #[schemars(description = "Repository path")]
    pub path: Option<String>,
}

// ============================================================================
// Re-exports
// ============================================================================

// SymbolIndexEntry is defined in cache.rs and re-exported from lib.rs
