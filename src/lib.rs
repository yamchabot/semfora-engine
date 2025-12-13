//! Semfora Engine: Semantic code analyzer with TOON output
//!
//! This library provides deterministic semantic analysis of source code files
//! across multiple programming languages. It uses tree-sitter for parsing and
//! outputs summaries in TOON (Token-Oriented Object Notation) format.
//!
//! # Supported Languages
//!
//! - TypeScript, TSX, JavaScript, JSX
//! - Rust
//! - Python
//! - Go
//! - Java
//! - C, C++
//! - HTML, CSS, Markdown
//! - JSON, YAML, TOML
//!
//! # Example
//!
//! ```ignore
//! use semfora_engine::{extract, Lang, encode_toon};
//! use std::path::Path;
//!
//! let source = r#"
//! export function hello() {
//!     return "Hello, World!";
//! }
//! "#;
//!
//! let path = Path::new("hello.ts");
//! let lang = Lang::from_path(path)?;
//!
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(&lang.tree_sitter_language())?;
//! let tree = parser.parse(source, None).unwrap();
//!
//! let summary = extract(path, source, &tree, lang)?;
//! let toon = encode_toon(&summary);
//! println!("{}", toon);
//! ```

pub mod analysis;
pub mod benchmark;
pub mod benchmark_builder;
pub mod cache;
pub mod cli;
pub mod detectors;
pub mod drift;
pub mod duplicate;
pub mod error;
pub mod extract;
pub mod git;
pub mod lang;
pub mod mcp_server;
pub mod overlay;
pub mod ripgrep;
pub mod risk;
pub mod schema;
pub mod search;
pub mod server;
pub mod shard;
pub mod socket_server;
pub mod test_runner;
pub mod tokens;
pub mod toon;

// Re-export commonly used types
pub use cli::{Cli, OperationMode, OutputFormat};
pub use error::{McpDiffError, Result};
pub use extract::extract;
pub use lang::{Lang, LangFamily};
pub use risk::calculate_risk;
pub use schema::{
    Argument, Call, ControlFlowChange, ControlFlowKind, Import, ImportedName, JsxElement, Location,
    ModuleGroup, Prop, RepoOverview, RepoStats, RiskLevel, SemanticDiff, SemanticSummary,
    StateChange, SurfaceDelta, SymbolId, SymbolKind, SCHEMA_VERSION,
};
// Note: Call is included above for function call tracking
pub use tokens::{format_analysis_compact, format_analysis_report, TokenAnalysis, TokenAnalyzer};
pub use toon::{encode_toon, encode_toon_clean, encode_toon_directory, generate_repo_overview};

// Re-export git module types
pub use git::{
    detect_base_branch, get_changed_files, get_commit_changed_files, get_commits_since,
    get_current_branch, get_file_at_ref, get_merge_base, get_parent_commit, get_repo_root,
    is_git_repo, ChangedFile, ChangeType, CommitInfo,
};

// Re-export cache module types
pub use cache::{
    get_cache_base_dir, list_cached_repos, prune_old_caches, CacheDir, CacheMeta,
    IndexingStatus, LayeredIndexMeta, RipgrepSearchResult, SearchWithFallbackResult,
    SourceFileInfo, SymbolIndexEntry,
};

// Re-export shard module types
pub use shard::{ShardStats, ShardWriter};

// Re-export benchmark types
pub use benchmark::{
    analyze_repo_tokens, estimate_tokens, RawFileRead, RepoTokenMetrics, SemanticQuery,
    TaskBenchmark, TokenMetrics,
};

// Re-export overlay types (Phase 2.5 - SEM-44)
pub use overlay::{
    compute_content_hash, compute_symbol_hash, FileMove, LayerKind, LayerMeta, LayeredIndex,
    LayeredIndexStats, Overlay, SymbolState,
};

// Re-export layered query types (Phase 2.5 - SEM-53)
pub use overlay::{LayeredSearchOptions, LayeredSearchResult};

// Re-export search types
pub use search::{is_test_file, lang_from_extension, SearchHints};

// Re-export ripgrep types (Phase 2.5 - SEM-46)
pub use ripgrep::{BlockLine, MergedBlock, RipgrepSearcher, SearchMatch, SearchOptions};

// Re-export drift detection types (Phase 2.5 - SEM-47)
pub use drift::{count_tracked_files, DriftDetector, DriftStatus, UpdateStrategy};

// Re-export test runner types (North Star - multi-language test harness)
pub use test_runner::{
    detect_all_frameworks, detect_framework, run_tests, run_tests_with_framework, TestFailure,
    TestFramework, TestResults, TestRunOptions,
};

// Re-export static analysis types
pub use analysis::{
    analyze_call_graph, format_analysis_report as format_static_analysis_report,
    analyze_module, analyze_repo, CallGraphAnalysis, ModuleMetrics, RepoAnalysis,
    SymbolComplexity,
};

// Re-export server types (SEM-98, SEM-99, SEM-101, SEM-102, SEM-104)
pub use server::{
    FileWatcher, GitPoller, LayerStatus, LayerSynchronizer, LayerUpdateStats,
    ServerState, ServerStatus,
};

// Re-export duplicate detection types
pub use duplicate::{
    boilerplate::{BoilerplateCategory, BoilerplateConfig, CustomBoilerplateRule},
    Difference, DuplicateCluster, DuplicateDetector, DuplicateKind, DuplicateMatch,
    FunctionSignature, SymbolRef,
};
