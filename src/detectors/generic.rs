//! Generic Semantic Extractor
//!
//! This module provides language-agnostic semantic extraction using
//! the grammar definitions from `grammar.rs`. Instead of duplicating
//! extraction logic in each language detector, we use one implementation
//! that works with any tree-sitter grammar.
//!
//! # Usage
//!
//! ```ignore
//! use crate::detectors::generic::extract_with_grammar;
//! use crate::detectors::grammar::GO_GRAMMAR;
//!
//! extract_with_grammar(summary, source, tree, &GO_GRAMMAR)?;
//! ```

use tree_sitter::{Node, Tree};

use crate::detectors::common::{
    find_containing_symbol_by_line, get_node_text, get_node_text_normalized,
};
use crate::detectors::grammar::LangGrammar;
use crate::detectors::variable_refs;
use crate::error::Result;
use crate::lang::Lang;
use crate::schema::{
    Call, ControlFlowChange, ControlFlowKind, FrameworkEntryPoint, Location, RefKind, RiskLevel,
    SemanticSummary, StateChange, SymbolInfo, SymbolKind,
};
use crate::utils::truncate_to_char_boundary;

// =============================================================================
// Main Entry Point
// =============================================================================

/// Extract semantic information from source code using the provided grammar
pub fn extract_with_grammar(
    summary: &mut SemanticSummary,
    source: &str,
    tree: &Tree,
    grammar: &LangGrammar,
) -> Result<()> {
    let root = tree.root_node();

    // Extract all semantic information
    extract_symbols(summary, &root, source, grammar);
    extract_imports(summary, &root, source, grammar);
    extract_state_changes(summary, &root, source, grammar);
    extract_control_flow(summary, &root, source, grammar);
    extract_calls(summary, &root, source, grammar);
    extract_variable_references(summary, &root, source, grammar);

    // Calculate derived metrics
    calculate_complexity(summary);
    determine_risk(summary);

    Ok(())
}

// =============================================================================
// Symbol Extraction
// =============================================================================

/// Candidate symbol for ranking
struct SymbolCandidate {
    name: String,
    kind: SymbolKind,
    is_exported: bool,
    start_line: usize,
    end_line: usize,
    score: i32,
    decorators: Vec<String>,
}

/// Extract all symbols (functions, classes, interfaces, enums)
fn extract_symbols(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    grammar: &LangGrammar,
) {
    let mut candidates: Vec<SymbolCandidate> = Vec::new();
    let filename_stem = extract_filename_stem(&summary.file);

    collect_symbols_recursive(root, source, grammar, &filename_stem, &mut candidates);

    // Sort by score (highest first)
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Convert to SymbolInfo and add to summary
    for candidate in &candidates {
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
            decorators: candidate.decorators.clone(),
            behavioral_risk: RiskLevel::Low,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::None,
        };
        summary.symbols.push(symbol_info);
    }

    // Set primary symbol (backward compatibility)
    if let Some(best) = candidates.first() {
        summary.symbol = Some(best.name.clone());
        summary.symbol_kind = Some(best.kind);
        summary.start_line = Some(best.start_line);
        summary.end_line = Some(best.end_line);
        summary.public_surface_changed = best.is_exported;
    }
}

fn collect_symbols_recursive(
    node: &Node,
    source: &str,
    grammar: &LangGrammar,
    filename_stem: &str,
    candidates: &mut Vec<SymbolCandidate>,
) {
    // Iterative traversal using tree-sitter cursor to avoid stack overflow
    let mut cursor = node.walk();
    let mut did_visit_children = false;

    loop {
        if !did_visit_children {
            let current_node = cursor.node();
            let kind_str = current_node.kind();

            // Check if this node is a symbol
            let symbol_kind = if grammar.function_nodes.contains(&kind_str) {
                Some(SymbolKind::Function)
            } else if grammar.class_nodes.contains(&kind_str) {
                Some(SymbolKind::Class)
            } else if grammar.interface_nodes.contains(&kind_str) {
                Some(SymbolKind::Trait)
            } else if grammar.enum_nodes.contains(&kind_str) {
                Some(SymbolKind::Enum)
            } else if grammar.module_var_nodes.contains(&kind_str) {
                // Module-level variable (const, static, top-level declaration)
                // Only extract if NOT inside a local scope
                if !is_in_local_scope(&current_node, grammar) {
                    Some(SymbolKind::Variable)
                } else {
                    None
                }
            } else if grammar.field_nodes.contains(&kind_str) {
                // Class/struct field - only extract if inside a class body
                if is_class_field(&current_node, grammar) {
                    Some(SymbolKind::Variable)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(kind) = symbol_kind {
                if let Some(name) = extract_symbol_name(&current_node, source, grammar) {
                    let is_exported = (grammar.is_exported)(&current_node, source);
                    let score =
                        calculate_symbol_score(&name, &kind, is_exported, filename_stem, grammar);
                    let decorators = extract_decorators(&current_node, source, grammar);

                    candidates.push(SymbolCandidate {
                        name,
                        kind,
                        is_exported,
                        start_line: current_node.start_position().row + 1,
                        end_line: current_node.end_position().row + 1,
                        score,
                        decorators,
                    });
                }
            }

            // Try to go to first child
            if cursor.goto_first_child() {
                did_visit_children = false;
                continue;
            }
        }

        // Try to go to next sibling
        if cursor.goto_next_sibling() {
            did_visit_children = false;
            continue;
        }

        // Go back to parent
        if !cursor.goto_parent() {
            break; // Reached the root, we're done
        }
        did_visit_children = true;
    }
}

// =============================================================================
// Variable Symbol Scope Detection
// =============================================================================

/// Check if a node is inside a local scope (function body, loop, etc.)
/// Variables inside local scopes are filtered out - only module-level variables become symbols.
fn is_in_local_scope(node: &Node, grammar: &LangGrammar) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        let kind = p.kind();
        // If we hit a local scope, the variable is local (filter it out)
        if grammar.local_scope_nodes.contains(&kind) {
            return true;
        }
        // If we hit program/source_file/module root, it's module-level (keep it)
        if kind == "program"
            || kind == "source_file"
            || kind == "translation_unit"
            || kind == "module"
            || kind == "compilation_unit"
        {
            return false;
        }
        parent = p.parent();
    }
    // Reached root without hitting local scope - it's module-level
    false
}

/// Check if a node is a class field (direct child of class body)
/// Returns true if the node is inside a class/struct body but not inside a method.
fn is_class_field(node: &Node, grammar: &LangGrammar) -> bool {
    if let Some(parent) = node.parent() {
        let parent_kind = parent.kind();
        // Check if parent looks like a class body
        if parent_kind.contains("body")
            || parent_kind.contains("class_body")
            || parent_kind.contains("struct_body")
            || parent_kind == "declaration_list"
            || parent_kind == "field_declaration_list"
        {
            // Verify grandparent is a class/struct node
            if let Some(grandparent) = parent.parent() {
                let gp_kind = grandparent.kind();
                return grammar.class_nodes.contains(&gp_kind)
                    || gp_kind.contains("class")
                    || gp_kind.contains("struct");
            }
        }
    }
    false
}

/// Extract decorators/attributes from a symbol node
///
/// Looks for decorator nodes in:
/// 1. Preceding siblings (Python @decorator, C# \[Attribute\])
/// 2. Child nodes (some grammars nest attributes within the declaration)
fn extract_decorators(node: &Node, source: &str, grammar: &LangGrammar) -> Vec<String> {
    let mut decorators = Vec::new();

    if grammar.decorator_nodes.is_empty() {
        return decorators;
    }

    // Check preceding siblings for decorators (common in Python, C#, Java)
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        let sibling_kind = sibling.kind();
        if grammar.decorator_nodes.contains(&sibling_kind) {
            if let Some(text) = extract_decorator_text(&sibling, source) {
                decorators.push(text);
            }
        } else {
            // Stop when we hit a non-decorator node
            break;
        }
        prev = sibling.prev_sibling();
    }

    // Also check direct children (some grammars nest attributes within declaration)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_kind = child.kind();
        if grammar.decorator_nodes.contains(&child_kind) {
            // For attribute_list nodes, extract each attribute
            if child_kind == "attribute_list" {
                let mut inner_cursor = child.walk();
                for attr in child.children(&mut inner_cursor) {
                    if attr.kind() == "attribute" {
                        if let Some(text) = extract_decorator_text(&attr, source) {
                            decorators.push(text);
                        }
                    }
                }
            } else if let Some(text) = extract_decorator_text(&child, source) {
                decorators.push(text);
            }
        }
    }

    // Reverse so they're in source order (we collected backwards from prev_sibling)
    decorators.reverse();
    decorators
}

/// Extract the text representation of a decorator/attribute
fn extract_decorator_text(node: &Node, source: &str) -> Option<String> {
    // Try to get just the name/identifier
    if let Some(name_node) = node.child_by_field_name("name") {
        let text = get_node_text(&name_node, source);
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Look for identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier"
            || child.kind() == "name"
            || child.kind() == "scoped_identifier"
        {
            let text = get_node_text(&child, source);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    // Fallback: get the whole node text (for simple decorators)
    let text = get_node_text(node, source);
    if !text.is_empty() {
        // Clean up common decorator prefixes/brackets
        let cleaned = text
            .trim()
            .trim_start_matches('@')
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim();

        // Extract just the attribute name (before any parentheses)
        if let Some(paren_idx) = cleaned.find('(') {
            return Some(cleaned[..paren_idx].to_string());
        }
        return Some(cleaned.to_string());
    }

    None
}

fn extract_symbol_name(node: &Node, source: &str, grammar: &LangGrammar) -> Option<String> {
    // Try the configured name field first
    if let Some(name_node) = node.child_by_field_name(grammar.name_field) {
        let name = get_node_text(&name_node, source);
        if !name.is_empty() {
            return Some(name);
        }
    }

    // Fallback: look for identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            let name = get_node_text(&child, source);
            if !name.is_empty() {
                return Some(name);
            }
        }
    }

    None
}

fn calculate_symbol_score(
    name: &str,
    kind: &SymbolKind,
    is_exported: bool,
    filename_stem: &str,
    grammar: &LangGrammar,
) -> i32 {
    let mut score = 0;

    // Base score by kind
    score += match kind {
        SymbolKind::Class => 30,
        SymbolKind::Struct => 30,
        SymbolKind::Trait => 28,
        SymbolKind::Enum => 25,
        SymbolKind::Function => 10,
        SymbolKind::Method => 15,
        _ => 5,
    };

    // Bonus for exported
    if is_exported {
        score += 50;
    }

    // Bonus for filename match
    let name_lower = name.to_lowercase();
    if name_lower == filename_stem {
        score += 40; // Exact match
    } else if name_lower.contains(filename_stem) || filename_stem.contains(&name_lower) {
        score += 20; // Partial match
    }

    // Bonus for main/Main
    if name == "main" || name == "Main" {
        score += 30;
    }

    // Penalty for test functions
    if name.starts_with("test") || name.starts_with("Test") || name.starts_with("_test") {
        score -= 30;
    }

    // Go-specific: uppercase bonus already handled by is_exported
    if grammar.uppercase_is_export
        && !is_exported
        && name
            .chars()
            .next()
            .map(|c| c.is_lowercase())
            .unwrap_or(true)
    {
        score -= 10;
    }

    score
}

fn extract_filename_stem(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

// =============================================================================
// Import Extraction
// =============================================================================

fn extract_imports(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    grammar: &LangGrammar,
) {
    visit_all(root, |node| {
        let kind = node.kind();
        if grammar.import_nodes.contains(&kind) {
            if let Some(import_name) = extract_import_name(node, source, grammar) {
                if !import_name.is_empty() && !summary.added_dependencies.contains(&import_name) {
                    summary.added_dependencies.push(import_name);
                }
            }
        }
    });
}

fn extract_import_name(node: &Node, source: &str, _grammar: &LangGrammar) -> Option<String> {
    // Try common patterns

    // Pattern 1: path field (Go imports)
    if let Some(path_node) = node.child_by_field_name("path") {
        let path = get_node_text(&path_node, source);
        let clean = path.trim_matches('"').trim_matches('\'');
        if let Some(last) = clean.split('/').last() {
            return Some(last.to_string());
        }
    }

    // Pattern 2: source field (JS/TS imports)
    if let Some(source_node) = node.child_by_field_name("source") {
        let path = get_node_text(&source_node, source);
        let clean = path.trim_matches('"').trim_matches('\'');
        if let Some(last) = clean.split('/').last() {
            return Some(last.to_string());
        }
    }

    // Pattern 3: module_name field (Python imports)
    if let Some(module) = node.child_by_field_name("module_name") {
        return Some(get_node_text(&module, source));
    }
    if let Some(name) = node.child_by_field_name("name") {
        return Some(get_node_text(&name, source));
    }

    // Pattern 4: argument field (Rust use declarations)
    if let Some(arg) = node.child_by_field_name("argument") {
        let text = get_node_text_normalized(&arg, source);
        // Extract the first path segment
        if let Some(first) = text.split("::").next() {
            return Some(first.trim().to_string());
        }
    }

    // Pattern 5: C/C++ includes
    if node.kind() == "preproc_include" {
        if let Some(path) = node.child_by_field_name("path") {
            let include = get_node_text(&path, source);
            let clean = include
                .trim_matches('"')
                .trim_matches('<')
                .trim_matches('>');
            return Some(clean.to_string());
        }
    }

    // Fallback: get the whole node text and extract something useful
    let text = get_node_text(node, source);
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 1 {
        // Skip "import", "use", "from", "#include"
        if let Some(last) = words.last() {
            let clean =
                last.trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == '<' || c == '>');
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }

    None
}

// =============================================================================
// State Change Extraction
// =============================================================================

fn extract_state_changes(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    grammar: &LangGrammar,
) {
    visit_all(root, |node| {
        let kind = node.kind();

        // Variable declarations
        if grammar.var_declaration_nodes.contains(&kind) {
            if let Some(state_change) = extract_var_declaration(node, source, grammar) {
                summary.state_changes.push(state_change);
            }
        }

        // Assignments
        if grammar.assignment_nodes.contains(&kind) {
            if let Some(state_change) = extract_assignment(node, source, grammar) {
                summary.state_changes.push(state_change);
            }
        }
    });
}

fn extract_var_declaration(
    node: &Node,
    source: &str,
    grammar: &LangGrammar,
) -> Option<StateChange> {
    // Try to get name from various fields
    let name = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("declarator"))
        .or_else(|| node.child_by_field_name("left"))
        .or_else(|| find_identifier_child(node))
        .map(|n| get_node_text(&n, source))?;

    if name.is_empty() {
        return None;
    }

    // Try to get type
    let state_type = node
        .child_by_field_name(grammar.type_field)
        .or_else(|| node.child_by_field_name("type"))
        .map(|n| get_node_text_normalized(&n, source))
        .unwrap_or_else(|| "_".to_string());

    // Try to get initializer
    let initializer = node
        .child_by_field_name(grammar.value_field)
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("right"))
        .map(|n| compress_initializer(&get_node_text_normalized(&n, source)))
        .unwrap_or_default();

    Some(StateChange {
        name,
        state_type,
        initializer,
    })
}

fn extract_assignment(node: &Node, source: &str, grammar: &LangGrammar) -> Option<StateChange> {
    let left = node.child_by_field_name("left")?;
    let right = node
        .child_by_field_name(grammar.value_field)
        .or_else(|| node.child_by_field_name("right"))?;

    let name = get_node_text(&left, source);
    if name.is_empty() {
        return None;
    }

    let initializer = compress_initializer(&get_node_text_normalized(&right, source));

    Some(StateChange {
        name,
        state_type: "_".to_string(),
        initializer,
    })
}

fn find_identifier_child<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "variable_declarator" {
            return Some(child);
        }
    }
    None
}

fn compress_initializer(init: &str) -> String {
    // Use the common utility if available, otherwise simple truncation
    if init.len() <= 60 {
        init.to_string()
    } else {
        format!("{}...", truncate_to_char_boundary(init, 57))
    }
}

// =============================================================================
// Control Flow Extraction
// =============================================================================

fn extract_control_flow(
    summary: &mut SemanticSummary,
    root: &Node,
    _source: &str,
    grammar: &LangGrammar,
) {
    let mut results: Vec<ControlFlowChange> = Vec::new();
    collect_control_flow_recursive(root, 0, grammar, &mut results);
    summary.control_flow_changes.extend(results);
}

fn collect_control_flow_recursive(
    node: &Node,
    _depth: usize, // Ignored - we track depth with our own stack
    grammar: &LangGrammar,
    results: &mut Vec<ControlFlowChange>,
) {
    // Iterative traversal using tree-sitter cursor to avoid stack overflow
    let mut cursor = node.walk();
    let mut depth_stack: Vec<usize> = vec![0];
    let mut did_visit_children = false;

    loop {
        if !did_visit_children {
            let current_node = cursor.node();
            let kind = current_node.kind();
            let current_depth = *depth_stack.last().unwrap_or(&0);

            if grammar.control_flow_nodes.contains(&kind) || grammar.try_nodes.contains(&kind) {
                let cf_kind = map_control_flow_kind(kind, grammar);

                let location = Location::new(
                    current_node.start_position().row + 1,
                    current_node.start_position().column,
                );

                results.push(ControlFlowChange {
                    kind: cf_kind,
                    location,
                    nesting_depth: current_depth,
                });
            }

            let is_control_flow =
                grammar.control_flow_nodes.contains(&kind) || grammar.try_nodes.contains(&kind);
            let child_depth = if is_control_flow {
                current_depth + 1
            } else {
                current_depth
            };

            // Try to go to first child
            if cursor.goto_first_child() {
                depth_stack.push(child_depth);
                did_visit_children = false;
                continue;
            }
        }

        // Try to go to next sibling
        if cursor.goto_next_sibling() {
            did_visit_children = false;
            continue;
        }

        // Go back to parent
        if !cursor.goto_parent() {
            break; // Reached the root, we're done
        }
        depth_stack.pop();
        did_visit_children = true;
    }
}

fn map_control_flow_kind(node_kind: &str, grammar: &LangGrammar) -> ControlFlowKind {
    // Check try nodes first
    if grammar.try_nodes.contains(&node_kind) {
        return ControlFlowKind::Try;
    }

    // Map based on node name patterns
    if node_kind.contains("if") {
        ControlFlowKind::If
    } else if node_kind.contains("for") || node_kind.contains("loop") {
        ControlFlowKind::For
    } else if node_kind.contains("while") {
        ControlFlowKind::While
    } else if node_kind.contains("match") || node_kind.contains("switch") {
        ControlFlowKind::Match
    } else if node_kind.contains("try") {
        ControlFlowKind::Try
    } else if node_kind.contains("with") {
        ControlFlowKind::Try // 'with' is like a context manager
    } else {
        ControlFlowKind::If // Default fallback
    }
}

// =============================================================================
// Call Extraction
// =============================================================================

fn extract_calls(summary: &mut SemanticSummary, root: &Node, source: &str, grammar: &LangGrammar) {
    // Collect all calls first with their line numbers
    let mut all_calls: Vec<(Call, usize)> = Vec::new();

    visit_all(root, |node| {
        let kind = node.kind();

        if grammar.call_nodes.contains(&kind) {
            if let Some(call) = extract_call(node, source, grammar) {
                let line = node.start_position().row + 1;
                all_calls.push((call, line));
            }
        }
    });

    // Attribute calls to symbols based on line ranges
    let mut calls_by_symbol: std::collections::HashMap<usize, Vec<Call>> =
        std::collections::HashMap::new();
    let mut file_level_calls: Vec<Call> = Vec::new();

    for (call, line) in all_calls {
        if let Some(symbol_idx) = find_containing_symbol_by_line(line, &summary.symbols) {
            calls_by_symbol.entry(symbol_idx).or_default().push(call);
        } else {
            // Call is at file level (not inside any symbol)
            file_level_calls.push(call);
        }
    }

    // Assign calls to their respective symbols (deduplicated per symbol)
    for (symbol_idx, calls) in calls_by_symbol {
        if symbol_idx < summary.symbols.len() {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut deduped_calls: Vec<Call> = Vec::new();
            for call in calls {
                let key = format!("{}:{}", call.name, call.is_awaited);
                if !seen.contains(&key) {
                    seen.insert(key);
                    deduped_calls.push(call);
                }
            }
            summary.symbols[symbol_idx].calls = deduped_calls;
        }
    }

    // Keep file-level calls in summary.calls for backward compatibility (deduplicated)
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for call in file_level_calls {
        let key = format!("{}:{}", call.name, call.is_awaited);
        if !seen.contains(&key) {
            seen.insert(key);
            summary.calls.push(call);
        }
    }
}

/// Node types that represent constructor/instantiation calls
const CONSTRUCTOR_NODE_TYPES: &[&str] = &[
    "object_creation_expression", // C#, Java
    "new_expression",             // JavaScript, TypeScript
];

fn extract_call(node: &Node, source: &str, grammar: &LangGrammar) -> Option<Call> {
    let node_kind = node.kind();

    // Get the function/type name - constructor nodes have different structure
    let func_node = if CONSTRUCTOR_NODE_TYPES.contains(&node_kind) {
        // For constructor calls, the type name is in "type" field (C#/Java) or "constructor" field (JS/TS)
        // Fallback to child(1) since child(0) is typically the "new" keyword
        node.child_by_field_name("type")
            .or_else(|| node.child_by_field_name("constructor"))
            .or_else(|| node.child(1)) // Skip "new" keyword at child(0)
    } else {
        // Regular function calls
        node.child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child(0))
    }?;

    let full_name = get_node_text(&func_node, source);
    if full_name.is_empty() || full_name.len() > 100 {
        return None;
    }

    // Split into object and method for method calls (e.g., "console.log" -> object="console", name="log")
    let (object, name) = if full_name.contains('.') {
        let parts: Vec<&str> = full_name.rsplitn(2, '.').collect();
        if parts.len() == 2 {
            (Some(parts[1].to_string()), parts[0].to_string())
        } else {
            (None, full_name)
        }
    } else {
        (None, full_name)
    };

    // Check if this is an async call (inside await)
    let is_awaited = if let Some(parent) = node.parent() {
        grammar.await_nodes.contains(&parent.kind())
    } else {
        false
    };

    // Check if this is inside a try block
    let in_try = is_inside_try(node, grammar);

    // Check if this is a React hook
    let is_hook = Call::check_is_hook(&name);

    // Check if this is an I/O operation
    let is_io = Call::check_is_io(&name);

    let location = Location::new(node.start_position().row + 1, node.start_position().column);

    Some(Call {
        name,
        object,
        is_awaited,
        in_try,
        is_hook,
        is_io,
        ref_kind: RefKind::None,
        location,
    })
}

fn is_inside_try(node: &Node, grammar: &LangGrammar) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if grammar.try_nodes.contains(&parent.kind()) {
            return true;
        }
        current = parent.parent();
    }
    false
}

// =============================================================================
// Variable Reference Extraction
// =============================================================================

/// Map grammar name to Lang enum for locals query lookup
fn grammar_name_to_lang(name: &str) -> Option<Lang> {
    match name {
        "rust" => Some(Lang::Rust),
        "go" => Some(Lang::Go),
        "java" => Some(Lang::Java),
        "csharp" => Some(Lang::CSharp),
        "python" => Some(Lang::Python),
        "javascript" => Some(Lang::JavaScript),
        "typescript" => Some(Lang::TypeScript),
        "c" => Some(Lang::C),
        "cpp" => Some(Lang::Cpp),
        "kotlin" => Some(Lang::Kotlin),
        "bash" => Some(Lang::Bash),
        "gradle" => Some(Lang::Gradle),
        "hcl" => Some(Lang::Hcl),
        // Config/Markup languages don't have variable references
        _ => None,
    }
}

/// Extract variable references using tree-sitter locals.scm queries
///
/// This finds all references to module-level Variable symbols (constants, statics)
/// and adds them to the symbol's `calls` list with `ref_kind` set to Read/Write.
fn extract_variable_references(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    grammar: &LangGrammar,
) {
    let lang = grammar_name_to_lang(grammar.name);
    let include_escape_locals = matches!(lang, Some(Lang::CSharp));
    variable_refs::extract_variable_references(
        summary,
        root,
        source,
        lang,
        include_escape_locals,
    );
}

// =============================================================================
// Complexity and Risk Calculation
// =============================================================================

fn calculate_complexity(_summary: &mut SemanticSummary) {
    // Cognitive complexity is calculated from control flow changes
    // This affects the behavioral_risk level
}

fn determine_risk(summary: &mut SemanticSummary) {
    // Calculate cognitive complexity from control flow
    let mut complexity: usize = 0;
    let mut max_depth: usize = 0;

    for cf in &summary.control_flow_changes {
        // Base complexity for each control flow construct
        complexity += 1;

        // Nesting penalty
        complexity += cf.nesting_depth;

        // Track max depth
        if cf.nesting_depth > max_depth {
            max_depth = cf.nesting_depth;
        }

        // Extra penalty for complex constructs
        match cf.kind {
            ControlFlowKind::Match => complexity += 1,
            ControlFlowKind::Try => complexity += 1,
            _ => {}
        }
    }

    let state_count = summary.state_changes.len();
    let call_count = summary.calls.len();

    // Risk scoring
    let risk_score = complexity / 5 + max_depth * 2 + state_count / 10 + call_count / 20;

    summary.behavioral_risk = if risk_score > 20 {
        RiskLevel::High
    } else if risk_score > 8 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
}

// =============================================================================
// Utility Functions
// =============================================================================

fn visit_all<F>(node: &Node, mut callback: F)
where
    F: FnMut(&Node),
{
    // Iterative traversal using tree-sitter cursor to avoid stack overflow
    let mut cursor = node.walk();
    let mut did_visit_children = false;

    loop {
        if !did_visit_children {
            callback(&cursor.node());

            // Try to go to first child
            if cursor.goto_first_child() {
                did_visit_children = false;
                continue;
            }
        }

        // Try to go to next sibling
        if cursor.goto_next_sibling() {
            did_visit_children = false;
            continue;
        }

        // Go back to parent
        if !cursor.goto_parent() {
            break; // Reached the root, we're done
        }
        did_visit_children = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detectors::common::find_containing_symbol_by_line;
    use crate::extract::extract;
    use crate::lang::Lang;
    use std::path::PathBuf;
    use tree_sitter::{Parser, Tree};

    /// Helper to parse source code into a tree-sitter Tree
    fn parse_source(source: &str, lang: Lang) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .expect("Failed to set language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_filename_stem() {
        assert_eq!(extract_filename_stem("/path/to/main.rs"), "main");
        assert_eq!(extract_filename_stem("server.go"), "server");
        assert_eq!(extract_filename_stem("MyClass.java"), "myclass");
    }

    #[test]
    fn test_compress_initializer() {
        assert_eq!(compress_initializer("simple"), "simple");
        // Input: 65 chars, truncated to first 57 chars + "..."
        assert_eq!(
            compress_initializer("this is a very long initializer that should be truncated to fit"),
            "this is a very long initializer that should be truncated ..."
        );
    }

    // =============================================================================
    // Call Graph Tests - Symbol-Level Call Attribution
    // =============================================================================

    /// Test that Python functions have calls attributed to symbols, not just file-level
    #[test]
    fn test_python_call_attribution() {
        let source = r#"
def fetch_users():
    response = requests.get("https://api.example.com/users")
    data = response.json()
    return process_data(data)

def process_data(data):
    return [transform(item) for item in data]
"#;
        let tree = parse_source(source, Lang::Python);
        let path = PathBuf::from("/test/api.py");
        let summary = extract(&path, source, &tree, Lang::Python).unwrap();

        // Should have 2 symbols
        assert_eq!(summary.symbols.len(), 2, "Should have 2 Python functions");

        // fetch_users should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetch_users");
        assert!(fetch_users.is_some(), "Should find fetch_users symbol");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetch_users should have calls attributed to it, got: {:?}",
            fetch_users.calls
        );
        // Should contain requests.get, response.json, process_data
        let call_names: Vec<_> = fetch_users.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(call_names.contains(&"get"), "Should have requests.get call");
        assert!(
            call_names.contains(&"process_data"),
            "Should have process_data call"
        );

        // process_data should have transform call
        let process_data = summary.symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data.is_some(), "Should find process_data symbol");
        let process_data = process_data.unwrap();
        assert!(
            !process_data.calls.is_empty(),
            "process_data should have transform call"
        );
    }

    /// Test that Go functions have calls attributed to symbols
    #[test]
    fn test_go_call_attribution() {
        let source = r#"
package main

import "fmt"

func FetchUsers() []User {
    resp, err := http.Get("https://api.example.com/users")
    if err != nil {
        log.Fatal(err)
    }
    return processResponse(resp)
}

func processResponse(resp *http.Response) []User {
    defer resp.Body.Close()
    return parseJSON(resp.Body)
}
"#;
        let tree = parse_source(source, Lang::Go);
        let path = PathBuf::from("/test/api.go");
        let summary = extract(&path, source, &tree, Lang::Go).unwrap();

        // Should have 2 symbols
        assert_eq!(summary.symbols.len(), 2, "Should have 2 Go functions");

        // FetchUsers should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "FetchUsers");
        assert!(fetch_users.is_some(), "Should find FetchUsers symbol");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "FetchUsers should have calls attributed to it, got: {:?}",
            fetch_users.calls
        );
        // Should contain http.Get, log.Fatal, processResponse
        let call_names: Vec<_> = fetch_users.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(call_names.contains(&"Get"), "Should have http.Get call");
        assert!(
            call_names.contains(&"processResponse"),
            "Should have processResponse call"
        );

        // processResponse should have calls
        let process_response = summary.symbols.iter().find(|s| s.name == "processResponse");
        assert!(
            process_response.is_some(),
            "Should find processResponse symbol"
        );
        let process_response = process_response.unwrap();
        assert!(
            !process_response.calls.is_empty(),
            "processResponse should have calls"
        );
    }

    /// Test that Rust functions have calls attributed to symbols
    #[test]
    fn test_rust_call_attribution() {
        let source = r#"
pub fn fetch_users() -> Vec<User> {
    let response = reqwest::get("url").await?;
    let data = response.json().await?;
    process_data(data)
}

fn process_data(data: Vec<RawUser>) -> Vec<User> {
    data.into_iter().map(transform).collect()
}
"#;
        let tree = parse_source(source, Lang::Rust);
        let path = PathBuf::from("/test/api.rs");
        let summary = extract(&path, source, &tree, Lang::Rust).unwrap();

        // Should have 2 symbols
        assert_eq!(summary.symbols.len(), 2, "Should have 2 Rust functions");

        // fetch_users should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetch_users");
        assert!(fetch_users.is_some(), "Should find fetch_users symbol");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetch_users should have calls attributed to it, got: {:?}",
            fetch_users.calls
        );
        // Should contain process_data
        let call_names: Vec<_> = fetch_users.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            call_names.contains(&"process_data"),
            "Should have process_data call"
        );

        // process_data should have calls (collect, map, into_iter)
        let process_data = summary.symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data.is_some(), "Should find process_data symbol");
        let process_data = process_data.unwrap();
        assert!(
            !process_data.calls.is_empty(),
            "process_data should have calls"
        );
    }

    /// Test that Java methods have calls attributed to symbols
    #[test]
    fn test_java_call_attribution() {
        let source = r#"
public class UserService {
    public List<User> fetchUsers() {
        Response response = httpClient.get("url");
        return processResponse(response);
    }

    private List<User> processResponse(Response response) {
        return response.body().parseJson();
    }
}
"#;
        let tree = parse_source(source, Lang::Java);
        let path = PathBuf::from("/test/UserService.java");
        let summary = extract(&path, source, &tree, Lang::Java).unwrap();

        // Should have symbols (class + methods)
        assert!(!summary.symbols.is_empty(), "Should have Java symbols");

        // fetchUsers should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some(), "Should find fetchUsers method");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls attributed to it"
        );
    }

    /// Test that file-level calls (not inside any function) go to summary.calls
    #[test]
    fn test_file_level_calls() {
        let source = r#"
import requests

# File-level call (not in a function)
config = load_config()

def fetch_data():
    return requests.get(config.url)
"#;
        let tree = parse_source(source, Lang::Python);
        let path = PathBuf::from("/test/script.py");
        let summary = extract(&path, source, &tree, Lang::Python).unwrap();

        // Should have 1 symbol (fetch_data)
        let fetch_data = summary.symbols.iter().find(|s| s.name == "fetch_data");
        assert!(fetch_data.is_some(), "Should find fetch_data function");

        // File-level call (load_config) should be in summary.calls, not in any symbol
        let file_level_call_names: Vec<_> = summary.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            file_level_call_names.contains(&"load_config"),
            "File-level load_config should be in summary.calls"
        );
    }

    /// Test that find_containing_symbol_by_line works correctly
    #[test]
    fn test_find_containing_symbol_by_line() {
        let symbols = vec![
            SymbolInfo {
                name: "func1".to_string(),
                kind: SymbolKind::Function,
                start_line: 1,
                end_line: 5,
                ..Default::default()
            },
            SymbolInfo {
                name: "func2".to_string(),
                kind: SymbolKind::Function,
                start_line: 7,
                end_line: 15,
                ..Default::default()
            },
        ];

        // Line 3 is inside func1
        assert_eq!(find_containing_symbol_by_line(3, &symbols), Some(0));
        // Line 10 is inside func2
        assert_eq!(find_containing_symbol_by_line(10, &symbols), Some(1));
        // Line 6 is not inside any symbol
        assert_eq!(find_containing_symbol_by_line(6, &symbols), None);
        // Line 20 is not inside any symbol
        assert_eq!(find_containing_symbol_by_line(20, &symbols), None);
    }

    /// Test that nested symbols prefer the most specific (smallest) match
    #[test]
    fn test_find_containing_symbol_nested() {
        let symbols = vec![
            SymbolInfo {
                name: "MyClass".to_string(),
                kind: SymbolKind::Class,
                start_line: 1,
                end_line: 20, // Class spans lines 1-20
                ..Default::default()
            },
            SymbolInfo {
                name: "method1".to_string(),
                kind: SymbolKind::Function,
                start_line: 3,
                end_line: 8, // Method within class
                ..Default::default()
            },
            SymbolInfo {
                name: "method2".to_string(),
                kind: SymbolKind::Function,
                start_line: 10,
                end_line: 15, // Another method within class
                ..Default::default()
            },
        ];

        // Line 5 is inside both MyClass and method1 - should prefer method1 (smaller range)
        assert_eq!(find_containing_symbol_by_line(5, &symbols), Some(1));
        // Line 12 is inside both MyClass and method2 - should prefer method2
        assert_eq!(find_containing_symbol_by_line(12, &symbols), Some(2));
        // Line 18 is only inside MyClass (after all methods)
        assert_eq!(find_containing_symbol_by_line(18, &symbols), Some(0));
    }

    /// Test that C functions have calls attributed to symbols
    #[test]
    fn test_c_call_attribution() {
        let source = r#"
#include <stdio.h>

void process_data(int* data, int len) {
    for (int i = 0; i < len; i++) {
        printf("%d\n", data[i]);
    }
}

int fetch_users(void) {
    int data[10];
    load_from_db(data, 10);
    process_data(data, 10);
    return 0;
}
"#;
        let tree = parse_source(source, Lang::C);
        let path = PathBuf::from("/test/api.c");
        let summary = extract(&path, source, &tree, Lang::C).unwrap();

        // Should have 2 symbols
        assert!(
            summary.symbols.len() >= 2,
            "Should have at least 2 C functions"
        );

        // Find function with printf call
        let has_calls = summary.symbols.iter().any(|s| !s.calls.is_empty());
        assert!(
            has_calls,
            "At least one C function should have calls attributed"
        );
    }

    /// Test that C++ methods have calls attributed to symbols
    #[test]
    fn test_cpp_call_attribution() {
        let source = r#"
#include <vector>

class UserService {
public:
    std::vector<User> fetchUsers() {
        auto response = httpClient.get("url");
        return processResponse(response);
    }

private:
    std::vector<User> processResponse(Response& response) {
        return response.parse();
    }
};
"#;
        let tree = parse_source(source, Lang::Cpp);
        let path = PathBuf::from("/test/UserService.cpp");
        let summary = extract(&path, source, &tree, Lang::Cpp).unwrap();

        // Should have symbols (class + methods)
        assert!(!summary.symbols.is_empty(), "Should have C++ symbols");

        // fetchUsers should have calls (not the class)
        let fetch_users = summary
            .symbols
            .iter()
            .find(|s| s.name.contains("fetchUsers"));
        assert!(fetch_users.is_some(), "Should find fetchUsers method");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls attributed to it"
        );
    }

    /// Test that Kotlin functions have calls attributed to symbols
    #[test]
    fn test_kotlin_call_attribution() {
        let source = r#"
class UserService {
    fun fetchUsers(): List<User> {
        val response = httpClient.get("url")
        return processResponse(response)
    }

    private fun processResponse(response: Response): List<User> {
        return response.body().parseJson()
    }
}
"#;
        let tree = parse_source(source, Lang::Kotlin);
        let path = PathBuf::from("/test/UserService.kt");
        let summary = extract(&path, source, &tree, Lang::Kotlin).unwrap();

        // Should have symbols
        assert!(!summary.symbols.is_empty(), "Should have Kotlin symbols");

        // fetchUsers should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some(), "Should find fetchUsers function");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls attributed to it"
        );
    }

    /// Test that Shell functions have calls attributed to symbols
    #[test]
    fn test_shell_call_attribution() {
        let source = r#"
#!/bin/bash

process_data() {
    echo "Processing: $1"
    grep -r "$1" /var/log
}

fetch_users() {
    local data=$(curl -s "https://api.example.com/users")
    process_data "$data"
    echo "Done"
}
"#;
        let tree = parse_source(source, Lang::Bash);
        let path = PathBuf::from("/test/script.sh");
        let summary = extract(&path, source, &tree, Lang::Bash).unwrap();

        // Should have 2 shell functions
        assert_eq!(summary.symbols.len(), 2, "Should have 2 shell functions");

        // fetch_users should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetch_users");
        assert!(fetch_users.is_some(), "Should find fetch_users function");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetch_users should have calls attributed to it"
        );
    }

    /// Test that Gradle/Groovy functions have calls attributed to symbols
    #[test]
    fn test_gradle_call_attribution() {
        let source = r#"
def compileJava() {
    println "Compiling Java"
    javac("src/main/java")
}

def processResources() {
    copy("resources", "build/resources")
    validate()
}

def buildApp() {
    compileJava()
    processResources()
    println "Build complete"
}
"#;
        let tree = parse_source(source, Lang::Gradle);
        let path = PathBuf::from("/test/build.gradle");
        let summary = extract(&path, source, &tree, Lang::Gradle).unwrap();

        // Should have 3 Gradle functions
        assert_eq!(summary.symbols.len(), 3, "Should have 3 Gradle functions");

        // buildApp should have calls to compileJava and processResources
        let build_app = summary.symbols.iter().find(|s| s.name == "buildApp");
        assert!(build_app.is_some(), "Should find buildApp function");
        let build_app = build_app.unwrap();
        assert!(
            !build_app.calls.is_empty(),
            "buildApp should have calls attributed to it"
        );

        let call_names: Vec<&str> = build_app.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            call_names.contains(&"compileJava"),
            "buildApp should call compileJava"
        );
        assert!(
            call_names.contains(&"processResources"),
            "buildApp should call processResources"
        );
    }
}

#[cfg(test)]
mod debug_const_tests {
    use tree_sitter::Parser;
    
    fn print_node(node: tree_sitter::Node, source: &str, indent: usize) {
        let indent_str = "  ".repeat(indent);
        let text = if node.child_count() == 0 && node.byte_range().len() < 50 {
            format!(" = {:?}", &source[node.byte_range()])
        } else {
            String::new()
        };
        
        println!("{}{}{}", indent_str, node.kind(), text);
        
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if let Some(field_name) = node.field_name_for_child(i as u32) {
                    println!("{}  [field: {}]", indent_str, field_name);
                }
                print_node(child, source, indent + 1);
            }
        }
    }

    #[test]
    fn debug_rust_const_tree() {
        let source = r#"pub const SCHEMA_VERSION: &str = "2.1";
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
static GLOBAL_COUNTER: u64 = 0;
fn main() {}
"#;

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        
        println!("\n=== TREE-SITTER OUTPUT FOR RUST CONSTS ===\n");
        print_node(tree.root_node(), source, 0);
        println!("\n===========================================\n");
    }

    #[test]
    fn debug_const_extraction_flow() {
        use super::*;
        use crate::detectors::grammar::RUST_GRAMMAR;

        let source = r#"pub const SCHEMA_VERSION: &str = "2.1";
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
static GLOBAL_COUNTER: u64 = 0;
fn main() {}
"#;

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        println!("\n=== CONST EXTRACTION DEBUG ===\n");

        let grammar = &RUST_GRAMMAR;
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            let kind = child.kind();
            println!("Node kind: {}", kind);
            println!("  - Is in module_var_nodes? {}", grammar.module_var_nodes.contains(&kind));
            println!("  - Is in function_nodes? {}", grammar.function_nodes.contains(&kind));

            if grammar.module_var_nodes.contains(&kind) {
                println!("  - Checking is_in_local_scope...");
                let in_local = is_in_local_scope(&child, grammar);
                println!("  - is_in_local_scope: {}", in_local);

                if !in_local {
                    println!("  - Attempting to extract name...");

                    // Try the configured name field first
                    if let Some(name_node) = child.child_by_field_name(grammar.name_field) {
                        println!("  - Found name field node: kind={}, text={:?}",
                            name_node.kind(),
                            &source[name_node.byte_range()]);
                    } else {
                        println!("  - name_field '{}' NOT FOUND", grammar.name_field);
                    }

                    // Full extraction attempt
                    if let Some(name) = extract_symbol_name(&child, source, grammar) {
                        println!("  - EXTRACTED NAME: {}", name);
                    } else {
                        println!("  - NAME EXTRACTION FAILED!");
                    }
                }
            }
            println!();
        }

        println!("=================================\n");
    }

    #[test]
    fn debug_full_extraction() {
        use super::*;
        use crate::detectors::grammar::RUST_GRAMMAR;
        use crate::schema::SemanticSummary;

        let source = r#"pub const SCHEMA_VERSION: &str = "2.1";
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
static GLOBAL_COUNTER: u64 = 0;
fn main() {}
pub struct Foo {}
"#;

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let mut summary = SemanticSummary {
            file: "test.rs".to_string(),
            language: "rust".to_string(),
            ..Default::default()
        };

        println!("\n=== FULL EXTRACTION DEBUG ===\n");

        // Call the actual extraction
        extract_with_grammar(&mut summary, source, &tree, &RUST_GRAMMAR).unwrap();

        println!("Symbols extracted: {}", summary.symbols.len());
        for sym in &summary.symbols {
            println!("  - {} ({:?}) lines {}-{}", sym.name, sym.kind, sym.start_line, sym.end_line);
        }

        println!("\nPrimary symbol: {:?}", summary.symbol);
        println!("Primary kind: {:?}", summary.symbol_kind);

        println!("\n=================================\n");

        // Assert we got the expected symbols
        assert!(summary.symbols.iter().any(|s| s.name == "SCHEMA_VERSION"), "SCHEMA_VERSION not found!");
        assert!(summary.symbols.iter().any(|s| s.name == "FNV_OFFSET"), "FNV_OFFSET not found!");
        assert!(summary.symbols.iter().any(|s| s.name == "GLOBAL_COUNTER"), "GLOBAL_COUNTER not found!");
        assert!(summary.symbols.iter().any(|s| s.name == "main"), "main not found!");
        assert!(summary.symbols.iter().any(|s| s.name == "Foo"), "Foo not found!");
    }

    #[test]
    fn debug_schema_rs_extraction() {
        use super::*;
        use crate::detectors::grammar::RUST_GRAMMAR;
        use crate::schema::SemanticSummary;
        use std::fs;

        // Read the actual schema.rs file
        let source = fs::read_to_string("/home/kadajett/Dev/Semfora_org/semfora-engine/src/schema.rs")
            .expect("Could not read schema.rs");

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&source, None).unwrap();

        let mut summary = SemanticSummary {
            file: "src/schema.rs".to_string(),
            language: "rust".to_string(),
            ..Default::default()
        };

        extract_with_grammar(&mut summary, &source, &tree, &RUST_GRAMMAR).unwrap();

        println!("\n=== SCHEMA.RS EXTRACTION ===\n");
        println!("Total symbols in summary.symbols: {}", summary.symbols.len());

        // Count by kind
        let mut kind_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for sym in &summary.symbols {
            *kind_counts.entry(format!("{:?}", sym.kind)).or_insert(0) += 1;
        }

        for (kind, count) in &kind_counts {
            println!("  {} x {}", kind, count);
        }

        // Print first 5 of each kind
        println!("\nSample Variable symbols:");
        for sym in summary.symbols.iter().filter(|s| matches!(s.kind, crate::schema::SymbolKind::Variable)).take(5) {
            println!("  - {} (lines {}-{})", sym.name, sym.start_line, sym.end_line);
        }

        println!("\n=================================\n");

        // Assert there are Variables
        let var_count = summary.symbols.iter().filter(|s| matches!(s.kind, crate::schema::SymbolKind::Variable)).count();
        assert!(var_count > 0, "Expected Variable symbols but found {}", var_count);
    }

    /// Test that variable references are extracted and attributed to symbols
    #[test]
    fn test_variable_reference_extraction() {
        use super::*;
        use crate::detectors::grammar::RUST_GRAMMAR;
        use crate::schema::SemanticSummary;

        let source = r#"
const MAX_SIZE: usize = 100;
const BUFFER_SIZE: usize = 1024;

fn process_data() {
    let arr = vec![0; MAX_SIZE];
    for i in 0..MAX_SIZE {
        if i < BUFFER_SIZE {
            println!("{}", i);
        }
    }
}

fn another_function() {
    let x = MAX_SIZE * 2;
}
"#;

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let mut summary = SemanticSummary {
            file: "test.rs".to_string(),
            language: "rust".to_string(),
            ..Default::default()
        };

        extract_with_grammar(&mut summary, source, &tree, &RUST_GRAMMAR).unwrap();

        // Find the process_data function
        let process_data = summary.symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data.is_some(), "Should find process_data function");
        let process_data = process_data.unwrap();

        // Should have variable references with ref_kind set
        let var_refs: Vec<_> = process_data.calls.iter().filter(|c| c.ref_kind.is_variable_ref()).collect();
        assert!(
            !var_refs.is_empty(),
            "process_data should have variable references, but found none. Calls: {:?}",
            process_data.calls
        );

        // Should reference MAX_SIZE
        let max_size_refs: Vec<_> = var_refs.iter().filter(|c| c.name == "MAX_SIZE").collect();
        assert!(
            !max_size_refs.is_empty(),
            "Should find reference to MAX_SIZE in process_data"
        );

        // Find another_function
        let another_fn = summary.symbols.iter().find(|s| s.name == "another_function");
        assert!(another_fn.is_some(), "Should find another_function");
        let another_fn = another_fn.unwrap();

        // Should also reference MAX_SIZE
        let another_var_refs: Vec<_> = another_fn.calls.iter().filter(|c| c.ref_kind.is_variable_ref()).collect();
        assert!(
            !another_var_refs.is_empty(),
            "another_function should have variable references"
        );
    }

    /// Test that local variables inside functions are NOT tracked as references
    #[test]
    fn test_local_variables_not_tracked() {
        use super::*;
        use crate::detectors::grammar::RUST_GRAMMAR;
        use crate::schema::SemanticSummary;

        let source = r#"
fn process() {
    let local_var = 42;
    let x = local_var + 1;
    println!("{}", x);
}
"#;

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let mut summary = SemanticSummary {
            file: "test.rs".to_string(),
            language: "rust".to_string(),
            ..Default::default()
        };

        extract_with_grammar(&mut summary, source, &tree, &RUST_GRAMMAR).unwrap();

        // Find the process function
        let process_fn = summary.symbols.iter().find(|s| s.name == "process");
        assert!(process_fn.is_some(), "Should find process function");
        let process_fn = process_fn.unwrap();

        // Should NOT have variable references (local_var is local, not module-level)
        let var_refs: Vec<_> = process_fn.calls.iter().filter(|c| c.ref_kind.is_variable_ref()).collect();
        let local_var_refs: Vec<_> = var_refs.iter().filter(|c| c.name == "local_var").collect();
        assert!(
            local_var_refs.is_empty(),
            "Should NOT track references to local variables, but found: {:?}",
            local_var_refs
        );
    }
}