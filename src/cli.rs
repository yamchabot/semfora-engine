//! CLI argument definitions using clap with subcommand architecture
//!
//! This module defines the command-line interface for semfora-engine using
//! a subcommand-based structure for better organization and discoverability.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Semantic code analyzer with TOON output
#[derive(Parser, Debug)]
#[command(name = "semfora")]
#[command(about = "Deterministic semantic code analyzer that outputs TOON-formatted summaries")]
#[command(version)]
#[command(author)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,

    /// Output format (applies to all commands)
    #[arg(short, long, default_value = "text", value_enum, global = true)]
    pub format: OutputFormat,

    /// Show verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Show progress percentage during long operations
    #[arg(long, global = true)]
    pub progress: bool,
}

// ============================================
// Main Commands Enum
// ============================================

/// Available subcommands for semfora
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze files, directories, or git changes
    #[command(visible_alias = "a")]
    Analyze(AnalyzeArgs),

    /// Search for code (runs both symbol and semantic search by default)
    #[command(visible_alias = "s")]
    Search(SearchArgs),

    /// Query the semantic index (symbols, source, callers, etc.)
    #[command(visible_alias = "q")]
    Query(QueryArgs),

    /// Trace symbol usage across the call graph
    Trace(TraceArgs),

    /// Run quality audits (complexity, duplicates)
    #[command(visible_alias = "v")]
    Validate(ValidateArgs),

    /// Manage the semantic index
    Index(IndexArgs),

    /// Manage the cache
    Cache(CacheArgs),

    // Security command hidden - internal use only
    // Security(SecurityArgs),
    /// Run or detect tests
    Test(TestArgs),

    /// Run linters to check code quality
    #[command(visible_alias = "l")]
    Lint(LintArgs),

    /// Prepare information for writing a commit message
    Commit(CommitArgs),

    /// Setup semfora-engine installation and MCP client configuration
    Setup(SetupArgs),

    /// Uninstall semfora-engine or remove MCP configurations
    Uninstall(UninstallArgs),

    /// Manage semfora-engine configuration
    Config(ConfigArgs),

    /// Run token efficiency benchmark
    Benchmark(BenchmarkArgs),

    /// Start the MCP server (for AI coding assistants)
    Serve(ServeArgs),
}

// ============================================
// Analyze Subcommand
// ============================================

/// Arguments for the analyze command
#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Path to file or directory to analyze
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Analyze git diff against a reference (auto-detects main/master if not specified)
    #[arg(long, value_name = "REF", num_args = 0..=1, default_missing_value = "auto")]
    pub diff: Option<String>,

    /// Analyze uncommitted changes (working directory vs HEAD)
    #[arg(long)]
    pub uncommitted: bool,

    /// Analyze a specific commit
    #[arg(long, value_name = "SHA")]
    pub commit: Option<String>,

    /// Analyze all commits on current branch since base
    #[arg(long)]
    pub all_commits: bool,

    /// Base branch for diff comparison
    #[arg(long, value_name = "BRANCH")]
    pub base: Option<String>,

    /// Target ref to compare against (defaults to HEAD, use WORKING for uncommitted)
    #[arg(long, value_name = "REF")]
    pub target_ref: Option<String>,

    /// Maximum number of files to show in diff output (pagination)
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Offset for diff pagination (skip first N files)
    #[arg(long, value_name = "N")]
    pub offset: Option<usize>,

    /// Maximum directory depth for recursive scan (default: 10)
    #[arg(long, default_value = "10")]
    pub max_depth: usize,

    /// Filter by file extension (can be repeated)
    #[arg(long = "ext", value_name = "EXT")]
    pub extensions: Vec<String>,

    /// Include test files in analysis (excluded by default)
    #[arg(long)]
    pub allow_tests: bool,

    /// Show summary statistics only (no per-file details)
    #[arg(long)]
    pub summary_only: bool,

    /// Start line for focused analysis (file mode only)
    #[arg(long, value_name = "LINE")]
    pub start_line: Option<usize>,

    /// End line for focused analysis (file mode only)
    #[arg(long, value_name = "LINE")]
    pub end_line: Option<usize>,

    /// Output mode: 'full' (default), 'symbols_only', or 'summary'
    #[arg(long, value_name = "MODE", default_value = "full")]
    pub output_mode: String,

    /// Generate sharded index (writes to cache directory)
    #[arg(long)]
    pub shard: bool,

    /// Incremental indexing: only re-index changed files
    #[arg(long, requires = "shard")]
    pub incremental: bool,

    /// Analyze AI token counts
    #[arg(long, value_enum)]
    pub analyze_tokens: Option<TokenAnalysisMode>,

    /// Include compact JSON in token analysis comparison
    #[arg(long, requires = "analyze_tokens")]
    pub compare_compact: bool,

    /// Print the parsed AST (for debugging)
    #[arg(long)]
    pub print_ast: bool,
}

// ============================================
// Search Subcommand (Hybrid Search)
// ============================================

/// Arguments for the search command - runs both symbol and semantic search by default
#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query (searches both symbol names and code semantically)
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Only show exact symbol name matches
    #[arg(short, long)]
    pub symbols: bool,

    /// Only show semantically related code
    #[arg(short, long)]
    pub related: bool,

    /// Use raw regex search (for comments, strings, patterns)
    #[arg(long)]
    pub raw: bool,

    /// Filter by symbol kind (fn, struct, component, etc.)
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,

    /// Filter by module name
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,

    /// Filter by risk level (high, medium, low)
    #[arg(long, value_name = "RISK")]
    pub risk: Option<String>,

    /// Include source code snippets in output
    #[arg(long)]
    pub include_source: bool,

    /// Limit number of results (default: 20)
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// File types to search (for raw search, e.g., "rs,ts,py")
    #[arg(long, value_name = "TYPES")]
    pub file_types: Option<String>,

    /// Case sensitive search
    #[arg(long)]
    pub case_sensitive: bool,

    /// Merge adjacent matches within N lines (for raw search)
    #[arg(long, default_value = "3")]
    pub merge_threshold: usize,

    /// Symbol scope for search results (functions = non-variable symbols)
    #[arg(long, value_enum, default_value = "functions")]
    pub symbol_scope: SymbolScope,

    /// Include local variables that escape their scope
    #[arg(long)]
    pub include_escape_refs: bool,
}

// ============================================
// Query Subcommand
// ============================================

/// Arguments for the query command
#[derive(Args, Debug)]
pub struct QueryArgs {
    /// Query type to execute
    #[command(subcommand)]
    pub query_type: QueryType,
}

// ============================================
// Trace Subcommand
// ============================================

/// Arguments for the trace command
#[derive(Args, Debug)]
pub struct TraceArgs {
    /// Symbol hash or name to trace
    #[arg(value_name = "TARGET")]
    pub target: String,

    /// Target kind (function, variable, component, module, file, etc.)
    #[arg(long)]
    pub kind: Option<String>,

    /// Trace direction (incoming, outgoing, both)
    #[arg(long, value_enum, default_value = "both")]
    pub direction: TraceDirection,

    /// Maximum depth to traverse
    #[arg(long, default_value = "2")]
    pub depth: usize,

    /// Maximum edges to return
    #[arg(long, default_value = "200")]
    pub limit: usize,

    /// Skip first N edges (for pagination)
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Include local variables that escape their scope
    #[arg(long)]
    pub include_escape_refs: bool,

    /// Include external nodes (ext:*)
    #[arg(long)]
    pub include_external: bool,

    /// Path to repository (defaults to current directory)
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum TraceDirection {
    Incoming,
    Outgoing,
    Both,
}

/// Types of queries available
#[derive(Subcommand, Debug)]
pub enum QueryType {
    /// Get repository overview
    Overview {
        /// Path to repository (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Include full module list
        #[arg(long)]
        modules: bool,

        /// Maximum modules to show (default: 30)
        #[arg(long, default_value = "30")]
        max_modules: usize,

        /// Exclude test directories from module list (default: true)
        #[arg(long, default_value = "true")]
        exclude_test_dirs: bool,

        /// Include git context (branch, last commit) in output (default: true)
        #[arg(long, default_value = "true")]
        include_git_context: bool,
    },

    /// Get a specific module's details
    Module {
        /// Module name
        name: String,

        /// Show only symbols (not full details)
        #[arg(long)]
        symbols: bool,

        /// Filter by symbol kind
        #[arg(long)]
        kind: Option<String>,

        /// Filter by risk level
        #[arg(long)]
        risk: Option<String>,

        /// Limit number of results
        #[arg(long, default_value = "50")]
        limit: usize,

        /// Symbol scope (functions = non-variable symbols)
        #[arg(long, value_enum, default_value = "functions")]
        symbol_scope: SymbolScope,

        /// Include local variables that escape their scope
        #[arg(long)]
        include_escape_refs: bool,
    },

    /// Get a specific symbol by hash or file+line location
    Symbol {
        /// Symbol hash (or multiple comma-separated hashes)
        hash: Option<String>,

        /// Path to repository (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// File path (for location-based lookup)
        #[arg(long)]
        file: Option<String>,

        /// Line number (for location-based lookup, requires --file)
        #[arg(long)]
        line: Option<usize>,

        /// Include source code
        #[arg(long)]
        source: bool,

        /// Lines of context for source code
        #[arg(long, default_value = "3")]
        context: usize,
    },

    /// Get source code for a file or symbol(s)
    Source {
        /// File path (optional if hash/hashes provided)
        file: Option<String>,

        /// Path to repository (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Start line (1-indexed, requires file)
        #[arg(long)]
        start: Option<usize>,

        /// End line (1-indexed, inclusive, requires file)
        #[arg(long)]
        end: Option<usize>,

        /// Symbol hash to get source for (comma-separated for batch)
        #[arg(long)]
        hash: Option<String>,

        /// Context lines before/after
        #[arg(long, default_value = "5")]
        context: usize,
    },

    /// Get callers of a symbol (reverse call graph)
    Callers {
        /// Symbol hash
        hash: String,

        /// Path to repository (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Depth (1=direct only, max 3)
        #[arg(long, default_value = "1")]
        depth: usize,

        /// Include source code snippets
        #[arg(long)]
        source: bool,

        /// Maximum callers to return
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Get the call graph
    Callgraph {
        /// Path to repository (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Filter to a specific module
        #[arg(long)]
        module: Option<String>,

        /// Filter to edges involving this symbol (name or hash)
        #[arg(long)]
        symbol: Option<String>,

        /// Export to SQLite file
        #[arg(long, value_name = "PATH")]
        export: Option<String>,

        /// Return only statistics (summary mode)
        #[arg(long)]
        stats_only: bool,

        /// Maximum edges to return
        #[arg(long, default_value = "500")]
        limit: usize,

        /// Skip first N edges (for pagination)
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Include local variables that escape their scope (passed/returned)
        #[arg(long)]
        include_escape_refs: bool,
    },

    /// Get all symbols in a file
    File {
        /// File path
        path: String,

        /// Path to repository (defaults to current directory)
        #[arg(long)]
        repo_path: Option<PathBuf>,

        /// Include source code snippets
        #[arg(long)]
        source: bool,

        /// Filter by symbol kind (function, struct, class, etc.)
        #[arg(long)]
        kind: Option<String>,

        /// Filter by risk level (low, medium, high)
        #[arg(long)]
        risk: Option<String>,

        /// Lines of context for source snippets
        #[arg(long, default_value = "2")]
        context: usize,

        /// Symbol scope (functions = non-variable symbols)
        #[arg(long, value_enum, default_value = "functions")]
        symbol_scope: SymbolScope,

        /// Include local variables that escape their scope
        #[arg(long)]
        include_escape_refs: bool,
    },

    /// List supported languages
    Languages,
}

// ============================================
// Validate Subcommand
// ============================================

/// Arguments for the validate command
#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Path to file, module name, or symbol hash to validate
    /// (auto-detects what type of validation to perform)
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Repository path (defaults to current directory)
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Symbol hash for single symbol validation
    #[arg(long)]
    pub symbol_hash: Option<String>,

    /// File path for file-level or symbol validation
    #[arg(long)]
    pub file_path: Option<String>,

    /// Line number for symbol at location (requires --file-path)
    #[arg(long)]
    pub line: Option<usize>,

    /// Module name for module-level validation
    #[arg(long)]
    pub module: Option<String>,

    /// Include source snippet in output
    #[arg(long)]
    pub include_source: bool,

    /// Find duplicate code patterns across the codebase
    #[arg(long)]
    pub duplicates: bool,

    /// Similarity threshold for duplicate detection (default: 0.90)
    #[arg(long, default_value = "0.90")]
    pub threshold: f64,

    /// Include boilerplate functions in duplicate detection
    #[arg(long)]
    pub include_boilerplate: bool,

    /// Filter by symbol kind
    #[arg(long)]
    pub kind: Option<String>,

    /// Symbol scope (functions = non-variable symbols)
    #[arg(long, value_enum, default_value = "functions")]
    pub symbol_scope: SymbolScope,

    /// Maximum clusters to return (default: 50)
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Pagination offset (skip first N clusters)
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Minimum function lines to include (default: 3)
    #[arg(long, default_value = "3")]
    pub min_lines: usize,

    /// Sort clusters by: similarity (default), size, or count
    #[arg(long, default_value = "similarity")]
    pub sort_by: String,
}

// ============================================
// Index Subcommand
// ============================================

/// Arguments for the index command
#[derive(Args, Debug)]
pub struct IndexArgs {
    /// Index operation to perform
    #[command(subcommand)]
    pub operation: IndexOperation,
}

/// Index operations
#[derive(Subcommand, Debug)]
pub enum IndexOperation {
    /// Generate or refresh the semantic index
    Generate {
        /// Path to index (default: current directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Force regeneration even if index is fresh
        #[arg(long)]
        force: bool,

        /// Use incremental indexing (only changed files)
        #[arg(long)]
        incremental: bool,

        /// Maximum directory depth
        #[arg(long, default_value = "10")]
        max_depth: usize,

        /// Filter by file extension
        #[arg(long = "ext")]
        extensions: Vec<String>,
    },

    /// Check if the index is fresh or stale
    Check {
        /// Auto-refresh if stale
        #[arg(long)]
        auto_refresh: bool,

        /// Maximum cache age in seconds (default: 3600)
        #[arg(long, default_value = "3600")]
        max_age: u64,
    },

    /// Export the index to SQLite
    Export {
        /// Output file path
        #[arg(value_name = "PATH")]
        path: Option<String>,
    },
}

// ============================================
// Cache Subcommand
// ============================================

/// Arguments for the cache command
#[derive(Args, Debug)]
pub struct CacheArgs {
    /// Cache operation to perform
    #[command(subcommand)]
    pub operation: CacheOperation,
}

/// Cache operations
#[derive(Subcommand, Debug)]
pub enum CacheOperation {
    /// Show cache information
    Info,

    /// Clear the cache for the current directory
    Clear,

    /// Prune caches older than N days
    Prune {
        /// Number of days
        days: u32,
    },
}

// ============================================
// Security Subcommand (HIDDEN from CLI - internal use only)
// ============================================
// Types kept for internal use by src/commands/security.rs

/// Arguments for the security command (internal use only)
#[derive(Args, Debug)]
pub struct SecurityArgs {
    /// Security operation to perform
    #[command(subcommand)]
    pub operation: SecurityOperation,
}

/// Security operations (internal use only)
#[derive(Subcommand, Debug)]
pub enum SecurityOperation {
    /// Scan for CVE vulnerability patterns
    Scan {
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        severity: Option<Vec<String>>,
        #[arg(long)]
        cwe: Option<Vec<String>>,
        #[arg(long, default_value = "0.75")]
        min_similarity: f32,
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Update security patterns from pattern server
    Update {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Show security pattern statistics
    Stats,
}

// ============================================
// Test Subcommand
// ============================================

/// Arguments for the test command
#[derive(Args, Debug)]
pub struct TestArgs {
    /// Test filter pattern
    #[arg(value_name = "FILTER")]
    pub filter: Option<String>,

    /// Only detect test framework, don't run tests
    #[arg(long)]
    pub detect: bool,

    /// Force a specific test framework
    #[arg(long)]
    pub framework: Option<String>,

    /// Run in verbose mode
    #[arg(long)]
    pub test_verbose: bool,

    /// Maximum time to run tests in seconds
    #[arg(long, default_value = "300")]
    pub timeout: u64,

    /// Path to project directory
    #[arg(long)]
    pub path: Option<PathBuf>,
}

// ============================================
// Lint Subcommand
// ============================================

/// Arguments for the lint command
#[derive(Args, Debug)]
pub struct LintArgs {
    #[command(subcommand)]
    pub operation: LintOperation,
}

#[derive(Subcommand, Debug)]
pub enum LintOperation {
    /// Scan for issues (default)
    Scan {
        /// Path to project directory
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Only run this specific linter
        #[arg(long)]
        linter: Option<String>,

        /// Filter by severity (error, warning, info, hint)
        #[arg(long)]
        severity: Option<Vec<String>>,

        /// Maximum issues to return
        #[arg(long, default_value = "100")]
        limit: usize,

        /// Filter to specific file
        #[arg(long)]
        file: Option<String>,

        /// Only show fixable issues
        #[arg(long)]
        fixable_only: bool,
    },

    /// Apply automatic fixes
    Fix {
        /// Path to project directory
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Only run this specific linter
        #[arg(long)]
        linter: Option<String>,

        /// Show what would be fixed without applying changes
        #[arg(long)]
        dry_run: bool,

        /// Only apply safe fixes
        #[arg(long)]
        safe_only: bool,
    },

    /// Run type checkers only
    Typecheck {
        /// Path to project directory
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Force a specific type checker
        #[arg(long)]
        checker: Option<String>,

        /// Maximum issues to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Detect available linters
    Detect {
        /// Path to project directory
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },

    /// Get recommendations for missing linters
    Recommend {
        /// Path to project directory
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
}

// ============================================
// Commit Subcommand
// ============================================

/// Arguments for the commit command (prep-commit)
#[derive(Args, Debug)]
pub struct CommitArgs {
    /// Repository path (defaults to current directory)
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Only show staged changes (default: shows both staged and unstaged)
    #[arg(long)]
    pub staged: bool,

    /// Include complexity metrics (cognitive, cyclomatic, max nesting)
    #[arg(long)]
    pub metrics: bool,

    /// Include all detailed metrics (complexity + fan-out, LOC, state mutations, I/O)
    #[arg(long)]
    pub all_metrics: bool,

    /// Skip auto-refresh of the semantic index
    #[arg(long)]
    pub no_auto_refresh: bool,

    /// Hide diff statistics (insertions/deletions per file)
    #[arg(long)]
    pub no_diff_stats: bool,
}

// ============================================
// Setup Subcommand (existing)
// ============================================

/// Arguments for the setup command
#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Run in non-interactive mode (use with --clients)
    #[arg(long)]
    pub non_interactive: bool,

    /// MCP clients to configure (comma-separated)
    /// Available: claude-desktop, claude-code, cursor, vscode, openai-codex
    #[arg(long, value_delimiter = ',')]
    pub clients: Option<Vec<String>>,

    /// Export MCP config to a custom path
    #[arg(long, value_name = "PATH")]
    pub export_config: Option<PathBuf>,

    /// Override the server binary path
    #[arg(long, value_name = "PATH")]
    pub binary_path: Option<PathBuf>,

    /// Override the cache directory
    #[arg(long, value_name = "PATH")]
    pub cache_dir: Option<PathBuf>,

    /// Log level for MCP server (error, info, debug)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Dry run - show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// List available MCP clients
    #[arg(long)]
    pub list_clients: bool,

    /// Install Semfora workflow agents (subagents for AI assistants)
    #[arg(long)]
    pub with_agents: bool,

    /// Agent installation scope: global, project, or both
    #[arg(long, value_enum, default_value = "global")]
    pub agents_scope: AgentScopeArg,

    /// Only install agents, skip MCP server configuration
    #[arg(long)]
    pub agents_only: bool,
}

/// Agent installation scope for CLI
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum AgentScopeArg {
    /// Install agents globally (~/.claude/agents/, ~/.cursor/rules/, etc.)
    #[default]
    Global,
    /// Install agents in current project (.claude/agents/, .cursor/rules/, etc.)
    Project,
    /// Install agents in both global and project locations
    Both,
}

// ============================================
// Uninstall Subcommand (existing)
// ============================================

/// Arguments for the uninstall command
#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// What to uninstall: mcp, engine, or all
    #[arg(value_name = "TARGET", default_value = "mcp")]
    pub target: String,

    /// Specific client to remove (only for mcp target)
    #[arg(long, value_name = "CLIENT")]
    pub client: Option<String>,

    /// Keep cache data when removing engine
    #[arg(long)]
    pub keep_cache: bool,

    /// Skip confirmation prompts
    #[arg(long, short)]
    pub force: bool,
}

// ============================================
// Config Subcommand (existing)
// ============================================

/// Arguments for the config command
#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Config operation: show, set, reset
    #[command(subcommand)]
    pub operation: ConfigOperation,
}

/// Config subcommand operations
#[derive(Subcommand, Debug)]
pub enum ConfigOperation {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., cache.dir, logging.level)
        key: String,
        /// Value to set
        value: String,
    },

    /// Reset configuration to defaults
    Reset,
}

// ============================================
// Benchmark Subcommand
// ============================================

/// Arguments for the benchmark command
#[derive(Args, Debug)]
pub struct BenchmarkArgs {
    /// Path to directory to benchmark
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
}

// ============================================
// Serve Subcommand (MCP Server)
// ============================================

/// Arguments for the serve command (MCP server mode)
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Repository path to serve (default: current directory)
    #[arg(short, long, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// Disable file watcher for live index updates
    #[arg(long)]
    pub no_watch: bool,

    /// Disable git polling for branch/commit changes
    #[arg(long)]
    pub no_git_poll: bool,
}

// ============================================
// Shared Types
// ============================================

/// Token analysis output mode
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TokenAnalysisMode {
    /// Full detailed report with breakdown
    Full,
    /// Compact single-line summary
    Compact,
}

/// Output format options
#[derive(Clone, Copy, Debug, Default, PartialEq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text with visual formatting (default for terminal)
    #[default]
    #[value(alias = "pretty")]
    Text,
    /// TOON (Token-Oriented Object Notation) - token-efficient format for AI consumption
    Toon,
    /// JSON - standard JSON output for machine parsing
    Json,
}

// ============================================
// Helper Implementations
// ============================================

impl Cli {
    /// Parse CLI arguments from command line
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

impl AnalyzeArgs {
    /// Check if we're in git mode
    pub fn is_git_mode(&self) -> bool {
        self.diff.is_some() || self.commit.is_some() || self.all_commits || self.uncommitted
    }

    /// Check if a file extension should be processed
    pub fn should_process_extension(&self, ext: &str) -> bool {
        if self.extensions.is_empty() {
            return true;
        }
        self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

impl SearchArgs {
    /// Determine the search mode based on flags
    pub fn search_mode(&self) -> SearchMode {
        if self.raw {
            SearchMode::Raw
        } else if self.symbols && !self.related {
            SearchMode::SymbolsOnly
        } else if self.related && !self.symbols {
            SearchMode::SemanticOnly
        } else {
            SearchMode::Hybrid // Default: run both
        }
    }

    /// Create SearchArgs for symbol-only search (used by MCP search_symbols)
    pub fn for_symbols(
        query: String,
        module: Option<String>,
        kind: Option<String>,
        risk: Option<String>,
        limit: usize,
    ) -> Self {
        Self {
            query,
            symbols: true,
            related: false,
            raw: false,
            kind,
            module,
            risk,
            include_source: false,
            limit,
            file_types: None,
            case_sensitive: false,
            merge_threshold: 3,
            symbol_scope: SymbolScope::Functions,
            include_escape_refs: false,
        }
    }

    /// Create SearchArgs for semantic-only search (used by MCP semantic_search)
    pub fn for_semantic(
        query: String,
        module: Option<String>,
        kind: Option<String>,
        include_source: bool,
        limit: usize,
    ) -> Self {
        Self {
            query,
            symbols: false,
            related: true,
            raw: false,
            kind,
            module,
            risk: None,
            include_source,
            limit,
            file_types: None,
            case_sensitive: false,
            merge_threshold: 3,
            symbol_scope: SymbolScope::Functions,
            include_escape_refs: false,
        }
    }

    /// Create SearchArgs for raw regex search (used by MCP raw_search)
    pub fn for_raw(
        pattern: String,
        file_types: Option<String>,
        case_sensitive: bool,
        limit: usize,
        merge_threshold: usize,
    ) -> Self {
        Self {
            query: pattern,
            symbols: false,
            related: false,
            raw: true,
            kind: None,
            module: None,
            risk: None,
            include_source: false,
            limit,
            file_types,
            case_sensitive,
            merge_threshold,
            symbol_scope: SymbolScope::Functions,
            include_escape_refs: false,
        }
    }

    /// Create SearchArgs for hybrid search with source (used by MCP search_and_get_symbols)
    pub fn for_hybrid_with_source(
        query: String,
        module: Option<String>,
        kind: Option<String>,
        risk: Option<String>,
        limit: usize,
    ) -> Self {
        Self {
            query,
            symbols: false,
            related: false,
            raw: false,
            kind,
            module,
            risk,
            include_source: true,
            limit,
            file_types: None,
            case_sensitive: false,
            merge_threshold: 3,
            symbol_scope: SymbolScope::Functions,
            include_escape_refs: false,
        }
    }
}

/// Scope of symbols to include in heavy query outputs
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SymbolScope {
    Functions,
    Variables,
    Both,
}

impl SymbolScope {
    pub fn matches_kind(self, kind: &str) -> bool {
        let is_variable = kind.eq_ignore_ascii_case("variable");
        match self {
            SymbolScope::Functions => !is_variable,
            SymbolScope::Variables => is_variable,
            SymbolScope::Both => true,
        }
    }

    pub fn from_optional(value: Option<&str>) -> Self {
        match value.map(|v| v.to_ascii_lowercase()) {
            Some(v) if v == "variables" || v == "variable" || v == "vars" || v == "var" => {
                SymbolScope::Variables
            }
            Some(v) if v == "both" || v == "all" => SymbolScope::Both,
            _ => SymbolScope::Functions,
        }
    }

    pub fn for_kind(self, kind: Option<&str>) -> Self {
        if kind
            .map(|k| k.eq_ignore_ascii_case("variable"))
            .unwrap_or(false)
        {
            SymbolScope::Variables
        } else {
            self
        }
    }
}

/// Search mode determined from flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Run both symbol and semantic search (default)
    Hybrid,
    /// Only exact symbol name matches
    SymbolsOnly,
    /// Only semantically related code
    SemanticOnly,
    /// Raw regex search
    Raw,
}

// ============================================
// Legacy Operation Mode (for transition)
// ============================================

/// Operation mode determined from CLI arguments (legacy support)
#[derive(Debug, Clone)]
pub enum OperationMode {
    /// Analyze a single file
    SingleFile(PathBuf),
    /// Analyze all files in a directory
    Directory { path: PathBuf, max_depth: usize },
    /// Diff current branch against base
    DiffBranch { base_ref: String },
    /// Analyze uncommitted changes
    Uncommitted { base_ref: String },
    /// Analyze a specific commit
    SingleCommit { sha: String },
    /// Analyze all commits since base
    AllCommits { base_ref: String },
}
