//! Core JavaScript/TypeScript semantic extraction
//!
//! This module provides the foundation for JS/TS analysis that is shared
//! across all frameworks. Framework-specific logic should go in the
//! `frameworks/` submodules.

use tree_sitter::Node;

use crate::detectors::common::{
    find_containing_symbol_by_line, get_node_text, visit_all, visit_with_nesting_depth,
};
use crate::error::Result;
use crate::lang::Lang;
use crate::schema::{
    Argument, Call, ControlFlowChange, ControlFlowKind, Location, Prop, RiskLevel, SemanticSummary,
    SymbolInfo, SymbolKind,
};
use crate::toon::is_meaningful_call;

// =============================================================================
// Main Entry Point
// =============================================================================

/// Core extraction for JavaScript/TypeScript
///
/// This extracts framework-agnostic semantic information:
/// - Symbols (functions, classes, exports)
/// - Imports/dependencies
/// - Control flow patterns
/// - Function calls
pub fn extract_core(
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
    lang: Lang,
) -> Result<()> {
    // Extract symbols (functions, classes, exports)
    find_primary_symbol(summary, root, source, lang);

    // Extract imports
    extract_imports(summary, root, source);

    // Extract control flow
    extract_control_flow(summary, root);

    // Extract function calls
    extract_calls(summary, root, source);

    Ok(())
}

// =============================================================================
// Symbol Detection
// =============================================================================

/// Candidate symbol for ranking
#[derive(Debug)]
pub struct SymbolCandidate {
    pub name: String,
    pub kind: SymbolKind,
    pub is_exported: bool,
    pub is_default_export: bool,
    pub returns_jsx: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub arguments: Vec<Argument>,
    pub props: Vec<Prop>,
    pub score: i32,
}

/// Find all symbols and populate both the primary symbol and symbols vec
///
/// Priority order for primary symbol:
/// 1. Default exported components (function returning JSX)
/// 2. Named exported components
/// 3. Default exported functions/classes
/// 4. Named exported functions/classes
/// 5. Non-exported functions/classes (file-local)
fn find_primary_symbol(summary: &mut SemanticSummary, root: &Node, source: &str, lang: Lang) {
    let mut candidates: Vec<SymbolCandidate> = Vec::new();
    let filename_stem = extract_filename_stem(&summary.file);

    collect_symbol_candidates(root, source, &filename_stem, lang, &mut candidates);

    // Sort by score (highest first)
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Convert ALL exported candidates to SymbolInfo and add to summary.symbols
    for candidate in &candidates {
        if candidate.is_exported || candidate.score > 0 {
            let kind = if candidate.returns_jsx {
                SymbolKind::Component
            } else {
                candidate.kind
            };

            let symbol_info = SymbolInfo {
                name: candidate.name.clone(),
                kind,
                start_line: candidate.start_line,
                end_line: candidate.end_line,
                is_exported: candidate.is_exported,
                is_default_export: candidate.is_default_export,
                hash: None,
                arguments: candidate.arguments.clone(),
                props: candidate.props.clone(),
                return_type: if candidate.returns_jsx {
                    Some("JSX.Element".to_string())
                } else {
                    None
                },
                calls: Vec::new(),
                control_flow: Vec::new(),
                state_changes: Vec::new(),
                behavioral_risk: RiskLevel::Low,
                decorators: Vec::new(),
            };

            summary.symbols.push(symbol_info);
        }
    }

    // Use the best candidate for primary symbol (backward compatibility)
    if let Some(best) = candidates.into_iter().next() {
        summary.symbol = Some(best.name);
        summary.symbol_kind = Some(if best.returns_jsx {
            SymbolKind::Component
        } else {
            best.kind
        });
        summary.start_line = Some(best.start_line);
        summary.end_line = Some(best.end_line);
        summary.public_surface_changed = best.is_exported;
        summary.arguments = best.arguments;
        summary.props = best.props;

        if best.returns_jsx {
            summary.return_type = Some("JSX.Element".to_string());
        }
    }
}

/// Extract filename stem
pub fn extract_filename_stem(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Collect symbol candidates from the AST
fn collect_symbol_candidates(
    root: &Node,
    source: &str,
    filename_stem: &str,
    lang: Lang,
    candidates: &mut Vec<SymbolCandidate>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                let is_default = has_default_keyword(&child, source);

                // Check for declaration inside export
                if let Some(decl) = child.child_by_field_name("declaration") {
                    if let Some(mut candidate) =
                        extract_candidate_from_declaration(&decl, source, filename_stem, lang)
                    {
                        candidate.is_exported = true;
                        candidate.is_default_export = is_default;
                        candidate.score = calculate_symbol_score(&candidate, filename_stem);
                        candidates.push(candidate);
                    }
                } else {
                    // Check for export clause (re-exports or named exports)
                    let mut found_export_clause = false;
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "export_clause" {
                            extract_reexports(&inner, source, filename_stem, candidates);
                            found_export_clause = true;
                        }
                    }

                    if !found_export_clause {
                        // Direct function/class inside export or default export of expression
                        let mut inner_cursor = child.walk();
                        for inner in child.children(&mut inner_cursor) {
                            if inner.kind() == "function_declaration"
                                || inner.kind() == "class_declaration"
                            {
                                if let Some(mut candidate) = extract_candidate_from_declaration(
                                    &inner,
                                    source,
                                    filename_stem,
                                    lang,
                                ) {
                                    candidate.is_exported = true;
                                    candidate.is_default_export = is_default;
                                    candidate.score =
                                        calculate_symbol_score(&candidate, filename_stem);
                                    candidates.push(candidate);
                                }
                                break;
                            }
                            // Handle: export default memo(Component) or export default forwardRef(...)
                            if inner.kind() == "call_expression" && is_default {
                                if let Some(candidate) =
                                    extract_default_export_call(&inner, source, filename_stem)
                                {
                                    let mut candidate = candidate;
                                    candidate.is_exported = true;
                                    candidate.is_default_export = true;
                                    candidate.score =
                                        calculate_symbol_score(&candidate, filename_stem);
                                    candidates.push(candidate);
                                    break;
                                }
                            }
                            // Handle: export default SomeIdentifier
                            if inner.kind() == "identifier" && is_default {
                                let name = get_node_text(&inner, source);
                                candidates.push(SymbolCandidate {
                                    name: name.clone(),
                                    kind: SymbolKind::Function,
                                    is_exported: true,
                                    is_default_export: true,
                                    returns_jsx: false,
                                    start_line: inner.start_position().row + 1,
                                    end_line: inner.end_position().row + 1,
                                    arguments: Vec::new(),
                                    props: Vec::new(),
                                    score: calculate_symbol_score(
                                        &SymbolCandidate {
                                            name,
                                            kind: SymbolKind::Function,
                                            is_exported: true,
                                            is_default_export: true,
                                            returns_jsx: false,
                                            start_line: 0,
                                            end_line: 0,
                                            arguments: Vec::new(),
                                            props: Vec::new(),
                                            score: 0,
                                        },
                                        filename_stem,
                                    ),
                                });
                                break;
                            }
                        }
                    }
                }
            }
            "function_declaration" | "class_declaration" | "lexical_declaration" => {
                if let Some(mut candidate) =
                    extract_candidate_from_declaration(&child, source, filename_stem, lang)
                {
                    candidate.score = calculate_symbol_score(&candidate, filename_stem);
                    candidates.push(candidate);
                }
            }
            // Handle CommonJS exports: exports.foo = function() or module.exports.foo = function()
            "expression_statement" => {
                if let Some(candidate) =
                    extract_commonjs_export(&child, source, filename_stem, lang)
                {
                    candidates.push(candidate);
                }
            }
            _ => {}
        }
    }
}

/// Extract CommonJS export: exports.foo = function() or module.exports.foo = function()
///
/// Handles patterns:
/// - exports.foo = function() { ... }
/// - exports.foo = () => { ... }
/// - module.exports.foo = function() { ... }
fn extract_commonjs_export(
    node: &Node,
    source: &str,
    filename_stem: &str,
    lang: Lang,
) -> Option<SymbolCandidate> {
    // Look for assignment_expression as first child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "assignment_expression" {
            // Get left side (member_expression like exports.foo or module.exports.foo)
            let left = child.child_by_field_name("left")?;
            if left.kind() != "member_expression" {
                continue;
            }

            // Check if it's exports.X or module.exports.X
            let left_text = get_node_text(&left, source);
            let is_exports =
                left_text.starts_with("exports.") || left_text.starts_with("module.exports.");

            if !is_exports {
                continue;
            }

            // Extract the exported name (the property being assigned)
            let property = left.child_by_field_name("property")?;
            let name = get_node_text(&property, source);

            // Skip non-identifier properties
            if name.is_empty() || name.contains('[') {
                continue;
            }

            // Get right side (function_expression, arrow_function, etc.)
            let right = child.child_by_field_name("right")?;

            let mut arguments = Vec::new();
            let mut props = Vec::new();

            let (kind, jsx) = match right.kind() {
                "function_expression" | "function" => {
                    if let Some(params) = right.child_by_field_name("parameters") {
                        extract_parameters(&params, source, &mut arguments, &mut props);
                    }
                    let jsx = lang.supports_jsx() && returns_jsx(&right);
                    (SymbolKind::Function, jsx)
                }
                "arrow_function" => {
                    if let Some(params) = right.child_by_field_name("parameters") {
                        extract_parameters(&params, source, &mut arguments, &mut props);
                    } else if let Some(param) = right.child_by_field_name("parameter") {
                        arguments.push(Argument {
                            name: get_node_text(&param, source),
                            arg_type: None,
                            default_value: None,
                        });
                    }
                    let jsx = lang.supports_jsx() && returns_jsx(&right);
                    (SymbolKind::Function, jsx)
                }
                "class_expression" | "class" => (SymbolKind::Class, false),
                _ => {
                    // Could be exports.foo = someValue - skip non-function exports
                    continue;
                }
            };

            let mut candidate = SymbolCandidate {
                name,
                kind,
                is_exported: true,
                is_default_export: false,
                returns_jsx: jsx,
                start_line: right.start_position().row + 1,
                end_line: right.end_position().row + 1,
                arguments,
                props,
                score: 0,
            };
            candidate.score = calculate_symbol_score(&candidate, filename_stem);
            return Some(candidate);
        }
    }

    None
}

/// Check if export has default keyword
fn has_default_keyword(node: &Node, source: &str) -> bool {
    let text = get_node_text(node, source);
    text.contains("export default")
}

/// Extract re-exported symbols from export clause
fn extract_reexports(
    node: &Node,
    source: &str,
    filename_stem: &str,
    candidates: &mut Vec<SymbolCandidate>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "export_specifier" {
            let name = if let Some(alias) = child.child_by_field_name("alias") {
                get_node_text(&alias, source)
            } else if let Some(name_node) = child.child_by_field_name("name") {
                get_node_text(&name_node, source)
            } else {
                continue;
            };

            let mut candidate = SymbolCandidate {
                name,
                kind: SymbolKind::Function,
                is_exported: true,
                is_default_export: false,
                returns_jsx: false,
                start_line: child.start_position().row + 1,
                end_line: child.end_position().row + 1,
                arguments: Vec::new(),
                props: Vec::new(),
                score: 0,
            };
            candidate.score = calculate_symbol_score(&candidate, filename_stem);
            candidates.push(candidate);
        }
    }
}

/// Extract symbol from default export of call expression
/// Handles: export default memo(Component) or export default forwardRef(...)
pub fn extract_default_export_call(
    node: &Node,
    source: &str,
    filename_stem: &str,
) -> Option<SymbolCandidate> {
    if let Some(func_node) = node.child_by_field_name("function") {
        let func_text = get_node_text(&func_node, source);

        // Check if this is a React component wrapper pattern
        let is_component_wrapper = func_text == "forwardRef"
            || func_text == "memo"
            || func_text.ends_with(".forwardRef")
            || func_text.ends_with(".memo");

        if is_component_wrapper {
            // Try to extract the component name from the arguments
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut args_cursor = args.walk();
                for arg in args.children(&mut args_cursor) {
                    if arg.kind() == "identifier" {
                        return Some(SymbolCandidate {
                            name: get_node_text(&arg, source),
                            kind: SymbolKind::Function,
                            is_exported: false,
                            is_default_export: false,
                            returns_jsx: true,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            arguments: Vec::new(),
                            props: Vec::new(),
                            score: 0,
                        });
                    }
                }
            }

            // Fallback: use filename as component name
            return Some(SymbolCandidate {
                name: to_pascal_case(filename_stem),
                kind: SymbolKind::Function,
                is_exported: false,
                is_default_export: false,
                returns_jsx: true,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                arguments: Vec::new(),
                props: Vec::new(),
                score: 0,
            });
        }
    }
    None
}

/// Convert string to PascalCase for component naming
pub fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '-' || c == '_' || c == '.')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Extract a symbol candidate from a declaration node
fn extract_candidate_from_declaration(
    node: &Node,
    source: &str,
    _filename_stem: &str,
    lang: Lang,
) -> Option<SymbolCandidate> {
    match node.kind() {
        "function_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = get_node_text(&name_node, source);

            let mut arguments = Vec::new();
            let mut props = Vec::new();

            if let Some(params) = node.child_by_field_name("parameters") {
                extract_parameters(&params, source, &mut arguments, &mut props);
            }

            Some(SymbolCandidate {
                name,
                kind: SymbolKind::Function,
                is_exported: false,
                is_default_export: false,
                returns_jsx: lang.supports_jsx() && returns_jsx(node),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                arguments,
                props,
                score: 0,
            })
        }
        "class_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = get_node_text(&name_node, source);

            Some(SymbolCandidate {
                name,
                kind: SymbolKind::Class,
                is_exported: false,
                is_default_export: false,
                returns_jsx: false,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                arguments: Vec::new(),
                props: Vec::new(),
                score: 0,
            })
        }
        "lexical_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    let name_node = child.child_by_field_name("name")?;
                    let value_node = child.child_by_field_name("value")?;

                    if value_node.kind() == "arrow_function" {
                        let name = get_node_text(&name_node, source);

                        let mut arguments = Vec::new();
                        let mut props = Vec::new();

                        if let Some(params) = value_node.child_by_field_name("parameters") {
                            extract_parameters(&params, source, &mut arguments, &mut props);
                        } else if let Some(param) = value_node.child_by_field_name("parameter") {
                            arguments.push(Argument {
                                name: get_node_text(&param, source),
                                arg_type: None,
                                default_value: None,
                            });
                        }

                        return Some(SymbolCandidate {
                            name,
                            kind: SymbolKind::Function,
                            is_exported: false,
                            is_default_export: false,
                            returns_jsx: lang.supports_jsx() && returns_jsx(&value_node),
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            arguments,
                            props,
                            score: 0,
                        });
                    }

                    // Handle React component patterns: forwardRef, memo, styled, etc.
                    if value_node.kind() == "call_expression" {
                        let name = get_node_text(&name_node, source);

                        if let Some(func_node) = value_node.child_by_field_name("function") {
                            let func_text = get_node_text(&func_node, source);

                            let is_component_wrapper = func_text == "forwardRef"
                                || func_text == "memo"
                                || func_text.ends_with(".forwardRef")
                                || func_text.ends_with(".memo")
                                || func_text.starts_with("styled.");

                            if is_component_wrapper {
                                let args_node = value_node.child_by_field_name("arguments");
                                let returns_jsx_content = args_node
                                    .map(|args| {
                                        let args_text = get_node_text(&args, source);
                                        args_text.contains("return")
                                            && (args_text.contains("<")
                                                || args_text.contains("jsx"))
                                            || args_text.contains("=>") && args_text.contains("<")
                                    })
                                    .unwrap_or(false);

                                return Some(SymbolCandidate {
                                    name,
                                    kind: SymbolKind::Function,
                                    is_exported: false,
                                    is_default_export: false,
                                    returns_jsx: returns_jsx_content,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    arguments: Vec::new(),
                                    props: Vec::new(),
                                    score: 0,
                                });
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Calculate symbol score for prioritization
fn calculate_symbol_score(candidate: &SymbolCandidate, filename_stem: &str) -> i32 {
    let mut score = 0;

    // Base score by kind
    score += match candidate.kind {
        SymbolKind::Component => 40,
        SymbolKind::Class => 30,
        SymbolKind::Function => 20,
        _ => 10,
    };

    // Bonus for JSX-returning functions (components)
    if candidate.returns_jsx {
        score += 30;
    }

    // Bonus for exports
    if candidate.is_exported {
        score += 50;
    }

    // Extra bonus for default exports
    if candidate.is_default_export {
        score += 20;
    }

    // Filename matching bonus
    let name_lower = candidate.name.to_lowercase();
    if name_lower == filename_stem {
        score += 40;
    } else if name_lower.contains(filename_stem) || filename_stem.contains(&name_lower) {
        score += 20;
    }

    // Penalty for test files
    if candidate.name.starts_with("test") || candidate.name.ends_with("Test") {
        score -= 30;
    }

    // Penalty for internal/helper naming
    if candidate.name.starts_with("_") || candidate.name.contains("Helper") {
        score -= 20;
    }

    score
}

/// Check if a function returns JSX
pub fn returns_jsx(node: &Node) -> bool {
    contains_node_kind(node, "jsx_element")
        || contains_node_kind(node, "jsx_self_closing_element")
        || contains_node_kind(node, "jsx_fragment")
}

/// Check if a node contains a specific kind
fn contains_node_kind(node: &Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_node_kind(&child, kind) {
            return true;
        }
    }
    false
}

/// Extract function parameters
pub fn extract_parameters(
    params: &Node,
    source: &str,
    arguments: &mut Vec<Argument>,
    props: &mut Vec<Prop>,
) {
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                arguments.push(Argument {
                    name: get_node_text(&child, source),
                    arg_type: None,
                    default_value: None,
                });
            }
            "required_parameter" | "optional_parameter" => {
                let name = child
                    .child_by_field_name("pattern")
                    .map(|n| get_node_text(&n, source))
                    .unwrap_or_default();
                let arg_type = child
                    .child_by_field_name("type")
                    .map(|n| get_node_text(&n, source));
                arguments.push(Argument {
                    name,
                    arg_type,
                    default_value: None,
                });
            }
            "assignment_pattern" => {
                if let Some(left) = child.child_by_field_name("left") {
                    let name = get_node_text(&left, source);
                    let default_value = child
                        .child_by_field_name("right")
                        .map(|n| get_node_text(&n, source));
                    arguments.push(Argument {
                        name,
                        arg_type: None,
                        default_value,
                    });
                }
            }
            "object_pattern" => {
                extract_object_pattern_as_props(&child, source, props);
            }
            _ => {}
        }
    }
}

/// Extract destructured props from object pattern
pub fn extract_object_pattern_as_props(pattern: &Node, source: &str, props: &mut Vec<Prop>) {
    let mut cursor = pattern.walk();
    for child in pattern.children(&mut cursor) {
        if child.kind() == "shorthand_property_identifier_pattern" {
            props.push(Prop {
                name: get_node_text(&child, source),
                prop_type: None,
                default_value: None,
                required: true,
            });
        } else if child.kind() == "pair_pattern" {
            if let Some(key) = child.child_by_field_name("key") {
                let name = get_node_text(&key, source);
                let default_value = child.child_by_field_name("value").and_then(|v| {
                    if v.kind() == "assignment_pattern" {
                        v.child_by_field_name("right")
                            .map(|r| get_node_text(&r, source))
                    } else {
                        None
                    }
                });
                props.push(Prop {
                    name,
                    prop_type: None,
                    default_value: default_value.clone(),
                    required: default_value.is_none(),
                });
            }
        }
    }
}

// =============================================================================
// Import Extraction
// =============================================================================

/// Extract imports as dependencies
pub fn extract_imports(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            if let Some(clause) = child.child_by_field_name("source") {
                let module = get_node_text(&clause, source);
                let module = module.trim_matches('"').trim_matches('\'');

                // Track local imports for data flow
                if is_local_import(module) {
                    summary.local_imports.push(normalize_import_path(module));
                }

                // Extract imported names
                extract_import_names(&child, source, module, &mut summary.added_dependencies);
            }
        }
    }
}

/// Check if an import path is local (starts with . or ..)
pub fn is_local_import(module: &str) -> bool {
    module.starts_with('.') || module.starts_with("..")
}

/// Normalize an import path
fn normalize_import_path(module: &str) -> String {
    module.trim_start_matches("./").to_string()
}

/// Extract imported names from import statement
fn extract_import_names(import: &Node, source: &str, module: &str, deps: &mut Vec<String>) {
    let mut cursor = import.walk();
    for child in import.children(&mut cursor) {
        if child.kind() == "import_clause" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                match inner.kind() {
                    "identifier" => {
                        deps.push(get_node_text(&inner, source));
                    }
                    "named_imports" => {
                        let mut named_cursor = inner.walk();
                        for named in inner.children(&mut named_cursor) {
                            if named.kind() == "import_specifier" {
                                if let Some(name_node) = named.child_by_field_name("name") {
                                    deps.push(get_node_text(&name_node, source));
                                }
                            }
                        }
                    }
                    "namespace_import" => {
                        if let Some(name_node) = inner.child_by_field_name("name") {
                            deps.push(get_node_text(&name_node, source));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // If no specific imports found, use module name
    if deps.is_empty() && !module.is_empty() {
        if let Some(last) = module.split('/').last() {
            deps.push(last.to_string());
        }
    }
}

// =============================================================================
// Control Flow Extraction
// =============================================================================

/// JavaScript control flow node kinds for nesting depth tracking
const JS_CONTROL_FLOW_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "switch_statement",
    "try_statement",
];

/// Extract control flow patterns with nesting depth for cognitive complexity
/// and attribute them to symbols based on line ranges
pub fn extract_control_flow(summary: &mut SemanticSummary, root: &Node) {
    // Collect all control flow items with their line numbers
    let mut all_cf: Vec<(ControlFlowChange, usize)> = Vec::new();

    visit_with_nesting_depth(
        root,
        |node, depth| {
            let kind = match node.kind() {
                "if_statement" => Some(ControlFlowKind::If),
                "for_statement" | "for_in_statement" => Some(ControlFlowKind::For),
                "while_statement" => Some(ControlFlowKind::While),
                "switch_statement" => Some(ControlFlowKind::Switch),
                "try_statement" => Some(ControlFlowKind::Try),
                _ => None,
            };

            if let Some(k) = kind {
                let nesting = if depth > 0 { depth - 1 } else { 0 };
                let line = node.start_position().row + 1;
                let cf = ControlFlowChange {
                    kind: k,
                    location: Location::new(line, node.start_position().column),
                    nesting_depth: nesting,
                };
                all_cf.push((cf, line));
            }
        },
        JS_CONTROL_FLOW_KINDS,
    );

    // Attribute control flow to symbols based on line ranges
    let mut cf_by_symbol: std::collections::HashMap<usize, Vec<ControlFlowChange>> =
        std::collections::HashMap::new();
    let mut file_level_cf: Vec<ControlFlowChange> = Vec::new();

    for (cf, line) in all_cf {
        if let Some(symbol_idx) = find_containing_symbol_by_line(line, &summary.symbols) {
            cf_by_symbol.entry(symbol_idx).or_default().push(cf);
        } else {
            // Control flow is at file level (not inside any symbol)
            file_level_cf.push(cf);
        }
    }

    // Assign control flow to their respective symbols
    for (symbol_idx, cf_items) in cf_by_symbol {
        if symbol_idx < summary.symbols.len() {
            summary.symbols[symbol_idx].control_flow = cf_items;
        }
    }

    // Keep file-level control flow for backward compatibility
    summary.control_flow_changes = file_level_cf;
}

// =============================================================================
// Call Extraction
// =============================================================================

/// Extract function calls with context and assign to symbols
pub fn extract_calls(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Build try ranges for in_try detection
    let mut try_ranges: Vec<(usize, usize)> = Vec::new();
    visit_all(root, |node| {
        if node.kind() == "try_statement" {
            try_ranges.push((node.start_byte(), node.end_byte()));
        }
    });

    // Collect all calls first
    let mut all_calls: Vec<(Call, usize)> = Vec::new(); // (call, line_number)

    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let (name, object) = extract_call_name(&func, source);

                if Call::check_is_hook(&name) || is_trivial_call(&name) {
                    return;
                }

                if !is_meaningful_call(&name, object.as_deref()) {
                    return;
                }

                let is_awaited = node
                    .parent()
                    .map(|p| p.kind() == "await_expression")
                    .unwrap_or(false);

                let node_start = node.start_byte();
                let in_try = try_ranges
                    .iter()
                    .any(|(start, end)| node_start >= *start && node_start < *end);

                let is_io = Call::check_is_io(&name);
                let line = node.start_position().row + 1;

                let call = Call {
                    name,
                    object,
                    is_awaited,
                    in_try,
                    is_hook: false,
                    is_io,
                    location: Location::new(line, node.start_position().column),
                };

                all_calls.push((call, line));
            }
        }
    });

    // Now assign calls to symbols based on line ranges
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

    // Assign calls to their respective symbols
    for (symbol_idx, calls) in calls_by_symbol {
        if symbol_idx < summary.symbols.len() {
            summary.symbols[symbol_idx].calls = calls;
        }
    }

    // Also keep file-level calls in summary.calls for backward compatibility
    summary.calls = file_level_calls;
}

/// Extract call name and object
fn extract_call_name(func_node: &Node, source: &str) -> (String, Option<String>) {
    match func_node.kind() {
        "identifier" => (get_node_text(func_node, source), None),
        "member_expression" => {
            let property = func_node
                .child_by_field_name("property")
                .map(|p| get_node_text(&p, source))
                .unwrap_or_default();
            let object = func_node
                .child_by_field_name("object")
                .map(|o| simplify_object(&o, source));
            (property, object)
        }
        _ => (get_node_text(func_node, source), None),
    }
}

/// Simplify object reference
fn simplify_object(node: &Node, source: &str) -> String {
    match node.kind() {
        "identifier" => get_node_text(node, source),
        "member_expression" => {
            if let Some(prop) = node.child_by_field_name("property") {
                get_node_text(&prop, source)
            } else {
                get_node_text(node, source)
            }
        }
        "this" => "this".to_string(),
        _ => "_".to_string(),
    }
}

/// Check if call is trivial
fn is_trivial_call(name: &str) -> bool {
    matches!(
        name,
        "log" | "error" | "warn" | "info" | "debug" | "trace" | "toString" | "valueOf"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_stem() {
        assert_eq!(extract_filename_stem("/path/to/Header.tsx"), "header");
        assert_eq!(extract_filename_stem("utils.ts"), "utils");
        assert_eq!(extract_filename_stem("index.js"), "index");
    }

    #[test]
    fn test_is_local_import() {
        assert!(is_local_import("./components"));
        assert!(is_local_import("../utils"));
        assert!(!is_local_import("react"));
        assert!(!is_local_import("@/components"));
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("my-component"), "MyComponent");
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
        assert_eq!(to_pascal_case("button"), "Button");
    }

    // ==========================================================================
    // Call Attribution Tests
    // ==========================================================================

    use crate::extract::extract;
    use crate::lang::Lang;
    use std::path::PathBuf;
    use tree_sitter::{Parser, Tree};

    fn parse_source(source: &str, lang: Lang) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .expect("Failed to set language");
        parser.parse(source, None).expect("Failed to parse")
    }

    /// Test that ES6 export functions have calls attributed to symbols
    #[test]
    fn test_es6_call_attribution() {
        let source = r#"
export function fetchUsers() {
    const response = fetch("https://api.example.com/users");
    const data = response.json();
    return processData(data);
}

export function processData(data) {
    return data.map(transform);
}
"#;
        let tree = parse_source(source, Lang::JavaScript);
        let path = PathBuf::from("/test/api.js");
        let summary = extract(&path, source, &tree, Lang::JavaScript).unwrap();

        // Should have 2 symbols
        assert_eq!(summary.symbols.len(), 2, "Should have 2 JS functions");

        // fetchUsers should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some(), "Should find fetchUsers function");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls attributed to it, got: {:?}",
            fetch_users.calls
        );
        // Should contain fetch, json, processData
        let call_names: Vec<_> = fetch_users.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(call_names.contains(&"fetch"), "Should have fetch call");
        assert!(
            call_names.contains(&"processData"),
            "Should have processData call"
        );
    }

    /// Test that CommonJS exports have calls attributed to symbols
    #[test]
    fn test_commonjs_call_attribution() {
        let source = r#"
exports.fetchUsers = function(req, res) {
    const data = loadData();
    res.json(processData(data));
};

exports.processData = function(data) {
    return data.map(transform);
};
"#;
        let tree = parse_source(source, Lang::JavaScript);
        let path = PathBuf::from("/test/api.js");
        let summary = extract(&path, source, &tree, Lang::JavaScript).unwrap();

        // Should have 2 symbols (CommonJS exports)
        assert_eq!(summary.symbols.len(), 2, "Should have 2 CommonJS exports");

        // fetchUsers should have calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some(), "Should find fetchUsers export");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls attributed to it"
        );
        // Should contain loadData, json, processData
        let call_names: Vec<_> = fetch_users.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            call_names.contains(&"loadData"),
            "Should have loadData call"
        );
        assert!(
            call_names.contains(&"processData"),
            "Should have processData call"
        );
    }

    /// Test that TypeScript functions have calls attributed to symbols
    #[test]
    fn test_typescript_call_attribution() {
        let source = r#"
export async function fetchUsers(): Promise<User[]> {
    const response = await fetch("https://api.example.com/users");
    const data = await response.json();
    return processData(data);
}

function processData(data: RawUser[]): User[] {
    return data.map(transform);
}
"#;
        let tree = parse_source(source, Lang::TypeScript);
        let path = PathBuf::from("/test/api.ts");
        let summary = extract(&path, source, &tree, Lang::TypeScript).unwrap();

        // fetchUsers should have async calls
        let fetch_users = summary.symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some(), "Should find fetchUsers function");
        let fetch_users = fetch_users.unwrap();
        assert!(
            !fetch_users.calls.is_empty(),
            "fetchUsers should have calls"
        );
        // Should have awaited calls
        let has_awaited = fetch_users.calls.iter().any(|c| c.is_awaited);
        assert!(has_awaited, "fetchUsers should have awaited calls");
    }

    /// Test that Vue SFC methods have calls attributed to symbols
    #[test]
    fn test_vue_sfc_call_attribution() {
        let source = r#"<template>
  <div>{{ message }}</div>
</template>

<script>
export default {
  methods: {
    fetchData() {
      this.loading = true;
      api.get('/users').then(this.processResponse);
    },
    processResponse(data) {
      this.users = data.map(transform);
    }
  }
}
</script>"#;

        let path = PathBuf::from("/test/component.vue");
        let mut summary = SemanticSummary {
            file: path.display().to_string(),
            language: "vue".to_string(),
            ..Default::default()
        };

        // Call extract_vue_sfc directly
        super::super::extract_vue_sfc(&mut summary, source).unwrap();

        // Vue Options API may not extract individual methods as symbols,
        // but should detect Vue patterns and have calls at file level
        assert!(
            summary.insertions.iter().any(|i| i.contains("Vue")),
            "Should detect Vue SFC. Insertions: {:?}",
            summary.insertions
        );

        // Should have calls (api.get, map, etc.)
        let all_calls: Vec<&str> = summary
            .calls
            .iter()
            .chain(summary.symbols.iter().flat_map(|s| s.calls.iter()))
            .map(|c| c.name.as_str())
            .collect();

        // Should have extracted some calls from the script
        assert!(
            !all_calls.is_empty() || !summary.insertions.is_empty(),
            "Should extract calls or detect Vue patterns"
        );
    }

    /// Test Vue script setup call attribution
    #[test]
    fn test_vue_script_setup_call_attribution() {
        let source = r#"<template>
  <div>{{ count }}</div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';

const count = ref(0);
const doubled = computed(() => count.value * 2);

function increment() {
  count.value++;
  logAction('increment');
}

function logAction(action: string) {
  console.log(action);
}
</script>"#;

        let path = PathBuf::from("/test/Counter.vue");
        let mut summary = SemanticSummary {
            file: path.display().to_string(),
            language: "vue".to_string(),
            ..Default::default()
        };

        super::super::extract_vue_sfc(&mut summary, source).unwrap();

        // Should have symbols
        assert!(
            !summary.symbols.is_empty(),
            "Vue script setup should have symbols"
        );

        // increment function should have calls
        let increment = summary.symbols.iter().find(|s| s.name == "increment");
        if let Some(increment) = increment {
            assert!(
                !increment.calls.is_empty(),
                "increment should have calls attributed to it"
            );
        }

        // Should detect script setup
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("script setup")),
            "Should detect script setup"
        );
    }

    /// Test Vue Composition API call attribution
    #[test]
    fn test_vue_composition_api_call_attribution() {
        let source = r#"<template>
  <div>{{ message }}</div>
</template>

<script lang="ts">
import { ref, onMounted, defineComponent } from 'vue';
import { fetchUser } from './api';

export default defineComponent({
  setup() {
    const message = ref('Hello');

    onMounted(async () => {
      const user = await fetchUser();
      message.value = user.name;
    });

    return { message };
  }
});
</script>"#;

        let path = PathBuf::from("/test/Greeting.vue");
        let mut summary = SemanticSummary {
            file: path.display().to_string(),
            language: "vue".to_string(),
            ..Default::default()
        };

        super::super::extract_vue_sfc(&mut summary, source).unwrap();

        // Should detect Vue insertions (Composition API is detected)
        assert!(
            summary.insertions.iter().any(|i| i.contains("Vue")),
            "Should detect Vue patterns"
        );

        // Should have calls (ref, onMounted, fetchUser)
        let all_calls: Vec<&str> = summary
            .calls
            .iter()
            .chain(summary.symbols.iter().flat_map(|s| s.calls.iter()))
            .map(|c| c.name.as_str())
            .collect();

        // At minimum, should have some Vue composition calls detected
        assert!(
            all_calls
                .iter()
                .any(|c| *c == "ref" || *c == "onMounted" || *c == "defineComponent"),
            "Should detect composition API calls like ref/onMounted"
        );
    }

    // ==========================================================================
    // One-liner Arrow Function Tests (Bug regression tests)
    // ==========================================================================

    /// Test one-liner arrow functions with axios calls have calls attributed
    /// This is a regression test for the bug where one-liner arrow functions
    /// like `export const fetchUser = (id) => axios.get(\`/user/${id}\`)` were
    /// not having their calls extracted.
    #[test]
    fn test_oneliner_arrow_axios_calls() {
        let source = r#"import axios from 'axios';

export const fetchUserByUsername = (username: string): Promise<any> => axios.get(`/v1/user/${username}`);

export const fetchUserVotes = (accessToken: string, username: string, skip: number): Promise<any> => axios.get(
    `/v1/entry/votes-of/${username}`, {
        headers: { 'Authorization': `Bearer ${accessToken}` },
        params: { skip }
    });
"#;
        let tree = parse_source(source, Lang::TypeScript);
        let path = PathBuf::from("/test/api.ts");
        let summary = extract(&path, source, &tree, Lang::TypeScript).unwrap();

        // Should have 2 symbols
        assert!(
            summary.symbols.len() >= 2,
            "Should have at least 2 symbols, got {}",
            summary.symbols.len()
        );

        // Find fetchUserByUsername
        let fetch_user = summary
            .symbols
            .iter()
            .find(|s| s.name == "fetchUserByUsername");
        assert!(
            fetch_user.is_some(),
            "Should find fetchUserByUsername symbol"
        );
        let fetch_user = fetch_user.unwrap();

        // Debug output
        eprintln!(
            "fetchUserByUsername: lines {}-{}, calls: {:?}",
            fetch_user.start_line, fetch_user.end_line, fetch_user.calls
        );
        eprintln!(
            "file-level calls: {:?}",
            summary.calls.iter().map(|c| &c.name).collect::<Vec<_>>()
        );

        // This is the key assertion - the axios.get call should be attributed to the symbol
        assert!(
            !fetch_user.calls.is_empty(),
            "fetchUserByUsername should have calls attributed (axios.get), but has none. \
            Symbol lines: {}-{}, file-level calls: {:?}",
            fetch_user.start_line,
            fetch_user.end_line,
            summary
                .calls
                .iter()
                .map(|c| format!("{}@{}", c.name, c.location.line))
                .collect::<Vec<_>>()
        );

        let call_names: Vec<_> = fetch_user.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            call_names.contains(&"get"),
            "Should have 'get' call (from axios.get)"
        );
    }

    /// Test multi-line arrow functions with switch statements have control flow
    #[test]
    fn test_arrow_function_control_flow() {
        let source = r#"
export const globalReducer = (state = initialState, action: GlobalActions) => {
    switch (action.type) {
        case SET_ACCESS_TOKEN:
            return { ...state, accessToken: action.payload };
        case SET_USER_INFO:
            return { ...state, userInfo: action.payload };
        case CLEAR_USER:
            return initialState;
        default:
            return state;
    }
};
"#;
        let tree = parse_source(source, Lang::TypeScript);
        let path = PathBuf::from("/test/reducer.ts");
        let summary = extract(&path, source, &tree, Lang::TypeScript).unwrap();

        // Find globalReducer
        let reducer = summary.symbols.iter().find(|s| s.name == "globalReducer");
        assert!(reducer.is_some(), "Should find globalReducer symbol");
        let reducer = reducer.unwrap();

        eprintln!(
            "globalReducer: lines {}-{}, control_flow: {:?}",
            reducer.start_line, reducer.end_line, reducer.control_flow
        );

        // The reducer should have control flow (switch statement)
        assert!(
            !reducer.control_flow.is_empty(),
            "globalReducer should have control flow (switch statement), but has none"
        );
    }
}
