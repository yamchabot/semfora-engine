// Clippy allows - these are style issues that can be addressed incrementally
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_map)]
#![allow(clippy::useless_format)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::single_char_add_str)]
#![allow(clippy::type_complexity)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::lines_filter_map_ok)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::unnecessary_to_owned)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::result_large_err)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::for_kv_map)]
#![allow(clippy::into_iter_on_ref)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::manual_find)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::filter_next)]
#![allow(clippy::option_map_or_none)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::let_and_return)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::manual_range_patterns)]
#![allow(clippy::manual_strip)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::char_lit_as_u8)]
#![allow(clippy::while_let_on_iterator)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::collapsible_str_replace)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::io_other_error)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::question_mark)]
#![allow(clippy::while_let_loop)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::len_zero)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::iter_kv_map)]
#![allow(clippy::manual_clamp)]
// Allow unused comparisons for >= 0 checks on usize (which are technically always true)
#![allow(unused_comparisons)]

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
pub mod bm25;
pub mod cache;
pub mod cli;
pub mod commands;
pub mod detectors;
pub mod drift;
pub mod duplicate;
pub mod error;
pub mod extract;
pub mod fs_utils;
pub mod git;
pub mod indexing;
pub mod installer;
pub mod lang;
pub mod lint_runner;
pub mod mcp_server;
pub mod module_registry;
pub mod overlay;
pub mod parsing;
pub mod paths;
pub mod ripgrep;
pub mod risk;
pub mod trace;
pub mod schema;
pub mod search;
pub mod security;
pub mod server;
pub mod shard;
pub mod socket_server;
pub mod sqlite_export;
pub mod test_runner;
pub mod tokens;
pub mod toon;
pub mod utils;

// Re-export commonly used types
pub use utils::{truncate_to_char_boundary, truncate_with_ellipsis};

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
    is_git_repo, ChangeType, ChangedFile, CommitInfo,
};

// Re-export cache module types
pub use cache::{
    get_cache_base_dir, list_cached_repos, normalize_kind, prune_old_caches, CacheDir, CacheMeta,
    IndexingStatus, LayeredIndexMeta, RipgrepSearchResult, SearchWithFallbackResult,
    SourceFileInfo, SymbolIndexEntry,
};

// Re-export shard module types
pub use shard::{compute_optimal_names_public, extract_module_name, ShardStats, ShardWriter};

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
    analyze_call_graph, analyze_module, analyze_repo,
    format_analysis_report as format_static_analysis_report, CallGraphAnalysis, ModuleMetrics,
    RepoAnalysis, SymbolComplexity,
};

// Re-export server types (SEM-98, SEM-99, SEM-101, SEM-102, SEM-104)
pub use server::{
    FileWatcher, GitPoller, LayerStatus, LayerSynchronizer, LayerUpdateStats, ServerState,
    ServerStatus,
};

// Re-export BM25 semantic search types (Phase 3)
pub use bm25::{extract_terms_from_symbol, tokenize, Bm25Document, Bm25Index, Bm25SearchResult};

// Re-export duplicate detection types
pub use duplicate::{
    boilerplate::{BoilerplateCategory, BoilerplateConfig, CustomBoilerplateRule},
    Difference, DuplicateCluster, DuplicateDetector, DuplicateKind, DuplicateMatch,
    FunctionSignature, SymbolRef,
};

// Re-export SQLite export types (call graph visualization)
pub use sqlite_export::{
    default_export_path, ExportPhase, ExportProgress, ExportStats, ProgressCallback, SqliteExporter,
};

// Re-export security types (CVE pattern detection)
pub use security::{
    CVEMatch, CVEPattern, CVEScanSummary, PatternDatabase, PatternSource, Severity,
};

// Re-export filesystem utilities (Windows compatibility)
pub use fs_utils::{atomic_rename, normalize_path};

// Re-export path resolution utilities (CLI/MCP unification)
pub use paths::{canonicalize_path, ensure_directory, resolve_path, resolve_path_or_cwd, resolve_pathbuf};

// Re-export indexing utilities (CLI/MCP unification - DEDUP-102)
pub use indexing::{
    analyze_files_parallel, collect_files, collect_files_recursive, should_skip_path,
    IndexGenerationResult, IndexingProgressCallback,
};

// Re-export parsing utilities (CLI/MCP unification - DEDUP-103)
pub use parsing::{parse_and_extract, parse_and_extract_with_options};

// Test change to trigger semfora-ci workflow
