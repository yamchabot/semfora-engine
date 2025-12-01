//! MCP-Diff: Semantic code analyzer with TOON output
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
//! use mcp_diff::{extract, Lang, encode_toon};
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

pub mod cli;
pub mod detectors;
pub mod error;
pub mod extract;
pub mod git;
pub mod lang;
pub mod risk;
pub mod schema;
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
    Prop, RiskLevel, SemanticSummary, StateChange, SymbolKind,
};
// Note: Call is included above for function call tracking
pub use tokens::{format_analysis_compact, format_analysis_report, TokenAnalysis, TokenAnalyzer};
pub use toon::encode_toon;

// Re-export git module types
pub use git::{
    detect_base_branch, get_changed_files, get_commit_changed_files, get_commits_since,
    get_current_branch, get_file_at_ref, get_merge_base, get_parent_commit, get_repo_root,
    is_git_repo, ChangedFile, ChangeType, CommitInfo,
};
