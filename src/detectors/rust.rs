//! Rust language detector
//!
//! Extracts semantic information from Rust source files using the generic extractor.
//! Rust's struct/enum/trait declarations are first-class AST nodes, so the generic
//! extractor handles them well.

use tree_sitter::Tree;

use crate::detectors::generic::extract_with_grammar;
use crate::detectors::grammar::RUST_GRAMMAR;
use crate::error::Result;
use crate::schema::SemanticSummary;

/// Extract semantic information from a Rust source file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    // The generic extractor handles everything for Rust:
    // - Symbols: function_item, struct_item, enum_item, trait_item
    // - Imports: use_declaration
    // - State changes: let_declaration, assignment_expression
    // - Control flow: if, for, while, match, loop
    // - Calls: call_expression
    // - Risk calculation
    extract_with_grammar(summary, source, tree, &RUST_GRAMMAR)
}
