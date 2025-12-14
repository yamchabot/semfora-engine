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
//! - `csharp`: C# (.NET)
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
//!
//! # Language Roadmap
//!
//! ## IMPLEMENTED: C# (.NET)
//! - Parser: tree-sitter-c-sharp (mature)
//! - Targets: classes, records, structs, interfaces, enums
//! - Control flow: if, switch, await, try/catch, pattern matching
//! - Modifiers: async, unsafe, partial, static
//! - Frameworks: ASP.NET Core, Entity Framework, Unity (via boilerplate detection)
//!
//! ## Priority 1: Kotlin - HIGH PRIORITY
//! TODO(SEM-XX): Enhance Kotlin detector (kotlin.rs)
//! - Add data class and copy semantics
//! - Add coroutines and suspend function tracking
//! - Add extension function detection (important for call graphs)
//! - Add sealed class and exhaustive when support
//! - Frameworks: Spring Boot, Ktor, Android ViewModel
//!
//! ## Priority 3: Swift - MEDIUM PRIORITY
//! TODO(SEM-XX): Add Swift detector (swift.rs)
//! - Parser: tree-sitter-swift
//! - Struct vs class semantics
//! - Protocol conformance tracking
//! - Property wrappers (@State, @Binding, etc.)
//! - SwiftUI View boilerplate
//! - Async/await task trees
//!
//! ## Priority 4: PHP - MEDIUM PRIORITY
//! TODO(SEM-XX): Add PHP detector (php.rs)
//! - Parser: tree-sitter-php
//! - Focus: Framework-aware semantics
//! - Frameworks: Laravel (controllers, providers, middleware, Eloquent)
//! - High ROI due to extreme boilerplate density
//!
//! ## Priority 5: Ruby - LOW PRIORITY
//! TODO(SEM-XX): Add Ruby detector (ruby.rs)
//! - Parser: tree-sitter-ruby
//! - Focus: Rails patterns
//! - ActiveRecord models, controllers, RSpec scaffolding
//!
//! ## Priority 6: Scala - OPTIONAL
//! TODO(SEM-XX): Add Scala detector (scala.rs) - only if enterprise demand
//! - Complex AST, powerful type system, FP semantics
//! - Requires more extractor sophistication
//!
//! ## Infra Languages (Parser-only, structural)
//! TODO(SEM-XX): Enhance shell.rs for PowerShell (.ps1)
//! TODO(SEM-XX): Add Dockerfile detector (structural patterns)
//! TODO(SEM-XX): Add Makefile detector (structural patterns)

pub mod c_family;
pub mod common;
pub mod config;
pub mod csharp;
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
