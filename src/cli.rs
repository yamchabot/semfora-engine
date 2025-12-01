//! CLI argument definitions using clap

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::tokens::{format_analysis_compact, format_analysis_report, TokenAnalyzer};

/// Semantic code analyzer with TOON output
#[derive(Parser, Debug)]
#[command(name = "mcp-diff")]
#[command(about = "Deterministic semantic code analyzer that outputs TOON-formatted summaries")]
#[command(version)]
#[command(author)]
pub struct Cli {
    /// Path to file to analyze (single file mode)
    #[arg(value_name = "FILE", required_unless_present_any = ["diff", "commit", "commits"])]
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
            message: "No operation specified. Provide a FILE/DIR or use --dir/--diff/--commit/--commits".to_string(),
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
        self.diff.is_some() || self.commit.is_some() || self.commits
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
