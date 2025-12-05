//! CLI argument definitions using clap

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::tokens::{format_analysis_compact, format_analysis_report, TokenAnalyzer};

/// Semantic code analyzer with TOON output
#[derive(Parser, Debug)]
#[command(name = "semfora-mcp")]
#[command(about = "Deterministic semantic code analyzer that outputs TOON-formatted summaries")]
#[command(version)]
#[command(author)]
pub struct Cli {
    /// Path to file to analyze (single file mode)
    #[arg(value_name = "FILE", required_unless_present_any = ["diff", "commit", "commits", "uncommitted", "cache_info", "cache_clear", "cache_prune", "dir", "benchmark", "list_modules", "get_module", "search_symbols", "list_symbols", "get_symbol", "get_overview"])]
    pub file: Option<PathBuf>,

    /// Output format
    #[arg(short, long, default_value = "toon", value_enum)]
    pub format: OutputFormat,

    /// Show verbose output including AST info
    #[arg(short, long)]
    pub verbose: bool,

    /// Print the parsed AST (for debugging)
    #[arg(long)]
    pub print_ast: bool,

    /// Analyze AI token counts (pre/post TOON conversion)
    #[arg(long, value_enum)]
    pub analyze_tokens: Option<TokenAnalysisMode>,

    /// Include compact JSON in token analysis comparison
    #[arg(long, requires = "analyze_tokens")]
    pub compare_compact: bool,

    // ============================================
    // Git Diff Options
    // ============================================

    /// Analyze git diff against base branch (auto-detects main/master)
    /// Optional: specify a ref to diff against (e.g., --diff develop)
    #[arg(long, value_name = "REF", num_args = 0..=1, default_missing_value = "auto")]
    pub diff: Option<String>,

    /// Analyze uncommitted changes (working directory vs HEAD)
    /// This includes both staged and unstaged changes
    #[arg(long)]
    pub uncommitted: bool,

    /// Base branch for diff comparison (default: auto-detect main/master)
    #[arg(long, value_name = "BRANCH")]
    pub base: Option<String>,

    /// Analyze a specific commit
    #[arg(long, value_name = "SHA")]
    pub commit: Option<String>,

    /// Analyze all commits on current branch since base
    #[arg(long)]
    pub commits: bool,

    /// Only process files with specific extensions (can be repeated)
    #[arg(long = "ext", value_name = "EXT")]
    pub extensions: Vec<String>,

    /// Show summary statistics only (no per-file details)
    #[arg(long)]
    pub summary_only: bool,

    /// Analyze all files in a directory (recursive)
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Maximum directory depth for recursive scan (default: 10)
    #[arg(long, default_value = "10")]
    pub max_depth: usize,

    /// Include test files in analysis (excluded by default)
    ///
    /// By default, files matching test patterns are excluded:
    /// - Rust: *_test.rs, tests/**
    /// - TypeScript/JS: *.test.ts, *.spec.ts, __tests__/**
    /// - Python: test_*.py, *_test.py, tests/**
    /// - Go: *_test.go
    /// - Java: *Test.java, *Tests.java
    #[arg(long)]
    pub allow_tests: bool,

    // ============================================
    // Sharded Output Options
    // ============================================

    /// Generate sharded output for large repos (writes to cache directory)
    #[arg(long)]
    pub shard: bool,

    /// Show cache information
    #[arg(long)]
    pub cache_info: bool,

    /// Clear the cache for the current directory
    #[arg(long)]
    pub cache_clear: bool,

    /// Prune caches older than N days
    #[arg(long, value_name = "DAYS")]
    pub cache_prune: Option<u32>,

    // ============================================
    // Shard Query Options (Query-Driven API)
    // ============================================

    /// List all modules in the cached index
    #[arg(long)]
    pub list_modules: bool,

    /// Get a specific module's content from the cache
    #[arg(long, value_name = "MODULE")]
    pub get_module: Option<String>,

    /// Search for symbols by name in the cached index
    #[arg(long, value_name = "QUERY")]
    pub search_symbols: Option<String>,

    /// List all symbols in a module from the cached index
    #[arg(long, value_name = "MODULE")]
    pub list_symbols: Option<String>,

    /// Get a specific symbol's details by hash
    #[arg(long, value_name = "HASH")]
    pub get_symbol: Option<String>,

    /// Get the repository overview from the cache
    #[arg(long)]
    pub get_overview: bool,

    /// Filter results by symbol kind (fn, struct, component, etc.)
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,

    /// Filter results by risk level (high, medium, low)
    #[arg(long, value_name = "RISK")]
    pub risk: Option<String>,

    /// Limit number of results (default: 50)
    #[arg(long, default_value = "50")]
    pub limit: usize,

    // ============================================
    // Benchmark Options
    // ============================================

    /// Run token efficiency benchmark comparing semantic vs raw file reads
    #[arg(long)]
    pub benchmark: bool,
}

/// Token analysis output mode
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TokenAnalysisMode {
    /// Full detailed report with breakdown
    Full,
    /// Compact single-line summary
    Compact,
}

/// Output format options
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    /// TOON (Token-Oriented Object Notation) - default, token-efficient format
    #[default]
    Toon,
    /// JSON - standard JSON output
    Json,
}

/// Operation mode determined from CLI arguments
#[derive(Debug, Clone)]
pub enum OperationMode {
    /// Analyze a single file
    SingleFile(PathBuf),
    /// Analyze all files in a directory
    Directory {
        path: PathBuf,
        max_depth: usize,
    },
    /// Diff current branch against base
    DiffBranch {
        base_ref: String,
    },
    /// Analyze uncommitted changes (working directory vs base_ref)
    Uncommitted {
        base_ref: String,
    },
    /// Analyze a specific commit
    SingleCommit {
        sha: String,
    },
    /// Analyze all commits since base
    AllCommits {
        base_ref: String,
    },
}

impl Cli {
    /// Parse CLI arguments from command line
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Determine the operation mode from CLI arguments
    pub fn operation_mode(&self) -> crate::error::Result<OperationMode> {
        // Explicit directory mode
        if let Some(ref dir) = self.dir {
            return Ok(OperationMode::Directory {
                path: dir.clone(),
                max_depth: self.max_depth,
            });
        }

        // Single file mode - but check if it's actually a directory
        if let Some(ref file) = self.file {
            if file.is_dir() {
                return Ok(OperationMode::Directory {
                    path: file.clone(),
                    max_depth: self.max_depth,
                });
            }
            return Ok(OperationMode::SingleFile(file.clone()));
        }

        // Uncommitted changes mode (working directory vs HEAD)
        if self.uncommitted {
            let base_ref = self.base.clone().unwrap_or_else(|| "HEAD".to_string());
            return Ok(OperationMode::Uncommitted { base_ref });
        }

        // Git diff mode
        if self.diff.is_some() || self.commits {
            let base_ref = self.resolve_base_ref()?;

            if self.commits {
                return Ok(OperationMode::AllCommits { base_ref });
            }

            return Ok(OperationMode::DiffBranch { base_ref });
        }

        // Single commit mode
        if let Some(ref sha) = self.commit {
            return Ok(OperationMode::SingleCommit { sha: sha.clone() });
        }

        Err(crate::error::McpDiffError::GitError {
            message: "No operation specified. Provide a FILE/DIR or use --dir/--diff/--commit/--commits/--uncommitted".to_string(),
        })
    }

    /// Resolve the base ref for diff operations
    fn resolve_base_ref(&self) -> crate::error::Result<String> {
        // Explicit --base takes priority
        if let Some(ref base) = self.base {
            return Ok(base.clone());
        }

        // Check if --diff provided explicit ref
        if let Some(ref diff_ref) = self.diff {
            if diff_ref != "auto" {
                return Ok(diff_ref.clone());
            }
        }

        // Auto-detect base branch
        crate::git::detect_base_branch(None)
    }

    /// Check if we're in git mode
    pub fn is_git_mode(&self) -> bool {
        self.diff.is_some() || self.commit.is_some() || self.commits || self.uncommitted
    }

    /// Check if a file extension should be processed
    pub fn should_process_extension(&self, ext: &str) -> bool {
        if self.extensions.is_empty() {
            return true;
        }
        self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Run token analysis on the given source, JSON (pretty & compact), and TOON outputs
    pub fn run_token_analysis(
        &self,
        source: &str,
        json_pretty: &str,
        json_compact: &str,
        toon: &str,
    ) -> Option<String> {
        self.analyze_tokens.map(|mode| {
            let analyzer = TokenAnalyzer::new();
            let analysis = analyzer.analyze(source, json_pretty, json_compact, toon);

            match mode {
                TokenAnalysisMode::Full => format_analysis_report(&analysis, self.compare_compact),
                TokenAnalysisMode::Compact => {
                    format_analysis_compact(&analysis, self.compare_compact)
                }
            }
        })
    }
}
