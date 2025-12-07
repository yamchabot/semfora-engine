//! C/C++ language detector

use tree_sitter::{Node, Tree};
use crate::detectors::common::get_node_text;
use crate::error::Result;
use crate::schema::{RiskLevel, SemanticSummary, SymbolInfo, SymbolKind};

pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();
    let is_header = summary.file.ends_with(".h")
        || summary.file.ends_with(".hpp")
        || summary.file.ends_with(".hxx")
        || summary.file.ends_with(".hh");

    find_primary_symbol(summary, &root, is_header, source);
    extract_includes(summary, &root, source);

    Ok(())
}

/// Candidate symbol for ranking
struct SymbolCandidate {
    name: String,
    kind: SymbolKind,
    is_public: bool,
    start_line: usize,
    end_line: usize,
    score: i32,
}

fn find_primary_symbol(summary: &mut SemanticSummary, root: &Node, is_header: bool, source: &str) {
    let mut candidates: Vec<SymbolCandidate> = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    if let Some(name) = extract_declarator_name(&declarator, source) {
                        // Functions in headers are public, main gets bonus
                        let is_public = is_header;
                        let mut score = if is_public { 50 } else { 10 };
                        if name == "main" {
                            score += 40;
                        }

                        candidates.push(SymbolCandidate {
                            name,
                            kind: SymbolKind::Function,
                            is_public,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            score,
                        });
                    }
                }
            }
            "struct_specifier" | "class_specifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(&name_node, source);
                    let is_public = is_header;
                    let score = if is_public { 80 } else { 30 }; // Structs/classes are important

                    candidates.push(SymbolCandidate {
                        name,
                        kind: SymbolKind::Struct,
                        is_public,
                        start_line: child.start_position().row + 1,
                        end_line: child.end_position().row + 1,
                        score,
                    });
                }
            }
            "declaration" => {
                let text = get_node_text(&child, source);
                if text.starts_with("extern") {
                    summary.public_surface_changed = true;
                }
            }
            _ => {}
        }
    }

    // Sort by score (highest first)
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Convert ALL candidates to SymbolInfo and add to summary.symbols
    for candidate in &candidates {
        let symbol_info = SymbolInfo {
            name: candidate.name.clone(),
            kind: candidate.kind,
            start_line: candidate.start_line,
            end_line: candidate.end_line,
            is_exported: candidate.is_public,
            is_default_export: false,
            hash: None,
            arguments: Vec::new(),
            props: Vec::new(),
            return_type: None,
            calls: Vec::new(),
            control_flow: Vec::new(),
            state_changes: Vec::new(),
            behavioral_risk: RiskLevel::Low,
        };
        summary.symbols.push(symbol_info);
    }

    // Use the best candidate for primary symbol (backward compatibility)
    if let Some(best) = candidates.first() {
        summary.symbol = Some(best.name.clone());
        summary.symbol_kind = Some(best.kind);
        summary.start_line = Some(best.start_line);
        summary.end_line = Some(best.end_line);
        if best.is_public {
            summary.public_surface_changed = true;
        }
    }
}

fn extract_declarator_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(get_node_text(node, source)),
        "function_declarator" | "pointer_declarator" => {
            node.child_by_field_name("declarator")
                .and_then(|d| extract_declarator_name(&d, source))
        }
        _ => None,
    }
}

fn extract_includes(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "preproc_include" {
            if let Some(path) = child.child_by_field_name("path") {
                let include = get_node_text(&path, source);
                let clean = include.trim_matches('"').trim_matches('<').trim_matches('>');
                summary.added_dependencies.push(clean.to_string());
            }
        }
    }
}
