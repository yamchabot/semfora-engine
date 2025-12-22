//! Locals query module for variable reference tracking
//!
//! Uses tree-sitter's locals.scm query files to extract variable definitions
//! and references. This enables tracking which symbols reference module-level
//! variables, constants, and fields.
//!
//! # Query Captures
//!
//! The locals.scm files use standardized captures from nvim-treesitter:
//! - `@local.definition.var` - Variable definitions
//! - `@local.definition.function` - Function definitions
//! - `@local.reference` - All identifier references
//! - `@local.scope` - Scope boundaries
//!
//! # Usage
//!
//! ```ignore
//! use crate::detectors::locals::get_locals_query;
//! use crate::lang::Lang;
//!
//! if let Some(locals) = get_locals_query(Lang::Rust) {
//!     let refs = locals.extract_references(&root_node, source);
//!     for r in refs {
//!         println!("Reference to {} at line {}", r.name, r.line);
//!     }
//! }
//! ```

use crate::lang::Lang;
use crate::schema::RefKind;
use std::ops::Range;
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator};

/// A reference to a variable/identifier found in source code
#[derive(Debug, Clone)]
pub struct Reference {
    /// The name of the referenced identifier
    pub name: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte range in source
    pub byte_range: Range<usize>,
    /// Kind of reference (read, write, or readwrite)
    pub ref_kind: RefKind,
}

/// Loaded locals.scm query for a language
pub struct LocalsQuery {
    query: Query,
    reference_idx: Option<u32>,
    #[allow(dead_code)]
    definition_idx: Option<u32>,
    #[allow(dead_code)]
    scope_idx: Option<u32>,
}

impl LocalsQuery {
    /// Create a new LocalsQuery from a tree-sitter language and query source
    pub fn new(language: &Language, query_src: &str) -> Option<Self> {
        let query = Query::new(language, query_src).ok()?;

        // Find capture indices for standard locals captures
        let reference_idx = query.capture_index_for_name("local.reference");
        let definition_idx = query.capture_index_for_name("local.definition")
            .or_else(|| query.capture_index_for_name("local.definition.var"));
        let scope_idx = query.capture_index_for_name("local.scope");

        Some(Self {
            query,
            reference_idx,
            definition_idx,
            scope_idx,
        })
    }

    /// Extract all references from an AST
    ///
    /// Returns a list of all identifier references found in the source.
    /// The caller should filter these to only track references to known
    /// module-level Variable symbols.
    pub fn extract_references<'a>(&self, root: &Node<'a>, source: &str) -> Vec<Reference> {
        let Some(ref_idx) = self.reference_idx else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, *root, source.as_bytes());

        let mut references = Vec::new();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == ref_idx {
                    let node = capture.node;
                    let name = get_node_text(&node, source);

                    // Skip empty or overly long names
                    if name.is_empty() || name.len() > 100 {
                        continue;
                    }

                    // Skip keywords and common noise
                    if is_keyword(&name) {
                        continue;
                    }

                    // Determine if this is a read, write, or readwrite
                    let ref_kind = determine_ref_kind(&node);

                    references.push(Reference {
                        name,
                        line: node.start_position().row + 1,
                        byte_range: node.byte_range(),
                        ref_kind,
                    });
                }
            }
        }

        references
    }
}

/// Get the locals query for a language, if available
///
/// Returns None for config/markup languages that don't have variable references.
pub fn get_locals_query(lang: Lang) -> Option<LocalsQuery> {
    let query_src = match lang {
        // Programming languages with locals.scm
        Lang::Rust => include_str!("../../queries/rust/locals.scm"),
        Lang::Go => include_str!("../../queries/go/locals.scm"),
        Lang::Java => include_str!("../../queries/java/locals.scm"),
        Lang::CSharp => include_str!("../../queries/c_sharp/locals.scm"),
        Lang::Python => include_str!("../../queries/python/locals.scm"),
        Lang::JavaScript | Lang::Jsx => include_str!("../../queries/javascript/locals.scm"),
        Lang::TypeScript => include_str!("../../queries/typescript/locals.scm"),
        Lang::Tsx => include_str!("../../queries/tsx/locals.scm"),
        Lang::C => include_str!("../../queries/c/locals.scm"),
        Lang::Cpp => include_str!("../../queries/cpp/locals.scm"),
        Lang::Kotlin => include_str!("../../queries/kotlin/locals.scm"),
        Lang::Bash | Lang::Dockerfile => include_str!("../../queries/bash/locals.scm"),
        Lang::Gradle => include_str!("../../queries/groovy/locals.scm"),
        Lang::Hcl => include_str!("../../queries/hcl/locals.scm"),
        Lang::Vue => include_str!("../../queries/javascript/locals.scm"), // Vue script uses JS

        // Config/Markup - no variable references to track
        Lang::Html | Lang::Css | Lang::Scss | Lang::Markdown |
        Lang::Json | Lang::Yaml | Lang::Toml | Lang::Xml => return None,
    };

    LocalsQuery::new(&lang.tree_sitter_language(), query_src)
}

/// Extract text from a node
fn get_node_text(node: &Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

/// Determine if a reference is a read, write, or readwrite operation
///
/// Analyzes the AST context around the node to detect:
/// - Write: LHS of assignment (=)
/// - ReadWrite: Compound assignment (+=, -=, etc.) or increment/decrement
/// - Read: Everything else (default)
fn determine_ref_kind(node: &Node) -> RefKind {
    // Walk up the tree to find assignment context
    let mut current = *node;

    while let Some(parent) = current.parent() {
        let parent_kind = parent.kind();

        // Check for assignment patterns across languages
        match parent_kind {
            // Compound assignment patterns (check first - more specific)
            // Python: augmented_assignment, Rust: compound_assignment_expr
            // JS/TS: augmented_assignment_expression
            "augmented_assignment" | "augmented_assignment_expression"
            | "compound_assignment_expr" => {
                if is_on_lhs(&current, &parent) {
                    return RefKind::ReadWrite;
                }
            }

            // Simple assignment: x = value
            // Also handles Bash variable_assignment
            "assignment_expression" | "assignment" | "variable_assignment" => {
                // First check for compound operators (JS uses assignment_expression for +=)
                if has_compound_operator(&parent) && is_on_lhs(&current, &parent) {
                    return RefKind::ReadWrite;
                }
                // Check if our node is on the left side
                if is_on_lhs(&current, &parent) {
                    return RefKind::Write;
                }
                // On RHS = read (continue to return Read)
                break;
            }

            // Increment/decrement: x++ or ++x (read + write)
            "update_expression" | "unary_expression" => {
                // Check for ++ or -- operators
                for i in 0..parent.child_count() {
                    if let Some(child) = parent.child(i) {
                        let text = child.kind();
                        if text == "++" || text == "--" {
                            return RefKind::ReadWrite;
                        }
                    }
                }
            }

            // Rust-specific patterns
            "let_declaration" => {
                // let x = ... - x is being written
                if let Some(pattern) = parent.child_by_field_name("pattern") {
                    if node_contains(&pattern, node) {
                        return RefKind::Write;
                    }
                }
            }

            // Go short variable declaration (:=)
            "short_var_declaration" => {
                if is_on_lhs(&current, &parent) {
                    return RefKind::Write;
                }
            }

            // Stop at statement boundaries
            "expression_statement" | "block" | "function_definition" | "method_definition" => {
                break;
            }

            _ => {}
        }

        current = parent;
    }

    // Default: reading the variable
    RefKind::Read
}

/// Check if an assignment node has a compound operator (+=, -=, etc.)
fn has_compound_operator(node: &Node) -> bool {
    // Check for operator field
    if let Some(op_node) = node.child_by_field_name("operator") {
        let op_kind = op_node.kind();
        return matches!(
            op_kind,
            "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
        );
    }

    // Fallback: check children for compound operator kinds
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let kind = child.kind();
            if matches!(
                kind,
                "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
            ) {
                return true;
            }
        }
    }

    false
}

/// Check if a node is on the left-hand side of a parent assignment
fn is_on_lhs(node: &Node, parent: &Node) -> bool {
    // Try field-based approach first (more reliable)
    if let Some(left) = parent.child_by_field_name("left") {
        return node_contains(&left, node);
    }

    // Fallback: first child is typically the LHS
    if let Some(first) = parent.child(0) {
        // The LHS is before the operator
        return node.end_byte() <= first.end_byte() || node_contains(&first, node);
    }

    false
}

/// Check if ancestor contains descendant
fn node_contains(ancestor: &Node, descendant: &Node) -> bool {
    ancestor.start_byte() <= descendant.start_byte()
        && ancestor.end_byte() >= descendant.end_byte()
}

/// Check if a name is a language keyword (should be skipped)
fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        // Common keywords across languages
        "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "default"
            | "break" | "continue" | "return" | "throw" | "try" | "catch" | "finally"
            | "new" | "delete" | "typeof" | "instanceof" | "void" | "null" | "undefined"
            | "true" | "false" | "this" | "self" | "super" | "class" | "struct" | "enum"
            | "interface" | "trait" | "impl" | "fn" | "func" | "function" | "def" | "let"
            | "const" | "var" | "mut" | "static" | "final" | "public" | "private"
            | "protected" | "internal" | "export" | "import" | "from" | "as" | "async"
            | "await" | "yield" | "in" | "of" | "is" | "and" | "or" | "not" | "None"
            | "True" | "False" | "nil" | "package" | "module" | "use" | "require"
            | "include" | "extends" | "implements" | "with" | "where" | "when"
            | "match" | "guard" | "loop" | "foreach" | "select" | "join"
            | "type" | "alias" | "typedef" | "namespace" | "using" | "goto" | "label"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_locals_query_loads() {
        let query = get_locals_query(Lang::Rust);
        assert!(query.is_some(), "Rust locals.scm should load successfully");
    }

    #[test]
    fn test_python_locals_query_loads() {
        let query = get_locals_query(Lang::Python);
        assert!(query.is_some(), "Python locals.scm should load successfully");
    }

    #[test]
    fn test_javascript_locals_query_loads() {
        let query = get_locals_query(Lang::JavaScript);
        assert!(query.is_some(), "JavaScript locals.scm should load successfully");
    }

    #[test]
    fn test_config_languages_return_none() {
        assert!(get_locals_query(Lang::Json).is_none());
        assert!(get_locals_query(Lang::Yaml).is_none());
        assert!(get_locals_query(Lang::Toml).is_none());
    }

    #[test]
    fn test_rust_extract_references() {
        let query = get_locals_query(Lang::Rust).unwrap();
        let source = r#"
const MAX_SIZE: usize = 100;

fn process() {
    let arr = vec![0; MAX_SIZE];
    for i in 0..MAX_SIZE {
        println!("{}", i);
    }
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&Lang::Rust.tree_sitter_language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let refs = query.extract_references(&tree.root_node(), source);

        // Should find references to MAX_SIZE
        let max_size_refs: Vec<_> = refs.iter().filter(|r| r.name == "MAX_SIZE").collect();
        assert!(
            max_size_refs.len() >= 2,
            "Should find at least 2 references to MAX_SIZE, found {}",
            max_size_refs.len()
        );
    }

    #[test]
    fn test_bash_extract_references() {
        let query = get_locals_query(Lang::Bash).unwrap();
        let source = r#"
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

echo "${RED}Error${NC}"
echo "${GREEN}Success${NC}"
"#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&Lang::Bash.tree_sitter_language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let refs = query.extract_references(&tree.root_node(), source);

        // Should find references to color variables
        let red_refs: Vec<_> = refs.iter().filter(|r| r.name == "RED").collect();
        let nc_refs: Vec<_> = refs.iter().filter(|r| r.name == "NC").collect();

        assert!(!red_refs.is_empty(), "Should find references to RED");
        assert!(!nc_refs.is_empty(), "Should find references to NC");
    }
}
