//! CLI command integration tests
//!
//! This module contains comprehensive tests for all CLI commands,
//! verifying correct behavior across all subcommands, arguments,
//! and output formats (text, toon, json).

pub mod analyze_tests;
pub mod cache_tests;
pub mod commit_tests;
pub mod index_tests;
pub mod query_tests;
pub mod search_tests;
// Security tests disabled - command hidden from CLI (kept in src/commands/security.rs for future use)
// pub mod security_tests;
pub mod setup_tests;
pub mod test_tests;
pub mod validate_tests;
