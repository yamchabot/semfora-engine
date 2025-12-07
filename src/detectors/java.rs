//! Java language detector

use tree_sitter::{Node, Tree};
use crate::detectors::common::get_node_text;
use crate::error::Result;
use crate::schema::{RiskLevel, SemanticSummary, SymbolInfo, SymbolKind};

pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();
    let filename_stem = extract_filename_stem(&summary.file);

    find_primary_symbol(summary, &root, &filename_stem, source);
    extract_imports(summary, &root, source);

    Ok(())
}

fn extract_filename_stem(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
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

fn find_primary_symbol(summary: &mut SemanticSummary, root: &Node, filename_stem: &str, source: &str) {
    let mut candidates: Vec<SymbolCandidate> = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(&name_node, source);
                    let is_public = has_public_modifier(&child);
                    let kind = match child.kind() {
                        "interface_declaration" => SymbolKind::Trait,
                        "enum_declaration" => SymbolKind::Enum,
                        _ => SymbolKind::Class,
                    };

                    let mut score = if is_public { 50 } else { 0 };
                    score += match kind {
                        SymbolKind::Class => 30,
                        SymbolKind::Trait => 25,
                        SymbolKind::Enum => 20,
                        _ => 10,
                    };
                    if name.to_lowercase() == filename_stem.to_lowercase() {
                        score += 40;
                    }

                    candidates.push(SymbolCandidate {
                        name,
                        kind,
                        is_public,
                        start_line: child.start_position().row + 1,
                        end_line: child.end_position().row + 1,
                        score,
                    });
                }
            }
            _ => {}
        }
    }

    // Sort by score (highest first)
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Convert ALL public candidates to SymbolInfo and add to summary.symbols
    for candidate in &candidates {
        if candidate.is_public || candidate.score > 0 {
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
    }

    // Use the best candidate for primary symbol (backward compatibility)
    if let Some(best) = candidates.first() {
        summary.symbol = Some(best.name.clone());
        summary.symbol_kind = Some(best.kind);
        summary.start_line = Some(best.start_line);
        summary.end_line = Some(best.end_line);
        summary.public_surface_changed = best.is_public;
    }
}

fn has_public_modifier(node: &Node) -> bool {
    if let Some(modifiers) = node.child_by_field_name("modifiers") {
        let mut cursor = modifiers.walk();
        for child in modifiers.children(&mut cursor) {
            if child.kind() == "public" {
                return true;
            }
        }
    }
    false
}

fn extract_imports(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            if let Some(scope) = child.child_by_field_name("scope") {
                let import_text = get_node_text(&scope, source);
                if let Some(last) = import_text.split('.').last() {
                    if last != "*" {
                        summary.added_dependencies.push(last.to_string());
                    }
                }
            }
        }
    }
}
