//! React Framework Detector
//!
//! Specialized extraction for React applications including:
//! - State hooks (useState, useReducer)
//! - JSX elements and component structure
//! - forwardRef/memo patterns
//! - styled-components

use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::schema::{Call, SemanticSummary, StateChange};

/// Enhance semantic summary with React-specific information
///
/// This is called when React is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Extract state hooks
    extract_state_hooks(summary, root, source);

    // Extract JSX patterns
    extract_jsx_insertions(summary, root, source);
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
                    for child in name_node.children(&mut cursor) {
                        if child.kind() == "identifier" {
                            let state_name = get_node_text(&child, source);

                            let init = extract_hook_initializer(node, source);

                            summary.state_changes.push(StateChange {
                                name: state_name.clone(),
                                state_type: infer_type(&init),
                                initializer: init,
                            });

                            summary.insertions.push(format!(
                                "local {} state via {}",
                                state_name, func_name
                            ));
                            break;
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
        if tag_name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
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
fn detect_dropdown_pattern(jsx_tags: &[String], has_conditional: bool, summary: &mut SemanticSummary) {
    if jsx_tags.iter().any(|t| t == "button" || t == "Button")
        && jsx_tags.iter().any(|t| t == "div" || t == "menu" || t == "Menu")
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
    let list_items = jsx_tags.iter().filter(|t| *t == "li" || *t == "ListItem").count();
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
}
