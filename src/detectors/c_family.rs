//! C/C++ language detector
//!
//! Extracts semantic information from C and C++ source files using the generic extractor.
//! Selects C_GRAMMAR or CPP_GRAMMAR based on file extension.

use tree_sitter::Tree;

use crate::detectors::generic::extract_with_grammar;
use crate::detectors::grammar::{CPP_GRAMMAR, C_GRAMMAR};
use crate::error::Result;
use crate::schema::SemanticSummary;

/// Extract semantic information from a C/C++ source file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    // Select grammar based on file extension
    let is_cpp = summary.file.ends_with(".cpp")
        || summary.file.ends_with(".cc")
        || summary.file.ends_with(".cxx")
        || summary.file.ends_with(".hpp")
        || summary.file.ends_with(".hxx")
        || summary.file.ends_with(".hh");

    let grammar = if is_cpp { &CPP_GRAMMAR } else { &C_GRAMMAR };

    // Mark header files as having public surface
    let is_header = summary.file.ends_with(".h")
        || summary.file.ends_with(".hpp")
        || summary.file.ends_with(".hxx")
        || summary.file.ends_with(".hh");

    if is_header {
        summary.public_surface_changed = true;
    }

    // The generic extractor handles everything for C/C++:
    // - Symbols: function_definition, struct_specifier, class_specifier, enum_specifier
    // - Imports: preproc_include
    // - State changes: declaration, assignment_expression
    // - Control flow: if, for, while, do, switch (+ for_range_loop, try in C++)
    // - Calls: call_expression
    // - Risk calculation
    extract_with_grammar(summary, source, tree, grammar)
}
