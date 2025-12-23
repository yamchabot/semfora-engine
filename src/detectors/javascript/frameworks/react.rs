//! React Framework Detector
//!
//! Specialized extraction for React applications including:
//! - State hooks (useState, useReducer)
//! - Effect hooks (useEffect with dependency tracking)
//! - Memoization hooks (useMemo, useCallback)
//! - Ref hooks (useRef for DOM and mutable values)
//! - JSX elements and component structure
//! - forwardRef/memo patterns
//! - styled-components

use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::schema::{
    Call, FrameworkEntryPoint, SemanticSummary, StateChange, SymbolInfo, SymbolKind,
};

/// Enhance semantic summary with React-specific information
///
/// This is called when React is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Extract state hooks (useState, useReducer)
    extract_state_hooks(summary, root, source);

    // Extract createContext calls
    extract_context_creation(summary, root, source);

    // Extract effect hooks (useEffect)
    extract_effect_hooks(summary, root, source);

    // Extract memoization hooks (useMemo, useCallback)
    extract_memo_hooks(summary, root, source);
    extract_callback_hooks(summary, root, source);

    // Extract ref hooks (useRef)
    extract_ref_hooks(summary, root, source);

    // Extract JSX patterns
    extract_jsx_insertions(summary, root, source);

    // Detect React root/entry point
    detect_react_entry_points(summary, source);
}

/// Detect React application entry points
///
/// Marks components used with:
/// - ReactDOM.createRoot().render()
/// - ReactDOM.render()
/// - App component in entry files (index.tsx, main.tsx)
fn detect_react_entry_points(summary: &mut SemanticSummary, source: &str) {
    let file_lower = summary.file.to_lowercase();
    let is_entry_file = file_lower.ends_with("/index.tsx")
        || file_lower.ends_with("/index.jsx")
        || file_lower.ends_with("/main.tsx")
        || file_lower.ends_with("/main.jsx")
        || file_lower.ends_with("/app.tsx")
        || file_lower.ends_with("/app.jsx");

    // ReactDOM.createRoot or ReactDOM.render indicates root component
    let is_root_file = source.contains("createRoot") || source.contains("ReactDOM.render");

    if is_root_file || is_entry_file {
        // Mark exported components as root components
        for symbol in &mut summary.symbols {
            if symbol.is_exported && symbol.kind == SymbolKind::Component {
                symbol.framework_entry_point = FrameworkEntryPoint::ReactRootComponent;
            }
        }

        if is_root_file {
            summary.framework_entry_point = FrameworkEntryPoint::ReactRootComponent;
            push_unique_insertion(
                &mut summary.insertions,
                "React root mount".to_string(),
                "root mount",
            );
        }
    }
}

// =============================================================================
// State Hooks Extraction
// =============================================================================

/// Extract React state hooks (useState, useReducer)
///
/// Detects patterns like:
/// ```javascript
/// const [count, setCount] = useState(0);
/// const [state, dispatch] = useReducer(reducer, initialState);
/// ```
pub fn extract_state_hooks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useState" || func_name == "useReducer" {
                    extract_hook_state(summary, node, &func_name, source);
                }
            }
        }
    });
}

/// Extract state from a hook call
fn extract_hook_state(summary: &mut SemanticSummary, node: &Node, func_name: &str, source: &str) {
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                if name_node.kind() == "array_pattern" {
                    let mut cursor = name_node.walk();
                    let mut found_state = false;
                    let mut found_setter = false;

                    for child in name_node.children(&mut cursor) {
                        if child.kind() == "identifier" {
                            let name = get_node_text(&child, source);
                            let start_line = child.start_position().row + 1;
                            let end_line = child.end_position().row + 1;

                            if !found_state {
                                // First identifier is the state variable
                                let init = extract_hook_initializer(node, source);

                                summary.state_changes.push(StateChange {
                                    name: name.clone(),
                                    state_type: infer_type(&init),
                                    initializer: init,
                                });

                                summary
                                    .insertions
                                    .push(format!("local {} state via {}", name, func_name));

                                // Create a Variable symbol for the state variable
                                let exists = summary
                                    .symbols
                                    .iter()
                                    .any(|s| s.name == name && s.kind == SymbolKind::Variable);

                                if !exists {
                                    summary.symbols.push(SymbolInfo {
                                        name: name.clone(),
                                        kind: SymbolKind::Variable,
                                        start_line,
                                        end_line,
                                        is_exported: false,
                                        is_default_export: false,
                                        framework_entry_point: FrameworkEntryPoint::ReactState,
                                        ..Default::default()
                                    });
                                }

                                found_state = true;
                            } else if !found_setter {
                                // Second identifier is the setter function - also create a symbol
                                let exists = summary
                                    .symbols
                                    .iter()
                                    .any(|s| s.name == name && s.kind == SymbolKind::Variable);

                                if !exists {
                                    summary.symbols.push(SymbolInfo {
                                        name: name.clone(),
                                        kind: SymbolKind::Variable,
                                        start_line,
                                        end_line,
                                        is_exported: false,
                                        is_default_export: false,
                                        framework_entry_point: FrameworkEntryPoint::ReactState,
                                        ..Default::default()
                                    });
                                }

                                found_setter = true;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Extract the initializer value from a hook call
fn extract_hook_initializer(node: &Node, source: &str) -> String {
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut arg_cursor = args.walk();
        for arg in args.children(&mut arg_cursor) {
            if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                return get_node_text(&arg, source);
            }
        }
    }
    "undefined".to_string()
}

/// Infer type from initializer
fn infer_type(init: &str) -> String {
    let trimmed = init.trim();
    if trimmed.starts_with('"') || trimmed.starts_with('\'') || trimmed.starts_with('`') {
        "string".to_string()
    } else if trimmed.parse::<i64>().is_ok() || trimmed.parse::<f64>().is_ok() {
        "number".to_string()
    } else if trimmed == "true" || trimmed == "false" {
        "boolean".to_string()
    } else if trimmed.starts_with('[') {
        "array".to_string()
    } else if trimmed.starts_with('{') {
        "object".to_string()
    } else if trimmed == "null" {
        "null".to_string()
    } else {
        "_".to_string()
    }
}

// =============================================================================
// Context Extraction
// =============================================================================

/// Extract React createContext calls and create Variable symbols for contexts
///
/// Detects patterns like:
/// ```javascript
/// const MyContext = createContext(defaultValue);
/// export const ThemeContext = React.createContext({ theme: 'light' });
/// ```
fn extract_context_creation(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_text = get_node_text(&func, source);
                // Match createContext or React.createContext
                if func_text == "createContext" || func_text.ends_with(".createContext") {
                    extract_context_variable(summary, node, source);
                }
            }
        }
    });
}

/// Extract the variable name from a createContext call
fn extract_context_variable(summary: &mut SemanticSummary, node: &Node, source: &str) {
    // Navigate up to find the variable declarator
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                if name_node.kind() == "identifier" {
                    let context_name = get_node_text(&name_node, source);
                    let start_line = name_node.start_position().row + 1;
                    let end_line = name_node.end_position().row + 1;

                    // Check if already exists
                    let exists = summary
                        .symbols
                        .iter()
                        .any(|s| s.name == context_name && s.kind == SymbolKind::Variable);

                    if !exists {
                        // Check if it's exported by looking at the grandparent
                        let is_exported = parent.parent().map_or(false, |gp| {
                            gp.kind() == "export_statement"
                                || (gp.kind() == "lexical_declaration"
                                    && gp.parent().map_or(false, |ggp| {
                                        ggp.kind() == "export_statement"
                                    }))
                        });

                        summary.symbols.push(SymbolInfo {
                            name: context_name.clone(),
                            kind: SymbolKind::Variable,
                            start_line,
                            end_line,
                            is_exported,
                            is_default_export: false,
                            framework_entry_point: FrameworkEntryPoint::ReactContext,
                            ..Default::default()
                        });

                        summary
                            .insertions
                            .push(format!("React context {}", context_name));
                    }
                }
            }
        }
    }
}

// =============================================================================
// Effect Hooks Extraction
// =============================================================================

/// Extract React effect hooks (useEffect, useLayoutEffect)
///
/// Detects patterns like:
/// ```javascript
/// useEffect(() => { fetchData(); }, [id]);      // effect on [id]
/// useEffect(() => { setup(); }, []);            // effect on mount
/// useEffect(() => { update(); });               // effect on every render
/// useEffect(() => { return () => cleanup(); }, []); // effect with cleanup
/// ```
pub fn extract_effect_hooks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useEffect" || func_name == "useLayoutEffect" {
                    extract_effect_info(summary, node, &func_name, source);
                }
            }
        }
    });
}

/// Extract information from an effect hook call
fn extract_effect_info(summary: &mut SemanticSummary, node: &Node, func_name: &str, source: &str) {
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut callback_node: Option<Node> = None;
        let mut deps_node: Option<Node> = None;
        let mut arg_index = 0;

        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
                continue;
            }
            if arg_index == 0 {
                callback_node = Some(child);
            } else if arg_index == 1 {
                deps_node = Some(child);
            }
            arg_index += 1;
        }

        // Check for cleanup function (return statement in callback)
        let has_cleanup = callback_node
            .map(|cb| {
                let cb_text = get_node_text(&cb, source);
                cb_text.contains("return ")
                    && (cb_text.contains("() =>") || cb_text.contains("function"))
            })
            .unwrap_or(false);

        // Build insertion string
        let effect_type = if func_name == "useLayoutEffect" {
            "layout effect"
        } else {
            "effect"
        };

        let deps_desc = match deps_node {
            Some(deps) => {
                let deps_text = get_node_text(&deps, source);
                if deps_text == "[]" {
                    "on mount".to_string()
                } else {
                    // Extract dependency names from array
                    let deps_inner = deps_text.trim_start_matches('[').trim_end_matches(']');
                    if deps_inner.is_empty() {
                        "on mount".to_string()
                    } else {
                        format!("on [{}]", truncate_deps(deps_inner))
                    }
                }
            }
            None => "on every render".to_string(),
        };

        let cleanup_suffix = if has_cleanup { " with cleanup" } else { "" };

        push_unique_insertion(
            &mut summary.insertions,
            format!("{} {}{}", effect_type, deps_desc, cleanup_suffix),
            effect_type,
        );
    }
}

// =============================================================================
// Memoization Hooks Extraction
// =============================================================================

/// Extract React memoization hooks (useMemo)
///
/// Detects patterns like:
/// ```javascript
/// const value = useMemo(() => expensiveCalc(a, b), [a, b]);
/// const filtered = useMemo(() => items.filter(predicate), [items]);
/// ```
pub fn extract_memo_hooks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useMemo" {
                    extract_memo_info(summary, node, source);
                }
            }
        }
    });
}

/// Extract information from a useMemo call
fn extract_memo_info(summary: &mut SemanticSummary, node: &Node, source: &str) {
    // Get variable name from parent
    let var_name = get_hook_variable_name(node, source);

    // Get dependencies
    let deps = extract_hook_deps(node, source);

    let insertion = match (var_name, deps) {
        (Some(name), Some(deps)) => format!("memoized {} on [{}]", name, truncate_deps(&deps)),
        (Some(name), None) => format!("memoized {}", name),
        (None, Some(deps)) => format!("memoized value on [{}]", truncate_deps(&deps)),
        (None, None) => "memoized value".to_string(),
    };

    push_unique_insertion(&mut summary.insertions, insertion, "memo");
}

/// Extract React callback hooks (useCallback)
///
/// Detects patterns like:
/// ```javascript
/// const handleClick = useCallback(() => onClick(id), [id, onClick]);
/// const submit = useCallback(async () => { await api.post(); }, []);
/// ```
pub fn extract_callback_hooks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useCallback" {
                    extract_callback_info(summary, node, source);
                }
            }
        }
    });
}

/// Extract information from a useCallback call
fn extract_callback_info(summary: &mut SemanticSummary, node: &Node, source: &str) {
    // Get variable name from parent
    let var_name = get_hook_variable_name(node, source);

    // Get dependencies
    let deps = extract_hook_deps(node, source);

    let insertion = match (var_name, deps) {
        (Some(name), Some(deps)) => {
            format!("memoized callback {} on [{}]", name, truncate_deps(&deps))
        }
        (Some(name), None) => format!("memoized callback {}", name),
        (None, Some(deps)) => format!("memoized callback on [{}]", truncate_deps(&deps)),
        (None, None) => "memoized callback".to_string(),
    };

    push_unique_insertion(&mut summary.insertions, insertion, "callback");
}

// =============================================================================
// Ref Hooks Extraction
// =============================================================================

/// Extract React ref hooks (useRef)
///
/// Detects patterns like:
/// ```javascript
/// const inputRef = useRef(null);           // DOM ref
/// const timerRef = useRef<number>();       // Mutable value
/// const countRef = useRef(0);              // Mutable counter
/// ```
pub fn extract_ref_hooks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useRef" {
                    extract_ref_info(summary, node, source);
                }
            }
        }
    });
}

/// Extract information from a useRef call
fn extract_ref_info(summary: &mut SemanticSummary, node: &Node, source: &str) {
    // Get variable name from parent
    let var_name = get_hook_variable_name(node, source);

    // Get initializer to determine if DOM ref or mutable value
    let init = extract_hook_initializer(node, source);
    let is_dom_ref = init == "null" || init == "undefined";

    let insertion = match var_name {
        Some(name) if is_dom_ref => format!("ref: {}", name),
        Some(name) => format!("mutable ref: {}", name),
        None if is_dom_ref => "ref".to_string(),
        None => "mutable ref".to_string(),
    };

    push_unique_insertion(&mut summary.insertions, insertion, "ref");
}

// =============================================================================
// Shared Hook Utilities
// =============================================================================

/// Get the variable name that a hook result is assigned to
fn get_hook_variable_name(node: &Node, source: &str) -> Option<String> {
    // Walk up to find variable_declarator
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                let name = get_node_text(&name_node, source);
                if !name.is_empty() && name != "[" {
                    return Some(name);
                }
            }
            break;
        }
        current = parent.parent();
    }
    None
}

/// Extract dependency array from a hook call (2nd argument)
fn extract_hook_deps(node: &Node, source: &str) -> Option<String> {
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut arg_index = 0;
        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
                continue;
            }
            if arg_index == 1 {
                // Second argument is deps array
                let deps_text = get_node_text(&child, source);
                if deps_text.starts_with('[') {
                    let inner = deps_text.trim_start_matches('[').trim_end_matches(']');
                    if !inner.is_empty() {
                        return Some(inner.to_string());
                    }
                }
                break;
            }
            arg_index += 1;
        }
    }
    None
}

/// Truncate dependency list if too long
fn truncate_deps(deps: &str) -> String {
    let deps = deps.trim();
    if deps.len() > 30 {
        let parts: Vec<&str> = deps.split(',').map(|s| s.trim()).collect();
        if parts.len() > 3 {
            format!("{}, {} more", parts[..2].join(", "), parts.len() - 2)
        } else {
            format!("{}...", &deps[..27])
        }
    } else {
        deps.to_string()
    }
}

// =============================================================================
// JSX Extraction
// =============================================================================

/// Extract JSX insertions for semantic context
///
/// Detects patterns like:
/// - Header containers with navigation
/// - Route links
/// - Dropdown menus
/// - Form elements
pub fn extract_jsx_insertions(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut jsx_tags: Vec<String> = Vec::new();
    let mut has_conditional_render = false;

    visit_all(root, |node| {
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
                    jsx_tags.push(get_node_text(&tag, source));
                }
            }
        }

        if node.kind() == "jsx_expression" {
            let expr_text = get_node_text(node, source);
            if expr_text.contains("&&") || expr_text.contains("?") {
                has_conditional_render = true;
            }
        }
    });

    // Add PascalCase components to calls (for call graph)
    // This captures component usage like <Button />, <Header>, <Icons.Home />
    for tag_name in &jsx_tags {
        // Only capture PascalCase names (React components, not HTML elements)
        if tag_name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            // Only add if not already present
            if !summary.calls.iter().any(|c| c.name == *tag_name) {
                summary.calls.push(Call {
                    name: tag_name.clone(),
                    object: None,
                    is_awaited: false,
                    in_try: false,
                    ..Default::default()
                });
            }
        }
    }

    // Header detection
    detect_header_pattern(&jsx_tags, summary);

    // Route links count
    detect_route_links(&jsx_tags, summary);

    // Dropdown detection
    detect_dropdown_pattern(&jsx_tags, has_conditional_render, summary);

    // Form detection
    detect_form_pattern(&jsx_tags, summary);

    // List rendering
    detect_list_pattern(&jsx_tags, summary);
}

/// Detect header container pattern
fn detect_header_pattern(jsx_tags: &[String], summary: &mut SemanticSummary) {
    if jsx_tags.iter().any(|t| t == "header" || t == "Header") {
        if jsx_tags.iter().any(|t| t == "nav" || t == "Nav") {
            push_unique_insertion(
                &mut summary.insertions,
                "header container with nav".to_string(),
                "header",
            );
        } else {
            push_unique_insertion(
                &mut summary.insertions,
                "header container".to_string(),
                "header",
            );
        }
    }
}

/// Detect route links pattern
fn detect_route_links(jsx_tags: &[String], summary: &mut SemanticSummary) {
    let link_count = jsx_tags
        .iter()
        .filter(|t| *t == "Link" || *t == "NavLink" || *t == "a")
        .count();

    if link_count >= 3 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} route links", link_count),
            "route",
        );
    }
}

/// Detect dropdown menu pattern
fn detect_dropdown_pattern(
    jsx_tags: &[String],
    has_conditional: bool,
    summary: &mut SemanticSummary,
) {
    if jsx_tags.iter().any(|t| t == "button" || t == "Button")
        && jsx_tags
            .iter()
            .any(|t| t == "div" || t == "menu" || t == "Menu")
        && has_conditional
    {
        push_unique_insertion(
            &mut summary.insertions,
            "dropdown menu".to_string(),
            "dropdown",
        );
    }
}

/// Detect form pattern
fn detect_form_pattern(jsx_tags: &[String], summary: &mut SemanticSummary) {
    if jsx_tags.iter().any(|t| t == "form" || t == "Form") {
        let input_count = jsx_tags
            .iter()
            .filter(|t| *t == "input" || *t == "Input" || *t == "textarea" || *t == "select")
            .count();

        if input_count > 0 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("form with {} inputs", input_count),
                "form",
            );
        }
    }
}

/// Detect list rendering pattern
fn detect_list_pattern(jsx_tags: &[String], summary: &mut SemanticSummary) {
    let list_items = jsx_tags
        .iter()
        .filter(|t| *t == "li" || *t == "ListItem")
        .count();
    if list_items > 0 {
        if jsx_tags.iter().any(|t| t == "ul" || t == "ol") {
            push_unique_insertion(
                &mut summary.insertions,
                format!("list with {} items", list_items),
                "list",
            );
        }
    }
}

// =============================================================================
// Additional React Patterns
// =============================================================================

/// Check if a component uses forwardRef
pub fn uses_forward_ref(source: &str) -> bool {
    source.contains("forwardRef") || source.contains(".forwardRef")
}

/// Check if a component uses memo
pub fn uses_memo(source: &str) -> bool {
    source.contains("memo(") || source.contains(".memo(")
}

/// Check if a component uses context
pub fn uses_context(source: &str) -> bool {
    source.contains("useContext(") || source.contains("createContext")
}

/// Check if a component uses custom hooks
pub fn count_custom_hooks(source: &str) -> usize {
    // Custom hooks start with "use" followed by uppercase letter
    let mut count = 0;
    let mut chars = source.chars().peekable();

    while let Some(c) = chars.next() {
        if c == 'u' {
            if let Some('s') = chars.peek().copied() {
                chars.next();
                if let Some('e') = chars.peek().copied() {
                    chars.next();
                    if let Some(next) = chars.peek() {
                        if next.is_uppercase() {
                            count += 1;
                        }
                    }
                }
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to parse TSX source for testing
    fn parse_tsx(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("Failed to set language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_infer_type() {
        assert_eq!(infer_type("\"hello\""), "string");
        assert_eq!(infer_type("42"), "number");
        assert_eq!(infer_type("true"), "boolean");
        assert_eq!(infer_type("[]"), "array");
        assert_eq!(infer_type("{}"), "object");
        assert_eq!(infer_type("null"), "null");
    }

    #[test]
    fn test_uses_forward_ref() {
        assert!(uses_forward_ref("React.forwardRef((props, ref) => {})"));
        assert!(uses_forward_ref("forwardRef(() => {})"));
        assert!(!uses_forward_ref("const Component = () => {}"));
    }

    #[test]
    fn test_uses_memo() {
        assert!(uses_memo("React.memo(Component)"));
        assert!(uses_memo("memo(Component)"));
        assert!(!uses_memo("const Component = () => {}"));
    }

    #[test]
    fn test_count_custom_hooks() {
        assert_eq!(count_custom_hooks("useMyHook(); useOtherHook();"), 2);
        // Note: Built-in hooks also start with uppercase (useState, useEffect),
        // so they will be counted. For a more accurate count, we'd need to
        // filter out known built-in hooks.
        assert_eq!(count_custom_hooks("useState(); useEffect();"), 2);
    }

    #[test]
    fn test_truncate_deps() {
        // Short deps should pass through unchanged
        assert_eq!(truncate_deps("a, b"), "a, b");
        assert_eq!(truncate_deps("id, name"), "id, name");

        // Long deps should be truncated
        let long_deps = "firstDep, secondDep, thirdDep, fourthDep";
        let truncated = truncate_deps(long_deps);
        assert!(truncated.contains("more"));

        // Empty deps
        assert_eq!(truncate_deps(""), "");
    }

    #[test]
    fn test_extract_effect_hooks_on_mount() {
        let source = r#"
            function Component() {
                useEffect(() => {
                    console.log("mounted");
                }, []);
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_effect_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("effect on mount")),
            "Should detect effect on mount, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_effect_hooks_with_deps() {
        let source = r#"
            function Component({ id }) {
                useEffect(() => {
                    fetchData(id);
                }, [id]);
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_effect_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("effect on [id]")),
            "Should detect effect with dependency, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_effect_hooks_every_render() {
        let source = r#"
            function Component() {
                useEffect(() => {
                    console.log("render");
                });
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_effect_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("effect on every render")),
            "Should detect effect on every render, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_effect_hooks_with_cleanup() {
        let source = r#"
            function Component() {
                useEffect(() => {
                    const timer = setInterval(() => {}, 1000);
                    return () => clearInterval(timer);
                }, []);
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_effect_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary.insertions.iter().any(|i| i.contains("cleanup")),
            "Should detect effect with cleanup, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_layout_effect() {
        let source = r#"
            function Component() {
                useLayoutEffect(() => {
                    measureDOM();
                }, []);
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_effect_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("layout effect")),
            "Should detect layout effect, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_memo_hooks() {
        let source = r#"
            function Component({ items }) {
                const filtered = useMemo(() => items.filter(x => x.active), [items]);
                return <div>{filtered.length}</div>;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_memo_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("memoized") && i.contains("filtered")),
            "Should detect memoized value with name, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_callback_hooks() {
        let source = r#"
            function Component({ onClick, id }) {
                const handleClick = useCallback(() => {
                    onClick(id);
                }, [onClick, id]);
                return <button onClick={handleClick}>Click</button>;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_callback_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("memoized callback") && i.contains("handleClick")),
            "Should detect memoized callback with name, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_ref_hooks_dom_ref() {
        let source = r#"
            function Component() {
                const inputRef = useRef(null);
                return <input ref={inputRef} />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_ref_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("ref:") && i.contains("inputRef")),
            "Should detect DOM ref, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_extract_ref_hooks_mutable_ref() {
        let source = r#"
            function Component() {
                const countRef = useRef(0);
                return <div />;
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        extract_ref_hooks(&mut summary, &tree.root_node(), source);

        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("mutable ref") && i.contains("countRef")),
            "Should detect mutable ref, got: {:?}",
            summary.insertions
        );
    }

    #[test]
    fn test_enhance_extracts_all_hooks() {
        let source = r#"
            import { useState, useEffect, useMemo, useCallback, useRef } from 'react';

            function MyComponent({ id }) {
                const [count, setCount] = useState(0);
                const inputRef = useRef(null);

                useEffect(() => {
                    console.log(id);
                }, [id]);

                const doubled = useMemo(() => count * 2, [count]);

                const handleClick = useCallback(() => {
                    setCount(c => c + 1);
                }, []);

                return (
                    <div>
                        <input ref={inputRef} />
                        <span>{doubled}</span>
                        <button onClick={handleClick}>+</button>
                    </div>
                );
            }
        "#;
        let tree = parse_tsx(source);
        let mut summary = SemanticSummary::default();
        enhance(&mut summary, &tree.root_node(), source);

        // Check for useState
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("local count state")),
            "Should extract useState, got: {:?}",
            summary.insertions
        );

        // Check for useEffect
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("effect on [id]")),
            "Should extract useEffect, got: {:?}",
            summary.insertions
        );

        // Check for useMemo
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("memoized") && i.contains("doubled")),
            "Should extract useMemo, got: {:?}",
            summary.insertions
        );

        // Check for useCallback
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("memoized callback")),
            "Should extract useCallback, got: {:?}",
            summary.insertions
        );

        // Check for useRef
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("ref") && i.contains("inputRef")),
            "Should extract useRef, got: {:?}",
            summary.insertions
        );
    }
}
