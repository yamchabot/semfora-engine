//! Common utilities shared across all language detectors
//!
//! This module provides helper functions for AST traversal, text extraction,
//! and semantic information processing that are language-agnostic.

use tree_sitter::Node;

/// Maximum length for raw source fallback when extraction is incomplete
pub const MAX_FALLBACK_LEN: usize = 1000;

// ============================================================================
// Text Extraction
// ============================================================================

/// Get text content of a node
pub fn get_node_text(node: &Node, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .unwrap_or("")
        .to_string()
}

/// Get text content of a node, normalized to single line (collapse whitespace)
pub fn get_node_text_normalized(node: &Node, source: &str) -> String {
    normalize_whitespace(&get_node_text(node, source))
}

/// Normalize whitespace: collapse multiple spaces/newlines to single space
pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Safely truncate a string at a UTF-8 char boundary
pub fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ============================================================================
// AST Traversal
// ============================================================================

/// Visit all nodes in a tree with a visitor function
pub fn visit_all<F>(node: &Node, mut visitor: F)
where
    F: FnMut(&Node),
{
    visit_all_recursive(node, &mut visitor);
}

fn visit_all_recursive<F>(node: &Node, visitor: &mut F)
where
    F: FnMut(&Node),
{
    visitor(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_all_recursive(&child, visitor);
    }
}

/// Visit all nodes tracking control flow nesting depth
/// The visitor receives (node, nesting_depth) where nesting_depth increments
/// inside control flow constructs (if, for, while, match, loop, try, switch)
pub fn visit_with_nesting_depth<F>(node: &Node, mut visitor: F, control_flow_kinds: &[&str])
where
    F: FnMut(&Node, usize),
{
    visit_with_nesting_recursive(node, &mut visitor, 0, control_flow_kinds);
}

fn visit_with_nesting_recursive<F>(node: &Node, visitor: &mut F, depth: usize, cf_kinds: &[&str])
where
    F: FnMut(&Node, usize),
{
    visitor(node, depth);

    // Check if this node increases nesting depth
    let is_control_flow = cf_kinds.contains(&node.kind());
    let child_depth = if is_control_flow { depth + 1 } else { depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_with_nesting_recursive(&child, visitor, child_depth, cf_kinds);
    }
}

// ============================================================================
// Semantic Processing
// ============================================================================

/// Compress a complex initializer expression to a semantic summary
/// Multi-line match/if/closures become "match Foo::bar(...)" style summaries
pub fn compress_initializer(init: &str) -> String {
    let normalized = normalize_whitespace(init);

    // If it's a simple value, return as-is
    if normalized.len() <= 60 && !normalized.contains('{') {
        return normalized;
    }

    // For complex expressions, extract the essence
    let trimmed = normalized.trim();

    // Match expressions: extract "match expr {...}"
    if trimmed.starts_with("match ") {
        if let Some(brace_pos) = trimmed.find('{') {
            let match_expr = &trimmed[6..brace_pos].trim();
            // Truncate long match subjects (UTF-8 safe)
            let subject = if match_expr.len() > 40 {
                format!("{}...", truncate_to_char_boundary(match_expr, 40))
            } else {
                match_expr.to_string()
            };
            return format!("match {} {{...}}", subject);
        }
    }

    // If expressions
    if trimmed.starts_with("if ") {
        if let Some(brace_pos) = trimmed.find('{') {
            let condition = &trimmed[3..brace_pos].trim();
            let cond_short = if condition.len() > 40 {
                format!("{}...", truncate_to_char_boundary(condition, 40))
            } else {
                condition.to_string()
            };
            return format!("if {} {{...}}", cond_short);
        }
    }

    // Function/method chains: extract first call
    if trimmed.contains("(") {
        // Find the function name before first paren
        if let Some(paren_pos) = trimmed.find('(') {
            let prefix = &trimmed[..paren_pos];
            // Get last identifier in the chain
            let func_name = prefix.rsplit(&['.', ':'][..]).next().unwrap_or(prefix);
            if func_name.len() <= 30 {
                return format!("{}(...)", func_name);
            }
        }
    }

    // Struct/vec literals: summarize
    if trimmed.starts_with("vec![") || trimmed.starts_with("Vec::") {
        return "vec![...]".to_string();
    }

    if trimmed.starts_with("SemanticSummary {") || trimmed.contains("Summary {") {
        return "SemanticSummary {...}".to_string();
    }

    if trimmed.starts_with("HashMap::new") {
        return "HashMap::new()".to_string();
    }

    // Generic struct literal
    if let Some(brace_pos) = trimmed.find(" {") {
        let struct_name = &trimmed[..brace_pos];
        if struct_name.len() <= 30 && !struct_name.contains('\n') {
            return format!("{} {{...}}", struct_name);
        }
    }

    // Fallback: truncate long expressions (UTF-8 safe)
    if normalized.len() > 60 {
        format!("{}...", truncate_to_char_boundary(&normalized, 57))
    } else {
        normalized
    }
}

/// Reorder insertions to put state hooks last (per spec)
pub fn reorder_insertions(insertions: &mut Vec<String>) {
    // Separate state hook insertions from others
    let (state_hooks, others): (Vec<_>, Vec<_>) = insertions
        .drain(..)
        .partition(|s| s.contains("state via"));

    // Put UI structure first, state hooks last
    insertions.extend(others);
    insertions.extend(state_hooks);
}

/// Push an insertion only if it's unique (not already present)
pub fn push_unique_insertion(insertions: &mut Vec<String>, insertion: String, keyword: &str) {
    // Check if we already have this kind of insertion
    if !insertions.iter().any(|i| i.contains(keyword)) {
        insertions.push(insertion);
    }
}

/// Infer a type from an initializer expression (for Rust/Go/Java)
pub fn infer_type_from_initializer(init: &str) -> String {
    let trimmed = init.trim();

    // String literals
    if trimmed.starts_with('"') || trimmed.starts_with("r#\"") || trimmed.starts_with("r\"") {
        return "String".to_string();
    }

    // Number literals
    if trimmed.parse::<i64>().is_ok() {
        return "i64".to_string();
    }
    if trimmed.parse::<f64>().is_ok() {
        return "f64".to_string();
    }

    // Boolean
    if trimmed == "true" || trimmed == "false" {
        return "bool".to_string();
    }

    // Vec/array
    if trimmed.starts_with("vec![") || trimmed.starts_with("Vec::") {
        return "Vec<_>".to_string();
    }

    // HashMap
    if trimmed.starts_with("HashMap::") {
        return "HashMap<_, _>".to_string();
    }

    // Constructor patterns (Type::new, Type::default)
    if trimmed.contains("::new(") || trimmed.contains("::default(") {
        if let Some(type_name) = trimmed.split("::").next() {
            return type_name.trim().to_string();
        }
    }

    // Default - unknown/expression
    "unknown".to_string()
}

// ============================================================================
// Symbol Line Range Utilities
// ============================================================================

use crate::schema::SymbolInfo;

/// Find which symbol (by index) contains a given line number.
/// Uses the symbol's start_line and end_line to determine containment.
/// When multiple symbols contain the line (e.g., nested class/method), returns
/// the most specific one (smallest line range) to handle nested symbols correctly.
///
/// # Arguments
/// * `line` - The 1-indexed line number to find
/// * `symbols` - Slice of symbols to search through
///
/// # Returns
/// * `Some(index)` - Index of the most specific symbol containing the line
/// * `None` - If no symbol contains the line
pub fn find_containing_symbol_by_line(line: usize, symbols: &[SymbolInfo]) -> Option<usize> {
    let mut best_match: Option<(usize, usize)> = None; // (index, range_size)

    for (idx, symbol) in symbols.iter().enumerate() {
        if line >= symbol.start_line && line <= symbol.end_line {
            let range_size = symbol.end_line.saturating_sub(symbol.start_line);
            match best_match {
                None => best_match = Some((idx, range_size)),
                Some((_, best_size)) if range_size < best_size => {
                    best_match = Some((idx, range_size));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_char_boundary() {
        let s = "hello world";
        assert_eq!(truncate_to_char_boundary(s, 100), "hello world");
        assert_eq!(truncate_to_char_boundary(s, 5), "hello");

        // Test with UTF-8 characters
        let utf8 = "héllo";
        assert_eq!(truncate_to_char_boundary(utf8, 3), "hé");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello  world"), "hello world");
        assert_eq!(normalize_whitespace("hello\n\nworld"), "hello world");
        assert_eq!(normalize_whitespace("  hello  "), "hello");
    }

    #[test]
    fn test_compress_initializer() {
        assert_eq!(compress_initializer("simple"), "simple");
        assert_eq!(compress_initializer("HashMap::new()"), "HashMap::new()");
        assert_eq!(
            compress_initializer("match foo { _ => bar }"),
            "match foo {...}"
        );
    }

    #[test]
    fn test_infer_type_from_initializer() {
        assert_eq!(infer_type_from_initializer("\"hello\""), "String");
        assert_eq!(infer_type_from_initializer("42"), "i64");
        assert_eq!(infer_type_from_initializer("true"), "bool");
        assert_eq!(infer_type_from_initializer("vec![1, 2, 3]"), "Vec<_>");
    }
}
