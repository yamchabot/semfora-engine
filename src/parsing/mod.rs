//! Unified parsing module for semantic code extraction.
//!
//! This module provides the core parsing functionality used by both CLI commands
//! and MCP tools. It consolidates the `parse_and_extract` logic that was previously
//! duplicated across multiple files.
//!
//! # Example
//!
//! ```ignore
//! use semfora_engine::parsing::parse_and_extract;
//! use semfora_engine::Lang;
//! use std::path::Path;
//!
//! let source = "function hello() { return 'world'; }";
//! let path = Path::new("hello.ts");
//! let lang = Lang::TypeScript;
//!
//! let summary = parse_and_extract(path, source, lang)?;
//! ```

use std::path::Path;

use crate::error::McpDiffError;
use crate::extract::extract;
use crate::lang::Lang;
use crate::SemanticSummary;

/// Parse source code and extract semantic summary.
///
/// This is the core parsing function used throughout the codebase. It:
/// 1. Creates a tree-sitter parser for the given language
/// 2. Parses the source code into an AST
/// 3. Extracts semantic information (functions, classes, etc.)
///
/// # Arguments
///
/// * `file_path` - Path to the file (used for error messages and summary metadata)
/// * `source` - The source code to parse
/// * `lang` - The programming language
///
/// # Errors
///
/// Returns `McpDiffError::ParseFailure` if:
/// - The language cannot be set on the parser
/// - The source code cannot be parsed
/// - Semantic extraction fails
pub fn parse_and_extract(
    file_path: &Path,
    source: &str,
    lang: Lang,
) -> Result<SemanticSummary, McpDiffError> {
    parse_and_extract_with_options(file_path, source, lang, false)
}

/// Parse source code and extract semantic summary with debug options.
///
/// This is the extended version that supports debugging features like AST printing.
///
/// # Arguments
///
/// * `file_path` - Path to the file
/// * `source` - The source code to parse
/// * `lang` - The programming language
/// * `print_ast` - If true, prints the AST to stderr for debugging
///
/// # Errors
///
/// Same as [`parse_and_extract`].
pub fn parse_and_extract_with_options(
    file_path: &Path,
    source: &str,
    lang: Lang,
    print_ast: bool,
) -> Result<SemanticSummary, McpDiffError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!(
                "Failed to set language for {}: {:?}",
                file_path.display(),
                e
            ),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: format!("Failed to parse file: {}", file_path.display()),
        })?;

    if print_ast {
        eprintln!("=== AST for {} ===", file_path.display());
        eprintln!("{}", tree.root_node().to_sexp());
        eprintln!("=================");
    }

    extract(file_path, source, &tree, lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_typescript() {
        let source = "export function hello(): string { return 'world'; }";
        let path = Path::new("test.ts");
        let lang = Lang::TypeScript;

        let result = parse_and_extract(path, source, lang);
        assert!(result.is_ok());

        let summary = result.unwrap();
        // SemanticSummary uses 'symbol' for primary symbol name
        // and 'symbols' Vec for all symbols in the file
        assert!(summary.symbol.is_some() || !summary.symbols.is_empty());
    }

    #[test]
    fn test_parse_rust() {
        let source = "pub fn greet() -> &'static str { \"hello\" }";
        let path = Path::new("test.rs");
        let lang = Lang::Rust;

        let result = parse_and_extract(path, source, lang);
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert!(summary.symbol.is_some() || !summary.symbols.is_empty());
    }

    #[test]
    fn test_parse_python() {
        let source = "def say_hello():\n    return 'hello'";
        let path = Path::new("test.py");
        let lang = Lang::Python;

        let result = parse_and_extract(path, source, lang);
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert!(summary.symbol.is_some() || !summary.symbols.is_empty());
    }

    #[test]
    fn test_parse_invalid_syntax() {
        // Tree-sitter is lenient with syntax errors, but we can test the flow
        let source = "function { invalid syntax";
        let path = Path::new("test.ts");
        let lang = Lang::TypeScript;

        // Tree-sitter will parse this with error nodes, but won't fail completely
        let result = parse_and_extract(path, source, lang);
        // The result may be Ok with empty symbols or an error, depending on severity
        // The important thing is it doesn't panic
        let _ = result;
    }
}
