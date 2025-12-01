//! Semantic extraction orchestration
//!
//! This module coordinates the extraction of semantic information from parsed
//! source files using language-specific detectors.

use std::path::Path;
use tree_sitter::Tree;

use crate::error::Result;
use crate::lang::Lang;
use crate::risk::calculate_risk;
use crate::schema::SemanticSummary;

/// Extract semantic information from a parsed source file
///
/// This is the main entry point for semantic extraction. It delegates to
/// language-specific extractors based on the detected language.
pub fn extract(file_path: &Path, source: &str, tree: &Tree, lang: Lang) -> Result<SemanticSummary> {
    let mut summary = SemanticSummary {
        file: file_path.display().to_string(),
        language: lang.name().to_string(),
        ..Default::default()
    };

    // Dispatch to language family extractor
    match lang.family() {
        crate::lang::LangFamily::JavaScript => {
            extract_javascript_family(&mut summary, source, tree, lang)?;
        }
        crate::lang::LangFamily::Rust => {
            extract_rust(&mut summary, source, tree)?;
        }
        crate::lang::LangFamily::Python => {
            extract_python(&mut summary, source, tree)?;
        }
        crate::lang::LangFamily::Go => {
            extract_go(&mut summary, source, tree)?;
        }
        crate::lang::LangFamily::Java => {
            extract_java(&mut summary, source, tree)?;
        }
        crate::lang::LangFamily::CFamily => {
            extract_c_family(&mut summary, source, tree, lang)?;
        }
        crate::lang::LangFamily::Markup => {
            extract_markup(&mut summary, source, tree, lang)?;
        }
        crate::lang::LangFamily::Config => {
            extract_config(&mut summary, source, tree, lang)?;
        }
    }

    // Reorder insertions: put state hooks last per spec
    reorder_insertions(&mut summary.insertions);

    // Calculate risk score
    summary.behavioral_risk = calculate_risk(&summary);

    // Mark extraction as complete if we got a symbol
    summary.extraction_complete = summary.symbol.is_some();

    // Add raw fallback if extraction was incomplete
    if !summary.extraction_complete {
        // Truncate source for fallback if too long
        let max_fallback_len = 1000;
        if source.len() > max_fallback_len {
            summary.raw_fallback = Some(format!("{}...", &source[..max_fallback_len]));
        } else {
            summary.raw_fallback = Some(source.to_string());
        }
    }

    Ok(summary)
}

/// Extract from JavaScript/TypeScript/JSX/TSX files
fn extract_javascript_family(
    summary: &mut SemanticSummary,
    source: &str,
    tree: &Tree,
    lang: Lang,
) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol (function, class, component)
    find_primary_symbol_js(summary, &root, source);

    // Extract imports
    extract_imports_js(summary, &root, source);

    // Extract state hooks (useState, useReducer)
    extract_state_hooks(summary, &root, source);

    // Extract JSX elements for insertion rules
    if lang.supports_jsx() {
        extract_jsx_insertions(summary, &root, source);
    }

    // Extract control flow
    extract_control_flow_js(summary, &root, source);

    // Extract function calls with context (awaited, in_try)
    extract_calls_js(summary, &root, source);

    Ok(())
}

/// Extract from Rust files
fn extract_rust(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol
    find_primary_symbol_rust(summary, &root, source);

    // Extract use statements
    extract_imports_rust(summary, &root, source);

    // Extract let bindings
    extract_state_rust(summary, &root, source);

    // Extract control flow
    extract_control_flow_rust(summary, &root, source);

    Ok(())
}

/// Extract from Python files
fn extract_python(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol
    find_primary_symbol_python(summary, &root, source);

    // Extract imports
    extract_imports_python(summary, &root, source);

    // Extract variable assignments
    extract_state_python(summary, &root, source);

    // Extract control flow
    extract_control_flow_python(summary, &root, source);

    Ok(())
}

/// Extract from Go files
fn extract_go(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol
    find_primary_symbol_go(summary, &root, source);

    // Extract imports
    extract_imports_go(summary, &root, source);

    Ok(())
}

/// Extract from Java files
fn extract_java(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol (class)
    find_primary_symbol_java(summary, &root, source);

    // Extract imports
    extract_imports_java(summary, &root, source);

    Ok(())
}

/// Extract from C/C++ files
fn extract_c_family(
    summary: &mut SemanticSummary,
    source: &str,
    tree: &Tree,
    _lang: Lang,
) -> Result<()> {
    let root = tree.root_node();

    // Find primary symbol
    find_primary_symbol_c(summary, &root, source);

    // Extract includes
    extract_includes_c(summary, &root, source);

    Ok(())
}

/// Extract from markup files (HTML, CSS, Markdown)
fn extract_markup(
    summary: &mut SemanticSummary,
    _source: &str,
    _tree: &Tree,
    _lang: Lang,
) -> Result<()> {
    // Markup files have simpler extraction - mainly structure
    // For now, just mark as complete with the file info
    summary.extraction_complete = true;
    Ok(())
}

/// Extract from config files (JSON, YAML, TOML)
fn extract_config(
    summary: &mut SemanticSummary,
    source: &str,
    tree: &Tree,
    lang: Lang,
) -> Result<()> {
    let root = tree.root_node();

    match lang {
        Lang::Json => extract_json_structure(summary, &root, source),
        Lang::Yaml => extract_yaml_structure(summary, &root, source),
        Lang::Toml => extract_toml_structure(summary, &root, source),
        _ => {}
    }

    summary.extraction_complete = true;
    Ok(())
}

/// Extract structure from JSON files - identify key fields
fn extract_json_structure(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    // Look for top-level object
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "object" {
            extract_json_keys(summary, &child, source, 0);
        }
    }
}

/// Extract keys from JSON object (only top 2 levels to keep it concise)
fn extract_json_keys(summary: &mut SemanticSummary, node: &tree_sitter::Node, source: &str, depth: usize) {
    if depth > 1 {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "pair" {
            if let Some(key) = child.child_by_field_name("key") {
                let key_str = get_node_text(&key, source);
                let key_clean = key_str.trim_matches('"');

                // Important config keys to track
                if is_meaningful_config_key(key_clean) {
                    summary.added_dependencies.push(key_clean.to_string());
                }

                // Recurse into nested objects
                if let Some(value) = child.child_by_field_name("value") {
                    if value.kind() == "object" {
                        extract_json_keys(summary, &value, source, depth + 1);
                    }
                }
            }
        }
    }
}

/// Extract structure from YAML files
fn extract_yaml_structure(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "block_mapping_pair" {
            if let Some(key) = node.child_by_field_name("key") {
                let key_str = get_node_text(&key, source);
                if is_meaningful_config_key(&key_str) {
                    summary.added_dependencies.push(key_str);
                }
            }
        }
    });
}

/// Extract structure from TOML files
fn extract_toml_structure(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "table" || node.kind() == "table_array_element" {
            // Get the table name
            let table_text = get_node_text(node, source);
            if let Some(name) = table_text.lines().next() {
                let name = name.trim_matches('[').trim_matches(']').trim();
                if !name.is_empty() {
                    summary.added_dependencies.push(name.to_string());
                }
            }
        } else if node.kind() == "pair" {
            if let Some(key) = node.child(0) {
                let key_str = get_node_text(&key, source);
                if is_meaningful_config_key(&key_str) {
                    summary.added_dependencies.push(key_str);
                }
            }
        }
    });
}

/// Check if a config key is meaningful enough to track
fn is_meaningful_config_key(key: &str) -> bool {
    // Package/project config keys
    if matches!(
        key,
        "name" | "version" | "description" | "main" | "type" | "license"
        | "scripts" | "dependencies" | "devDependencies" | "peerDependencies"
        | "engines" | "repository" | "author" | "keywords"
        // Docker/container
        | "image" | "services" | "volumes" | "ports" | "environment"
        // Database
        | "schema" | "dialect" | "dbCredentials"
        // Framework config
        | "compilerOptions" | "include" | "exclude" | "extends"
        | "plugins" | "rules" | "settings"
    ) {
        return true;
    }
    false
}

// ============================================================================
// JavaScript/TypeScript extraction helpers
// ============================================================================

fn find_primary_symbol_js(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                // Look for default export or named export
                if let Some(decl) = child.child_by_field_name("declaration") {
                    extract_symbol_from_declaration_js(summary, &decl, source);
                } else {
                    // Check for direct function/class inside export
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "function_declaration"
                            || inner.kind() == "class_declaration"
                        {
                            extract_symbol_from_declaration_js(summary, &inner, source);
                            break;
                        }
                    }
                }
                if summary.symbol.is_some() {
                    // Phase 1: Single file analysis has no "before" state to compare
                    // public_surface_changed will be set in Phase 2 diff analysis
                    return;
                }
            }
            "function_declaration" | "class_declaration" | "lexical_declaration" => {
                extract_symbol_from_declaration_js(summary, &child, source);
                if summary.symbol.is_some() {
                    return;
                }
            }
            _ => {}
        }
    }
}

fn extract_symbol_from_declaration_js(
    summary: &mut SemanticSummary,
    node: &tree_sitter::Node,
    source: &str,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                summary.symbol = Some(get_node_text(&name_node, source));
                summary.symbol_kind = Some(crate::schema::SymbolKind::Function);

                // Extract function parameters
                if let Some(params) = node.child_by_field_name("parameters") {
                    extract_js_parameters(summary, &params, source);
                }

                // Check if it returns JSX (making it a component)
                if returns_jsx(node, source) {
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Component);
                    summary.return_type = Some("JSX.Element".to_string());
                }
            }
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                summary.symbol = Some(get_node_text(&name_node, source));
                summary.symbol_kind = Some(crate::schema::SymbolKind::Class);
            }
        }
        "lexical_declaration" => {
            // Look for arrow function assigned to const
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Some(value_node) = child.child_by_field_name("value") {
                            if value_node.kind() == "arrow_function" {
                                summary.symbol = Some(get_node_text(&name_node, source));
                                summary.symbol_kind = Some(crate::schema::SymbolKind::Function);

                                // Extract arrow function parameters
                                if let Some(params) = value_node.child_by_field_name("parameters") {
                                    extract_js_parameters(summary, &params, source);
                                } else if let Some(param) = value_node.child_by_field_name("parameter") {
                                    // Single parameter without parentheses
                                    let name = get_node_text(&param, source);
                                    summary.arguments.push(crate::schema::Argument {
                                        name,
                                        arg_type: None,
                                        default_value: None,
                                    });
                                }

                                if returns_jsx(&value_node, source) {
                                    summary.symbol_kind =
                                        Some(crate::schema::SymbolKind::Component);
                                    summary.return_type = Some("JSX.Element".to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Extract function parameters from a formal_parameters node
fn extract_js_parameters(
    summary: &mut SemanticSummary,
    params: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                summary.arguments.push(crate::schema::Argument {
                    name: get_node_text(&child, source),
                    arg_type: None,
                    default_value: None,
                });
            }
            "required_parameter" | "optional_parameter" => {
                // TypeScript typed parameter
                let name = child
                    .child_by_field_name("pattern")
                    .map(|n| get_node_text(&n, source))
                    .unwrap_or_default();
                let arg_type = child
                    .child_by_field_name("type")
                    .map(|n| get_node_text(&n, source));
                summary.arguments.push(crate::schema::Argument {
                    name,
                    arg_type,
                    default_value: None,
                });
            }
            "assignment_pattern" => {
                // Parameter with default value
                if let Some(left) = child.child_by_field_name("left") {
                    let name = get_node_text(&left, source);
                    let default_value = child
                        .child_by_field_name("right")
                        .map(|n| get_node_text(&n, source));
                    summary.arguments.push(crate::schema::Argument {
                        name,
                        arg_type: None,
                        default_value,
                    });
                }
            }
            "object_pattern" => {
                // Destructured props - these become component props
                extract_object_pattern_as_props(summary, &child, source);
            }
            _ => {}
        }
    }
}

/// Extract destructured object pattern as component props
fn extract_object_pattern_as_props(
    summary: &mut SemanticSummary,
    pattern: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = pattern.walk();
    for child in pattern.children(&mut cursor) {
        if child.kind() == "shorthand_property_identifier_pattern" {
            let name = get_node_text(&child, source);
            summary.props.push(crate::schema::Prop {
                name,
                prop_type: None,
                default_value: None,
                required: true,
            });
        } else if child.kind() == "pair_pattern" {
            if let Some(key) = child.child_by_field_name("key") {
                let name = get_node_text(&key, source);
                let default_value = child
                    .child_by_field_name("value")
                    .and_then(|v| {
                        if v.kind() == "assignment_pattern" {
                            v.child_by_field_name("right").map(|r| get_node_text(&r, source))
                        } else {
                            None
                        }
                    });
                let is_required = default_value.is_none();
                summary.props.push(crate::schema::Prop {
                    name,
                    prop_type: None,
                    default_value,
                    required: is_required,
                });
            }
        }
    }
}

fn returns_jsx(node: &tree_sitter::Node, _source: &str) -> bool {
    // Only detect JSX via AST node kinds, not text matching
    // Text matching "<" causes false positives with TypeScript generics
    contains_node_kind(node, "jsx_element")
        || contains_node_kind(node, "jsx_self_closing_element")
        || contains_node_kind(node, "jsx_fragment")
}

fn contains_node_kind(node: &tree_sitter::Node, kind: &str) -> bool {
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

fn extract_imports_js(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            // Get imported names from import clause
            if let Some(clause) = child.child_by_field_name("source") {
                let module = get_node_text(&clause, source);
                let module = module.trim_matches('"').trim_matches('\'');

                // Get specific imports
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "import_clause" {
                        extract_import_names(summary, &inner, source, module);
                    }
                }
            }
        }
    }
}

fn extract_import_names(
    summary: &mut SemanticSummary,
    clause: &tree_sitter::Node,
    source: &str,
    _module: &str,
) {
    let mut cursor = clause.walk();

    for child in clause.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Default import - filter to meaningful imports
                let name = get_node_text(&child, source);
                if is_meaningful_import(&name) {
                    summary.added_dependencies.push(name);
                }
            }
            "named_imports" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "import_specifier" {
                        if let Some(name) = inner.child_by_field_name("name") {
                            let name_str = get_node_text(&name, source);
                            if is_meaningful_import(&name_str) {
                                summary.added_dependencies.push(name_str);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check if an import is meaningful for semantic analysis
/// Includes: React hooks, navigation components, state management
/// Excludes: Layout wrappers like Outlet that don't add UI complexity
fn is_meaningful_import(name: &str) -> bool {
    // React hooks (useState, useEffect, useReducer, etc.)
    if name.starts_with("use")
        && name.chars().nth(3).map(|c| c.is_uppercase()).unwrap_or(false)
    {
        return true;
    }

    // Navigation and routing components
    if matches!(
        name,
        "Link" | "NavLink" | "Navigate" | "Router" | "Route" | "Routes"
    ) {
        return true;
    }

    // State management
    if matches!(
        name,
        "createContext"
            | "useContext"
            | "createStore"
            | "Provider"
            | "connect"
            | "useSelector"
            | "useDispatch"
    ) {
        return true;
    }

    // Exclude layout wrappers that don't add semantic complexity
    if matches!(name, "Outlet" | "Fragment" | "Suspense" | "ErrorBoundary") {
        return false;
    }

    // Include other named imports by default (types, utilities, etc.)
    // For Phase 1, we're conservative - include most imports
    true
}

fn extract_state_hooks(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useState" || func_name == "useReducer" {
                    // Look for the variable declarator parent to get the state name
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "variable_declarator" {
                            if let Some(name_node) = parent.child_by_field_name("name") {
                                if name_node.kind() == "array_pattern" {
                                    // Get first element of destructuring
                                    let mut cursor = name_node.walk();
                                    for child in name_node.children(&mut cursor) {
                                        if child.kind() == "identifier" {
                                            let state_name = get_node_text(&child, source);

                                            // Get initializer
                                            let mut init = "undefined".to_string();
                                            if let Some(args) = node.child_by_field_name("arguments")
                                            {
                                                let mut arg_cursor = args.walk();
                                                for arg in args.children(&mut arg_cursor) {
                                                    if arg.kind() != "(" && arg.kind() != ")" {
                                                        init = get_node_text(&arg, source);
                                                        break;
                                                    }
                                                }
                                            }

                                            summary.state_changes.push(crate::schema::StateChange {
                                                name: state_name,
                                                state_type: "boolean".to_string(), // Simplified
                                                initializer: init,
                                            });

                                            // Add insertion for state hook
                                            summary.insertions.push(format!(
                                                "local {} state via {}",
                                                get_node_text(&child, source),
                                                func_name
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

fn extract_jsx_insertions(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut jsx_tags: Vec<String> = Vec::new();
    let mut has_conditional_render = false;
    let mut button_text: Option<String> = None;
    let mut conditional_text: Option<String> = None;

    visit_all(root, |node| {
        // Track JSX elements
        if node.kind() == "jsx_element" || node.kind() == "jsx_self_closing_element" {
            if let Some(opening) = node.child(0) {
                let tag_node = if opening.kind() == "jsx_opening_element" {
                    opening.child_by_field_name("name")
                } else if node.kind() == "jsx_self_closing_element" {
                    node.child_by_field_name("name")
                } else {
                    None
                };

                if let Some(tag) = tag_node {
                    let tag_name = get_node_text(&tag, source);

                    // Capture button text for dropdown detection
                    if tag_name == "button" {
                        let btn_text = get_node_text(node, source);
                        if btn_text.contains("Account") {
                            button_text = Some("account".to_string());
                        }
                    }

                    jsx_tags.push(tag_name);
                }
            }
        }

        // Detect conditional rendering pattern: {condition && <element>}
        if node.kind() == "jsx_expression" {
            let expr_text = get_node_text(node, source);
            if expr_text.contains("&&") {
                has_conditional_render = true;
                // Check for sign out text
                if expr_text.to_lowercase().contains("sign out")
                    || expr_text.to_lowercase().contains("signout")
                    || expr_text.to_lowercase().contains("logout")
                {
                    conditional_text = Some("sign out".to_string());
                }
            }
        }
    });

    // Apply insertion rules (in order from plan.md)

    // 1. Header container detection
    if jsx_tags.iter().any(|t| t == "header") {
        if jsx_tags.iter().any(|t| t == "nav") {
            summary
                .insertions
                .push("header container with nav".to_string());
        } else {
            summary.insertions.push("header container".to_string());
        }
    }

    // 2. Route links count
    let link_count = jsx_tags.iter().filter(|t| *t == "Link" || *t == "a").count();
    if link_count >= 3 {
        summary
            .insertions
            .push(format!("{} route links", link_count));
    }

    // 3. Account dropdown with sign out (button + conditional render pattern)
    if button_text.is_some() && has_conditional_render {
        let dropdown_desc = if conditional_text.is_some() {
            "account dropdown with sign out".to_string()
        } else {
            "account dropdown".to_string()
        };
        summary.insertions.push(dropdown_desc);
    } else if jsx_tags.iter().any(|t| t == "button")
        && jsx_tags.iter().any(|t| t == "div" || t == "menu")
        && has_conditional_render
    {
        summary.insertions.push("dropdown menu".to_string());
    }
}

fn extract_control_flow_js(summary: &mut SemanticSummary, root: &tree_sitter::Node, _source: &str) {
    visit_all(root, |node| {
        let kind = match node.kind() {
            "if_statement" => Some(crate::schema::ControlFlowKind::If),
            "for_statement" | "for_in_statement" => Some(crate::schema::ControlFlowKind::For),
            "while_statement" => Some(crate::schema::ControlFlowKind::While),
            "switch_statement" => Some(crate::schema::ControlFlowKind::Switch),
            "try_statement" => Some(crate::schema::ControlFlowKind::Try),
            _ => None,
        };

        if let Some(k) = kind {
            summary
                .control_flow_changes
                .push(crate::schema::ControlFlowChange {
                    kind: k,
                    location: crate::schema::Location::new(
                        node.start_position().row + 1,
                        node.start_position().column,
                    ),
                });
        }
    });
}

/// Extract function calls with context (awaited, in try block)
fn extract_calls_js(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    // First, collect all try_statement ranges
    let mut try_ranges: Vec<(usize, usize)> = Vec::new();
    visit_all(root, |node| {
        if node.kind() == "try_statement" {
            try_ranges.push((node.start_byte(), node.end_byte()));
        }
    });

    // Now extract calls
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let (name, object) = extract_call_name(&func, source);

                // Skip if it's a hook (already handled by state extraction)
                if crate::schema::Call::check_is_hook(&name) {
                    return;
                }

                // Skip common utility calls that don't add semantic value
                if is_trivial_call(&name) {
                    return;
                }

                // Check if this call is awaited
                let is_awaited = node.parent()
                    .map(|p| p.kind() == "await_expression")
                    .unwrap_or(false);

                // Check if this call is inside a try block
                let node_start = node.start_byte();
                let in_try = try_ranges.iter().any(|(start, end)| {
                    node_start >= *start && node_start < *end
                });

                // Compute is_io before moving name
                let is_io = crate::schema::Call::check_is_io(&name);

                summary.calls.push(crate::schema::Call {
                    name,
                    object,
                    is_awaited,
                    in_try,
                    is_hook: false,
                    is_io,
                    location: crate::schema::Location::new(
                        node.start_position().row + 1,
                        node.start_position().column,
                    ),
                });
            }
        }
    });
}

/// Extract function name and optional object from a call expression
fn extract_call_name(func_node: &tree_sitter::Node, source: &str) -> (String, Option<String>) {
    match func_node.kind() {
        "identifier" => (get_node_text(func_node, source), None),
        "member_expression" => {
            let property = func_node.child_by_field_name("property")
                .map(|p| get_node_text(&p, source))
                .unwrap_or_default();

            // Simplify object - only get the immediate identifier, not full expression
            let object = func_node.child_by_field_name("object")
                .map(|o| simplify_object(&o, source));

            (property, object)
        }
        _ => (get_node_text(func_node, source), None),
    }
}

/// Simplify an object expression to its core identifier (avoid capturing full call chains)
fn simplify_object(node: &tree_sitter::Node, source: &str) -> String {
    match node.kind() {
        "identifier" => get_node_text(node, source),
        "this" => "this".to_string(),
        "member_expression" => {
            // For chained calls like foo.bar.baz(), get the root identifier
            if let Some(obj) = node.child_by_field_name("object") {
                let root = simplify_object(&obj, source);
                if let Some(prop) = node.child_by_field_name("property") {
                    let prop_name = get_node_text(&prop, source);
                    return format!("{}.{}", root, prop_name);
                }
                return root;
            }
            get_node_text(node, source)
        }
        "call_expression" => {
            // For something like foo().bar(), just use the function name
            if let Some(func) = node.child_by_field_name("function") {
                let (name, obj) = extract_call_name(&func, source);
                if let Some(o) = obj {
                    return format!("{}#{}", o, name); // Use # to indicate it's a call result
                }
                return format!("{}#", name);
            }
            "_".to_string()
        }
        _ => {
            // For complex expressions, truncate and simplify
            let text = get_node_text(node, source);
            if text.len() > 20 {
                format!("{}...", &text.chars().take(17).collect::<String>())
            } else {
                text
            }
        }
    }
}

/// Check if a call is trivial (low semantic value)
fn is_trivial_call(name: &str) -> bool {
    matches!(
        name,
        "log" | "debug" | "info" | "warn" | "error" | "trace" // console methods
        | "toString" | "valueOf" | "toJSON" // conversions
        | "push" | "pop" | "shift" | "unshift" // array mutations (common)
        | "forEach" | "map" | "filter" | "reduce" | "find" | "some" | "every" // array iterations
        | "keys" | "values" | "entries" // object methods
        | "parseInt" | "parseFloat" | "Number" | "String" | "Boolean" // type conversions
    )
}

// ============================================================================
// Rust extraction helpers
// ============================================================================

fn find_primary_symbol_rust(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Function);
                    // Phase 1: Single file analysis has no "before" state to compare
                    // public_surface_changed will be set in Phase 2 diff analysis
                    return;
                }
            }
            "struct_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Struct);
                    return;
                }
            }
            "impl_item" => {
                if let Some(type_node) = child.child_by_field_name("type") {
                    summary.symbol = Some(get_node_text(&type_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Method);
                    return;
                }
            }
            "trait_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Trait);
                    return;
                }
            }
            "enum_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Enum);
                    return;
                }
            }
            _ => {}
        }
    }
}

fn extract_imports_rust(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "use_declaration" {
            // Get the full use path
            if let Some(arg) = child.child_by_field_name("argument") {
                let use_text = get_node_text(&arg, source);
                // Extract the last segment as the imported name
                if let Some(last) = use_text.split("::").last() {
                    let name = last.trim_matches('{').trim_matches('}').trim();
                    if !name.is_empty() && name != "*" {
                        summary.added_dependencies.push(name.to_string());
                    }
                }
            }
        }
    }
}

fn extract_state_rust(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "let_declaration" {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                let name = get_node_text(&pattern, source);
                let type_str = node
                    .child_by_field_name("type")
                    .map(|t| get_node_text(&t, source))
                    .unwrap_or_else(|| "_".to_string());
                let init = node
                    .child_by_field_name("value")
                    .map(|v| get_node_text(&v, source))
                    .unwrap_or_else(|| "_".to_string());

                summary.state_changes.push(crate::schema::StateChange {
                    name,
                    state_type: type_str,
                    initializer: init,
                });
            }
        }
    });
}

fn extract_control_flow_rust(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    _source: &str,
) {
    visit_all(root, |node| {
        let kind = match node.kind() {
            "if_expression" => Some(crate::schema::ControlFlowKind::If),
            "for_expression" => Some(crate::schema::ControlFlowKind::For),
            "while_expression" => Some(crate::schema::ControlFlowKind::While),
            "match_expression" => Some(crate::schema::ControlFlowKind::Match),
            "loop_expression" => Some(crate::schema::ControlFlowKind::Loop),
            _ => None,
        };

        if let Some(k) = kind {
            summary
                .control_flow_changes
                .push(crate::schema::ControlFlowChange {
                    kind: k,
                    location: crate::schema::Location::new(
                        node.start_position().row + 1,
                        node.start_position().column,
                    ),
                });
        }
    });
}

// ============================================================================
// Python extraction helpers
// ============================================================================

fn find_primary_symbol_python(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Function);
                    return;
                }
            }
            "class_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    summary.symbol = Some(get_node_text(&name_node, source));
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Class);
                    return;
                }
            }
            "decorated_definition" => {
                // Look inside decorated definition
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "function_definition" || inner.kind() == "class_definition" {
                        find_primary_symbol_python(summary, &inner, source);
                        if summary.symbol.is_some() {
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_imports_python(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                if let Some(name) = child.child_by_field_name("name") {
                    summary
                        .added_dependencies
                        .push(get_node_text(&name, source));
                }
            }
            "import_from_statement" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "dotted_name" || inner.kind() == "aliased_import" {
                        summary
                            .added_dependencies
                            .push(get_node_text(&inner, source));
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_state_python(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "assignment" {
                    if let Some(left) = inner.child_by_field_name("left") {
                        if let Some(right) = inner.child_by_field_name("right") {
                            summary.state_changes.push(crate::schema::StateChange {
                                name: get_node_text(&left, source),
                                state_type: "_".to_string(),
                                initializer: get_node_text(&right, source),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn extract_control_flow_python(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    _source: &str,
) {
    visit_all(root, |node| {
        let kind = match node.kind() {
            "if_statement" => Some(crate::schema::ControlFlowKind::If),
            "for_statement" => Some(crate::schema::ControlFlowKind::For),
            "while_statement" => Some(crate::schema::ControlFlowKind::While),
            "try_statement" => Some(crate::schema::ControlFlowKind::Try),
            "match_statement" => Some(crate::schema::ControlFlowKind::Match),
            _ => None,
        };

        if let Some(k) = kind {
            summary
                .control_flow_changes
                .push(crate::schema::ControlFlowChange {
                    kind: k,
                    location: crate::schema::Location::new(
                        node.start_position().row + 1,
                        node.start_position().column,
                    ),
                });
        }
    });
}

// ============================================================================
// Go extraction helpers
// ============================================================================

fn find_primary_symbol_go(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(&name_node, source);
                    summary.symbol = Some(name.clone());
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Function);
                    // Phase 1: Single file analysis has no "before" state to compare
                    // public_surface_changed will be set in Phase 2 diff analysis
                    return;
                }
            }
            "type_declaration" => {
                // Look for struct or interface
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "type_spec" {
                        if let Some(name_node) = inner.child_by_field_name("name") {
                            summary.symbol = Some(get_node_text(&name_node, source));
                            summary.symbol_kind = Some(crate::schema::SymbolKind::Struct);
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_imports_go(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "import_spec" {
            if let Some(path) = node.child_by_field_name("path") {
                let import_path = get_node_text(&path, source);
                // Get the last segment of the import path
                let clean = import_path.trim_matches('"');
                if let Some(last) = clean.split('/').last() {
                    summary.added_dependencies.push(last.to_string());
                }
            }
        }
    });
}

// ============================================================================
// Java extraction helpers
// ============================================================================

fn find_primary_symbol_java(
    summary: &mut SemanticSummary,
    root: &tree_sitter::Node,
    source: &str,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "class_declaration" {
            if let Some(name_node) = child.child_by_field_name("name") {
                summary.symbol = Some(get_node_text(&name_node, source));
                summary.symbol_kind = Some(crate::schema::SymbolKind::Class);
                // Phase 1: Single file analysis has no "before" state to compare
                // public_surface_changed will be set in Phase 2 diff analysis
                return;
            }
        }
    }
}

fn extract_imports_java(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            let import_text = get_node_text(&child, source);
            // Extract the class name from the import
            let clean = import_text.trim_start_matches("import ");
            let clean = clean.trim_end_matches(';');
            if let Some(last) = clean.split('.').last() {
                if last != "*" {
                    summary.added_dependencies.push(last.to_string());
                }
            }
        }
    }
}

// ============================================================================
// C/C++ extraction helpers
// ============================================================================

fn find_primary_symbol_c(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "function_definition" {
            if let Some(declarator) = child.child_by_field_name("declarator") {
                // Navigate to find the function name
                let name = extract_declarator_name(&declarator, source);
                if let Some(n) = name {
                    summary.symbol = Some(n);
                    summary.symbol_kind = Some(crate::schema::SymbolKind::Function);
                    return;
                }
            }
        }
    }
}

fn extract_declarator_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(get_node_text(node, source)),
        "function_declarator" => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                extract_declarator_name(&declarator, source)
            } else {
                None
            }
        }
        "pointer_declarator" => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                extract_declarator_name(&declarator, source)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_includes_c(summary: &mut SemanticSummary, root: &tree_sitter::Node, source: &str) {
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

// ============================================================================
// Utility functions
// ============================================================================

/// Reorder insertions to put state hooks last (per plan.md spec)
fn reorder_insertions(insertions: &mut Vec<String>) {
    // Separate state hook insertions from others
    let (state_hooks, others): (Vec<_>, Vec<_>) = insertions
        .drain(..)
        .partition(|s| s.contains("state via"));

    // Put UI structure first, state hooks last
    insertions.extend(others);
    insertions.extend(state_hooks);
}

/// Get text content of a node
fn get_node_text(node: &tree_sitter::Node, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .unwrap_or("")
        .to_string()
}

/// Visit all nodes in a tree
fn visit_all<F>(node: &tree_sitter::Node, mut visitor: F)
where
    F: FnMut(&tree_sitter::Node),
{
    visit_all_recursive(node, &mut visitor);
}

fn visit_all_recursive<F>(node: &tree_sitter::Node, visitor: &mut F)
where
    F: FnMut(&tree_sitter::Node),
{
    visitor(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_all_recursive(&child, visitor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse_source(source: &str, lang: Lang) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang.tree_sitter_language()).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_extract_tsx_component() {
        let source = r#"
import { useState } from "react";
import { Link } from "react-router-dom";

export default function AppLayout() {
    const [open, setOpen] = useState(false);
    return <div><header><nav><Link to="/a" /></nav></header></div>;
}
"#;

        let tree = parse_source(source, Lang::Tsx);
        let path = PathBuf::from("test.tsx");
        let summary = extract(&path, source, &tree, Lang::Tsx).unwrap();

        assert_eq!(summary.symbol, Some("AppLayout".to_string()));
        // Phase 1: public_surface_changed is always false (no "before" state to compare)
        assert!(!summary.public_surface_changed);
        assert!(!summary.added_dependencies.is_empty());
    }

    #[test]
    fn test_extract_rust_function() {
        let source = r#"
use std::io::Result;

pub fn main() -> Result<()> {
    let x = 42;
    if x > 0 {
        println!("positive");
    }
    Ok(())
}
"#;

        let tree = parse_source(source, Lang::Rust);
        let path = PathBuf::from("test.rs");
        let summary = extract(&path, source, &tree, Lang::Rust).unwrap();

        assert_eq!(summary.symbol, Some("main".to_string()));
        // Phase 1: public_surface_changed is always false (no "before" state to compare)
        // Phase 2 diff mode will enable this detection
        assert!(!summary.public_surface_changed);
    }

    #[test]
    fn test_extract_python_function() {
        let source = r#"
import os
from typing import List

def process_files(paths: List[str]) -> None:
    for path in paths:
        if os.path.exists(path):
            print(path)
"#;

        let tree = parse_source(source, Lang::Python);
        let path = PathBuf::from("test.py");
        let summary = extract(&path, source, &tree, Lang::Python).unwrap();

        assert_eq!(summary.symbol, Some("process_files".to_string()));
        assert!(!summary.added_dependencies.is_empty());
    }
}
