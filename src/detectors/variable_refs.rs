//! Variable reference extraction with optional local-escape tracking
//!
//! This module extends locals.scm variable references to include local variables
//! that escape their scope (passed as arguments, returned, or assigned to fields).

use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

use crate::detectors::common::{find_containing_symbol_by_line, get_node_text};
use crate::detectors::locals;
use crate::lang::Lang;
use crate::schema::{
    Call, FrameworkEntryPoint, Location, RefKind, RiskLevel, SemanticSummary, SymbolInfo,
    SymbolKind,
};

/// Extract variable references and attach them to symbols.
///
/// When include_escape_locals is true, local variables/parameters that escape
/// their scope are tracked using escape ref kinds.
pub fn extract_variable_references(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    lang: Option<Lang>,
    include_escape_locals: bool,
) {
    let Some(lang) = lang else {
        return;
    };

    let Some(locals_query) = locals::get_locals_query(lang) else {
        return;
    };

    // Module-level Variable symbols (constants, statics, class fields)
    let var_names: HashSet<&str> = summary
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.as_str())
        .collect();

    if !include_escape_locals && var_names.is_empty() {
        return;
    }

    let (escape_locals, escape_local_defs) = if include_escape_locals {
        let escape_candidates = collect_escape_variable_names(root, source, lang);
        if escape_candidates.is_empty() {
            (HashSet::new(), Vec::new())
        } else {
            let local_defs = locals_query.extract_local_definitions_with_locations(root, source);

            let mut name_counts: HashMap<String, usize> = HashMap::new();
            for def in &local_defs {
                *name_counts.entry(def.name.clone()).or_default() += 1;
            }

            let mut escape_locals = HashSet::new();
            let mut escape_local_defs = Vec::new();

            for def in local_defs {
                if !escape_candidates.contains(&def.name) {
                    continue;
                }

                escape_locals.insert(def.name.clone());

                if name_counts.get(&def.name).copied().unwrap_or(0) == 1 {
                    escape_local_defs.push(def);
                }
            }

            (escape_locals, escape_local_defs)
        }
    } else {
        (HashSet::new(), Vec::new())
    };

    if var_names.is_empty() && escape_locals.is_empty() {
        return;
    }

    let references = locals_query.extract_references(root, source);

    let mut calls_by_symbol: HashMap<usize, Vec<Call>> = HashMap::new();
    let mut file_level_refs: Vec<Call> = Vec::new();

    for reference in references {
        let is_module_var = var_names.contains(reference.name.as_str());
        let is_escape_local = escape_locals.contains(&reference.name);

        if !is_module_var && !is_escape_local {
            continue;
        }

        let ref_kind = if is_escape_local {
            map_escape_ref_kind(reference.ref_kind)
        } else {
            reference.ref_kind
        };

        let call = Call {
            name: reference.name.clone(),
            object: None,
            is_awaited: false,
            in_try: false,
            is_hook: false,
            is_io: false,
            ref_kind,
            location: Location::new(reference.line, 0),
        };

        if let Some(symbol_idx) = find_containing_symbol_by_line(reference.line, &summary.symbols) {
            let defining_symbol = &summary.symbols[symbol_idx];
            if defining_symbol.kind == SymbolKind::Variable
                && defining_symbol.name == reference.name
            {
                continue;
            }
            calls_by_symbol.entry(symbol_idx).or_default().push(call);
        } else {
            file_level_refs.push(call);
        }
    }

    // Merge variable references into existing symbol calls (deduplicated)
    for (symbol_idx, var_refs) in calls_by_symbol {
        if symbol_idx < summary.symbols.len() {
            let existing_calls = &mut summary.symbols[symbol_idx].calls;
            let mut seen: HashSet<(String, RefKind)> = existing_calls
                .iter()
                .filter(|c| c.ref_kind.is_variable_ref())
                .map(|c| (c.name.clone(), c.ref_kind))
                .collect();

            for var_ref in var_refs {
                if seen.insert((var_ref.name.clone(), var_ref.ref_kind)) {
                    existing_calls.push(var_ref);
                }
            }
        }
    }

    // Add file-level variable references to summary.calls (deduplicated)
    let mut seen: HashSet<(String, RefKind)> = summary
        .calls
        .iter()
        .filter(|c| c.ref_kind.is_variable_ref())
        .map(|c| (c.name.clone(), c.ref_kind))
        .collect();

    for var_ref in file_level_refs {
        if seen.insert((var_ref.name.clone(), var_ref.ref_kind)) {
            summary.calls.push(var_ref);
        }
    }

    if include_escape_locals {
        add_escape_local_symbols(summary, &escape_local_defs);
    }
}

fn map_escape_ref_kind(kind: RefKind) -> RefKind {
    match kind {
        RefKind::Write => RefKind::EscapeWrite,
        RefKind::ReadWrite => RefKind::EscapeReadWrite,
        _ => RefKind::EscapeRead,
    }
}

fn collect_escape_variable_names(root: &Node, source: &str, lang: Lang) -> HashSet<String> {
    match lang {
        Lang::JavaScript | Lang::TypeScript | Lang::Jsx | Lang::Tsx | Lang::Vue => {
            collect_js_escape_names(root, source)
        }
        Lang::CSharp => collect_csharp_escape_names(root, source),
        _ => HashSet::new(),
    }
}

fn add_escape_local_symbols(summary: &mut SemanticSummary, defs: &[locals::LocalDefinition]) {
    if defs.is_empty() {
        return;
    }

    let mut existing: HashSet<String> = summary
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.clone())
        .collect();

    for def in defs {
        if !existing.insert(def.name.clone()) {
            continue;
        }

        summary.symbols.push(SymbolInfo {
            name: def.name.clone(),
            kind: SymbolKind::Variable,
            start_line: def.start_line,
            end_line: def.end_line,
            is_exported: false,
            is_default_export: false,
            is_escape_local: true,
            hash: None,
            arguments: Vec::new(),
            props: Vec::new(),
            return_type: None,
            calls: Vec::new(),
            control_flow: Vec::new(),
            state_changes: Vec::new(),
            decorators: Vec::new(),
            behavioral_risk: RiskLevel::Low,
            framework_entry_point: FrameworkEntryPoint::None,
            is_async: false,
            base_classes: Vec::new(),
        });
    }
}

fn collect_js_escape_names(root: &Node, source: &str) -> HashSet<String> {
    collect_escape_names(root, source, Lang::JavaScript, is_js_capture_child)
}

fn collect_csharp_escape_names(root: &Node, source: &str) -> HashSet<String> {
    collect_escape_names(root, source, Lang::CSharp, is_csharp_capture_child)
}

fn collect_escape_names(
    root: &Node,
    source: &str,
    lang: Lang,
    capture_child: fn(&Node, &Node) -> bool,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<(Node, bool)> = vec![(*root, false)];

    while let Some((node, capture)) = stack.pop() {
        if capture && is_identifier_node(&node, lang) {
            if !is_in_call_function(&node) && !is_member_property_identifier(&node) {
                let text = get_node_text(&node, source);
                if !text.is_empty() && text.len() <= 100 {
                    names.insert(text);
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let child_capture = capture || capture_child(&node, &child);
            stack.push((child, child_capture));
        }
    }

    names
}

fn is_js_capture_child(parent: &Node, child: &Node) -> bool {
    match parent.kind() {
        "call_expression" => parent
            .child_by_field_name("arguments")
            .map(|args| args == *child)
            .unwrap_or(false),
        "return_statement" => parent
            .child_by_field_name("argument")
            .or_else(|| parent.child_by_field_name("expression"))
            .map(|expr| expr == *child)
            .unwrap_or(false),
        "jsx_attribute" => parent
            .child_by_field_name("value")
            .map(|value| value == *child)
            .unwrap_or(false),
        "assignment_expression" => {
            let left = parent.child_by_field_name("left");
            let right = parent.child_by_field_name("right");
            if let (Some(left), Some(right)) = (left, right) {
                right == *child
                    && matches!(left.kind(), "member_expression" | "subscript_expression")
            } else {
                false
            }
        }
        _ => false,
    }
}

fn is_csharp_capture_child(parent: &Node, child: &Node) -> bool {
    match parent.kind() {
        "invocation_expression" | "object_creation_expression" => {
            find_child_by_kind(parent, "argument_list")
                .map(|args| args == *child)
                .unwrap_or(false)
        }
        "return_statement" => parent
            .child_by_field_name("expression")
            .map(|expr| expr == *child)
            .unwrap_or(false),
        "assignment_expression" => {
            let left = parent.child_by_field_name("left");
            let right = parent.child_by_field_name("right");
            if let (Some(left), Some(right)) = (left, right) {
                right == *child
                    && matches!(
                        left.kind(),
                        "member_access_expression"
                            | "member_binding_expression"
                            | "element_access_expression"
                            | "qualified_name"
                    )
            } else {
                false
            }
        }
        _ => false,
    }
}

fn is_identifier_node(node: &Node, lang: Lang) -> bool {
    match lang {
        Lang::JavaScript | Lang::TypeScript | Lang::Jsx | Lang::Tsx | Lang::Vue => {
            matches!(node.kind(), "identifier" | "shorthand_property_identifier")
        }
        Lang::CSharp => node.kind() == "identifier",
        _ => false,
    }
}

fn is_in_call_function(node: &Node) -> bool {
    let mut current = *node;
    let node_range = node.byte_range();

    while let Some(parent) = current.parent() {
        let parent_kind = parent.kind();
        if parent_kind == "call_expression" || parent_kind == "invocation_expression" {
            if let Some(func) = parent.child_by_field_name("function") {
                let func_range = func.byte_range();
                if node_range.start >= func_range.start && node_range.end <= func_range.end {
                    return true;
                }
            }
            // We reached the call; if not in function position, don't keep walking
            return false;
        }
        current = parent;
    }

    false
}

fn is_member_property_identifier(node: &Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "member_expression" => parent
            .child_by_field_name("property")
            .map(|prop| prop == *node)
            .unwrap_or(false),
        "member_access_expression" => parent
            .child_by_field_name("name")
            .map(|prop| prop == *node)
            .unwrap_or(false),
        _ => false,
    }
}

fn find_child_by_kind<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}
