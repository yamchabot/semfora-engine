//! Command modules for the semfora-engine CLI
//!
//! This module contains all subcommand implementations organized by functionality.
//!
//! ## Architecture
//!
//! Each command module implements a single top-level command:
//! - `analyze` - File/directory/diff analysis
//! - `search` - Hybrid symbol + semantic search (the "magic" search)
//! - `query` - Query the semantic index (symbols, source, callers, callgraph)
//! - `validate` - Quality audits (complexity, duplicates)
//! - `index` - Manage the semantic index
//! - `cache` - Manage the cache
//! - `security` - CVE scanning and pattern management
//! - `test` - Run or detect tests
//! - `commit` - Prepare commit information
//!
//! All command handlers take their respective `Args` struct from `cli.rs`
//! and a shared `CommandContext` for output format and verbosity.

pub mod analyze;
pub mod cache;
pub mod commit;
pub mod index;
pub mod query;
pub mod search;
pub mod security;
pub mod serve;
pub mod test;
pub mod toon_parser;
pub mod validate;

// Re-export command handlers for easy access
pub use analyze::run_analyze;
pub use cache::run_cache;
pub use commit::run_commit;
pub use index::run_index;
pub use query::run_query;
pub use search::run_search;
pub use security::run_security;
pub use serve::run_serve;
pub use test::run_test;
pub use validate::run_validate;

use crate::cli::OutputFormat;

/// Shared context passed to all command handlers
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Output format (text, toon, or json)
    pub format: OutputFormat,
    /// Show verbose output
    pub verbose: bool,
    /// Show progress during long operations
    pub progress: bool,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self {
            format: OutputFormat::Text,
            verbose: false,
            progress: false,
        }
    }
}

/// Encode a JSON value as proper TOON using the rtoon library
pub fn encode_toon(value: &serde_json::Value) -> String {
    rtoon::encode_default(value).unwrap_or_else(|e| format!("TOON encoding error: {}", e))
}

impl CommandContext {
    /// Create a new CommandContext from CLI args
    pub fn from_cli(format: OutputFormat, verbose: bool, progress: bool) -> Self {
        Self {
            format,
            verbose,
            progress,
        }
    }
}
