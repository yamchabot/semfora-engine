//! Go language detector
//!
//! Extracts semantic information from Go source files including:
//! - Primary symbol detection with improved heuristics
//! - Import statements (dependencies)
//! - Type declarations and functions
//! - State changes (variable declarations)
//! - Control flow (if, for, switch, select)
//! - Function calls

use tree_sitter::{Node, Tree};

use crate::detectors::common::get_node_text;
use crate::detectors::generic::extract_with_grammar;
use crate::detectors::grammar::GO_GRAMMAR;
use crate::error::Result;
use crate::schema::{FrameworkEntryPoint, RiskLevel, SemanticSummary, SymbolInfo, SymbolKind};

/// Extract semantic information from a Go source file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    // Use the generic extractor for most semantic extraction
    // This handles: imports, state_changes, control_flow, calls, and risk calculation
    extract_with_grammar(summary, source, tree, &GO_GRAMMAR)?;

    // Go has a unique type declaration structure: type_declaration > type_spec > (struct_type | interface_type)
    // The generic extractor won't find these, so we do Go-specific symbol extraction
    // that merges with what the generic extractor already found
    let root = tree.root_node();
    find_go_type_symbols(summary, &root, source);

    Ok(())
}

// ============================================================================
// Go-Specific Type Declaration Handling
// ============================================================================
// Go's type declarations have a unique structure: type_declaration > type_spec > (struct_type | interface_type)
// The generic extractor handles functions/methods, but not these nested type declarations.

/// Candidate symbol for ranking
#[derive(Debug)]
struct SymbolCandidate {
    name: String,
    kind: SymbolKind,
    is_exported: bool,
    start_line: usize,
    end_line: usize,
    score: i32,
}

/// Find Go-specific type symbols (structs and interfaces)
///
/// This function extracts type declarations that the generic extractor can't handle
/// due to Go's unique AST structure where types are nested inside type_declaration nodes.
///
/// Go convention: exported names start with uppercase
fn find_go_type_symbols(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut candidates: Vec<SymbolCandidate> = Vec::new();
    let filename_stem = extract_filename_stem(&summary.file);

    // Only collect type declarations (structs, interfaces)
    collect_type_candidates(root, source, &filename_stem, &mut candidates);

    // Sort by score (highest first)
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Add type candidates to summary.symbols (functions/methods already added by generic extractor)
    for candidate in &candidates {
        if candidate.is_exported || candidate.score > 0 {
            // Check if this symbol already exists (to avoid duplicates)
            let already_exists = summary.symbols.iter().any(|s| s.name == candidate.name);
            if already_exists {
                continue;
            }

            let symbol_info = SymbolInfo {
                name: candidate.name.clone(),
                kind: candidate.kind,
                start_line: candidate.start_line,
                end_line: candidate.end_line,
                is_exported: candidate.is_exported,
                is_default_export: false,
                hash: None,
                arguments: Vec::new(),
                props: Vec::new(),
                return_type: None,
                calls: Vec::new(),
                control_flow: Vec::new(),
                state_changes: Vec::new(),
                behavioral_risk: RiskLevel::Low,
                decorators: Vec::new(),
                framework_entry_point: FrameworkEntryPoint::None,
            };
            summary.symbols.push(symbol_info);
        }
    }

    // If we found type candidates with higher scores than current primary, update it
    if let Some(best_type) = candidates.first() {
        let current_primary_score = summary
            .symbol
            .as_ref()
            .map(|name| {
                calculate_symbol_score(
                    name,
                    &summary.symbol_kind.unwrap_or(SymbolKind::Function),
                    summary.public_surface_changed,
                    &filename_stem,
                )
            })
            .unwrap_or(0);

        if best_type.score > current_primary_score {
            summary.symbol = Some(best_type.name.clone());
            summary.symbol_kind = Some(best_type.kind);
            summary.start_line = Some(best_type.start_line);
            summary.end_line = Some(best_type.end_line);
            summary.public_surface_changed = best_type.is_exported;
        }
    }
}

/// Extract the filename stem from a file path
fn extract_filename_stem(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Collect only type declarations (structs, interfaces) from the AST
/// Functions and methods are handled by the generic extractor
fn collect_type_candidates(
    root: &Node,
    source: &str,
    filename_stem: &str,
    candidates: &mut Vec<SymbolCandidate>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "type_declaration" {
            // Look for struct or interface type specs inside the type_declaration
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "type_spec" {
                    if let Some(name_node) = inner.child_by_field_name("name") {
                        let name = get_node_text(&name_node, source);
                        let is_exported = name
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false);

                        // Determine if it's a struct or interface
                        let kind = determine_type_kind(&inner);
                        let score =
                            calculate_symbol_score(&name, &kind, is_exported, filename_stem);

                        candidates.push(SymbolCandidate {
                            name,
                            kind,
                            is_exported,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            score,
                        });
                    }
                }
            }
        }
    }
}

/// Determine if a type_spec is a struct, interface, or other type
fn determine_type_kind(type_spec: &Node) -> SymbolKind {
    if let Some(type_node) = type_spec.child_by_field_name("type") {
        match type_node.kind() {
            "struct_type" => return SymbolKind::Struct,
            "interface_type" => return SymbolKind::Trait, // Use Trait for interfaces
            _ => {}
        }
    }
    SymbolKind::Struct // Default to struct for type aliases
}

/// Calculate a score for symbol prioritization
fn calculate_symbol_score(
    name: &str,
    kind: &SymbolKind,
    is_exported: bool,
    filename_stem: &str,
) -> i32 {
    let mut score = 0;

    // Base score by kind (types preferred over functions)
    score += match kind {
        SymbolKind::Struct => 30,
        SymbolKind::Trait => 28, // interface
        SymbolKind::Method => 15,
        SymbolKind::Function => 10,
        _ => 5,
    };

    // Bonus for exported (uppercase start)
    if is_exported {
        score += 50;
    }

    // Bonus for filename match
    let name_lower = name.to_lowercase();
    if name_lower == filename_stem {
        // Exact match
        score += 40;
    } else if name_lower.contains(filename_stem) || filename_stem.contains(&name_lower) {
        // Partial match
        score += 20;
    }

    // Bonus for main function in main.go
    if name == "main" && filename_stem == "main" {
        score += 30;
    }

    // Penalty for test functions
    if name.starts_with("Test") || name.starts_with("Benchmark") {
        score -= 30;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_stem() {
        assert_eq!(extract_filename_stem("/path/to/server.go"), "server");
        assert_eq!(extract_filename_stem("main.go"), "main");
        assert_eq!(extract_filename_stem("handler_test.go"), "handler_test");
    }

    #[test]
    fn test_calculate_symbol_score() {
        // Exported struct should beat unexported function
        let exported_struct = calculate_symbol_score("Server", &SymbolKind::Struct, true, "server");
        let unexported_func =
            calculate_symbol_score("helper", &SymbolKind::Function, false, "server");
        assert!(exported_struct > unexported_func);

        // main function in main.go gets bonus
        let main_fn = calculate_symbol_score("main", &SymbolKind::Function, false, "main");
        let other_fn = calculate_symbol_score("helper", &SymbolKind::Function, false, "main");
        assert!(main_fn > other_fn);

        // Test functions should be penalized
        let test_fn = calculate_symbol_score("TestServer", &SymbolKind::Function, true, "server");
        let normal_fn =
            calculate_symbol_score("CreateServer", &SymbolKind::Function, true, "server");
        assert!(normal_fn > test_fn);
    }
}
