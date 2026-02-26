//! Python language detector
//!
//! Extracts semantic information from Python source files using the generic extractor.
//! Python-specific features like decorator detection are handled in a second pass.

use tree_sitter::{Node, Tree};

use crate::detectors::common::get_node_text;
use crate::detectors::generic::extract_with_grammar;
use crate::detectors::grammar::PYTHON_GRAMMAR;
use crate::error::Result;
use crate::schema::{FrameworkEntryPoint, RiskLevel, SemanticSummary};

/// Extract semantic information from a Python source file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    // Use the generic extractor for core semantic extraction
    // This handles: symbols, imports, state_changes, control_flow, calls, risk
    extract_with_grammar(summary, source, tree, &PYTHON_GRAMMAR)?;

    // Python-specific: detect decorated definitions and improve symbol scoring
    let root = tree.root_node();
    enhance_python_symbols(summary, &root, source);

    Ok(())
}

/// Enhance Python symbols with decorator detection
/// The generic extractor finds symbols, but doesn't detect Python decorators
fn enhance_python_symbols(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let filename_stem = extract_filename_stem(&summary.file);
    let mut decorated_symbols: Vec<(String, bool)> = Vec::new(); // (name, has_important_decorator)

    // Find decorated definitions
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "decorated_definition" {
            let has_decorator = has_important_decorator(&child, source);

            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "function_definition" || inner.kind() == "class_definition" {
                    if let Some(name_node) = inner.child_by_field_name("name") {
                        let name = get_node_text(&name_node, source);
                        decorated_symbols.push((name, has_decorator));
                    }
                }
            }
        }
    }

    // Determine if this is a test file based on its path
    let is_test_file = is_test_file_path(&summary.file);

    // Update existing symbols with decorator info, test detection, and recalculate scores
    for sym in &mut summary.symbols {
        // Mark test functions as framework entry points so they are never flagged as dead code
        let name_is_test = sym.name.starts_with("test_") || sym.name.starts_with("Test");
        let has_pytest_decorator = sym.decorators.iter().any(|d| {
            let dl = d.to_lowercase();
            dl.contains("pytest") || dl.contains("unittest")
        });

        if (is_test_file && name_is_test) || has_pytest_decorator {
            sym.framework_entry_point = FrameworkEntryPoint::TestFunction;
        }

        if let Some((_, has_decorator)) = decorated_symbols.iter().find(|(n, _)| n == &sym.name) {
            if *has_decorator {
                // Boost score for decorated symbols (they're often important)
                // The behavioral_risk field can serve as a proxy for importance
                sym.behavioral_risk = RiskLevel::Medium;
            }
        }
    }

    // Re-sort symbols and update primary if a decorated symbol should take priority
    if !decorated_symbols.is_empty() {
        let best_decorated = decorated_symbols
            .iter()
            .filter(|(_, has_dec)| *has_dec)
            .next();

        if let Some((decorated_name, _)) = best_decorated {
            // Check if decorated symbol should be primary
            let current_primary_score = summary
                .symbol
                .as_ref()
                .map(|name| calculate_basic_score(name, &filename_stem))
                .unwrap_or(0);

            let decorated_score = calculate_basic_score(decorated_name, &filename_stem) + 25; // decorator bonus

            if decorated_score > current_primary_score {
                if let Some(sym) = summary.symbols.iter().find(|s| &s.name == decorated_name) {
                    summary.symbol = Some(sym.name.clone());
                    summary.symbol_kind = Some(sym.kind);
                    summary.start_line = Some(sym.start_line);
                    summary.end_line = Some(sym.end_line);
                }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

// Use shared extract_filename_stem from parent module
use super::extract_filename_stem;

/// Calculate a basic score for symbol prioritization (simplified)
fn calculate_basic_score(name: &str, filename_stem: &str) -> i32 {
    let mut score = 0;

    // Bonus for public (not starting with _)
    if !name.starts_with('_') {
        score += 50;
    }

    // Bonus for filename match
    let name_lower = name.to_lowercase();
    if name_lower == filename_stem {
        score += 40;
    } else if name_lower.contains(filename_stem) || filename_stem.contains(&name_lower) {
        score += 20;
    }

    // Penalty for test functions
    if name.starts_with("test_") || name.starts_with("Test") {
        score -= 30;
    }

    score
}

/// Return true if the file path looks like a Python test file.
///
/// Matches:
///   - `test_*.py` / `*_test.py` filename patterns
///   - Any `.py` file inside a `tests/` or `test/` directory
fn is_test_file_path(path: &str) -> bool {
    let path_lower = path.replace('\\', "/").to_lowercase();
    // Directory component: .../tests/... or .../test/... (or starts with tests/ or test/)
    if path_lower.contains("/tests/")
        || path_lower.contains("/test/")
        || path_lower.starts_with("tests/")
        || path_lower.starts_with("test/")
    {
        return true;
    }
    // Filename pattern: extract the last segment
    if let Some(filename) = path_lower.split('/').next_back() {
        let stem = filename.strip_suffix(".py").unwrap_or(filename);
        if stem.starts_with("test_") || stem.ends_with("_test") {
            return true;
        }
    }
    false
}

/// Check if a decorated definition has important decorators
fn has_important_decorator(node: &Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "decorator" {
            let text = get_node_text(&child, source).to_lowercase();
            // Important decorators that indicate primary symbols
            if text.contains("dataclass")
                || text.contains("app.route")
                || text.contains("router")
                || text.contains("api")
                || text.contains("endpoint")
                || text.contains("pytest")
                || text.contains("fixture")
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_stem() {
        assert_eq!(extract_filename_stem("/path/to/models.py"), "models");
        assert_eq!(extract_filename_stem("utils.py"), "utils");
        assert_eq!(extract_filename_stem("__init__.py"), "__init__");
    }

    #[test]
    fn test_calculate_basic_score() {
        // Public symbol should beat private
        let pub_score = calculate_basic_score("User", "user");
        let priv_score = calculate_basic_score("_helper", "user");
        assert!(pub_score > priv_score);

        // Filename match bonus
        let match_score = calculate_basic_score("User", "user");
        let no_match = calculate_basic_score("Helper", "user");
        assert!(match_score > no_match);

        // Test functions should be penalized
        let test_score = calculate_basic_score("test_user", "user");
        let normal_score = calculate_basic_score("create_user", "user");
        assert!(normal_score > test_score);
    }
}
