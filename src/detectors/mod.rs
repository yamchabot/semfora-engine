//! Language-specific semantic detectors
//!
//! This module contains specialized extractors for different language families.
//! Each detector knows how to extract semantic information from AST nodes
//! for its target language(s).
//!
//! # Architecture
//!
//! The detector system has two layers:
//!
//! 1. **Generic extractor** (`generic.rs`): Language-agnostic extraction logic
//!    that works with any tree-sitter grammar using `LangGrammar` definitions.
//!
//! 2. **Grammar definitions** (`grammar.rs`): Per-language AST node mappings
//!    and visibility rules.
//!
//! 3. **Legacy detectors**: Language-specific extractors that can override or
//!    extend the generic behavior for special cases (JSX, React, etc.).
//!
//! # Supported Languages
//!
//! - `javascript`: JS, TS, JSX, TSX (with React/component detection)
//! - `rust`: Rust
//! - `python`: Python (with decorator detection)
//! - `go`: Go
//! - `java`: Java
//! - `c_family`: C, C++
//! - `markup`: HTML, CSS, Markdown
//! - `config`: JSON, YAML, TOML
//!
//! # Symbol Selection Heuristics
//!
//! All detectors implement improved symbol selection:
//! - Prioritize public/exported symbols over private helpers
//! - Prefer types (structs/classes/enums) over functions where applicable
//! - Consider filename matching (e.g., `toon.rs` → prefer `Toon` or `encode_toon`)
//! - For multi-symbol files, select the most semantically significant symbol
//!
//! # Adding a New Language
//!
//! 1. Add tree-sitter grammar to `Cargo.toml`
//! 2. Add `Lang` variant in `lang.rs`
//! 3. Add `LangGrammar` in `grammar.rs` with AST node mappings
//! 4. (Optional) Add language-specific detector for special features

pub mod c_family;
pub mod common;
pub mod config;
pub mod generic;
pub mod go;
pub mod gradle;
pub mod grammar;
pub mod hcl;
pub mod java;
// JavaScript is now a directory module with framework support in:
//   - javascript/core.rs: Generic JS/TS extraction
//   - javascript/frameworks/: React, Next.js, Express, Angular, Vue
pub mod javascript;
pub mod kotlin;
pub mod markup;
pub mod python;
pub mod rust;
pub mod shell;

// Re-export key types for convenience
pub use generic::extract_with_grammar;
pub use grammar::{get_grammar, LangGrammar};
