//! Semantic extraction orchestration
//!
//! This module coordinates the extraction of semantic information from parsed
//! source files using language-specific detectors.

use crate::utils::truncate_to_char_boundary;
use std::path::Path;

/// Maximum length for raw source fallback when extraction is incomplete
const MAX_FALLBACK_LEN: usize = 1000;
use tree_sitter::Tree;

use crate::error::Result;
use crate::lang::Lang;
use crate::risk::calculate_risk;
use crate::schema::{SemanticSummary, SymbolId};

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
    // Vue SFCs need special handling - extract script section first
    if lang.is_vue_sfc() {
        crate::detectors::javascript::extract_vue_sfc(&mut summary, source)?;
    } else {
        match lang.family() {
            crate::lang::LangFamily::JavaScript => {
                crate::detectors::javascript::extract(&mut summary, source, tree, lang)?;
            }
            crate::lang::LangFamily::Rust => {
                crate::detectors::rust::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Python => {
                crate::detectors::python::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Go => {
                crate::detectors::go::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Java => {
                crate::detectors::java::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::CSharp => {
                crate::detectors::csharp::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Kotlin => {
                crate::detectors::kotlin::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::CFamily => {
                crate::detectors::c_family::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Markup => {
                crate::detectors::markup::extract(&mut summary, source, tree, lang)?;
            }
            crate::lang::LangFamily::Config => {
                crate::detectors::config::extract(&mut summary, source, tree, lang)?;
            }
            crate::lang::LangFamily::Shell => {
                crate::detectors::shell::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Gradle => {
                crate::detectors::gradle::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Hcl => {
                crate::detectors::hcl::extract(&mut summary, source, tree)?;
            }
            crate::lang::LangFamily::Dockerfile => {
                crate::detectors::dockerfile::extract(&mut summary, source, tree)?;
            }
        }
    }

    // Reorder insertions: put state hooks last per spec
    reorder_insertions(&mut summary.insertions);

    // Calculate risk score
    summary.behavioral_risk = calculate_risk(&summary);

    // Mark extraction as complete if we got meaningful semantic info
    summary.extraction_complete = summary.symbol.is_some()
        || !summary.insertions.is_empty()
        || !summary.calls.is_empty()
        || !summary.added_dependencies.is_empty();

    // Generate stable symbol ID for cross-commit tracking
    summary.symbol_id = SymbolId::from_summary(&summary);

    // Add raw fallback if extraction was incomplete
    if !summary.extraction_complete {
        // Truncate source for fallback if too long (UTF-8 safe)
        if source.len() > MAX_FALLBACK_LEN {
            let truncated = truncate_to_char_boundary(source, MAX_FALLBACK_LEN);
            summary.raw_fallback = Some(format!("{}...", truncated));
        } else {
            summary.raw_fallback = Some(source.to_string());
        }
    }

    Ok(summary)
}

// ============================================================================
// Utility functions
// ============================================================================

/// Reorder insertions to put state hooks last (per plan.md spec)
fn reorder_insertions(insertions: &mut Vec<String>) {
    // Separate state hook insertions from others
    let (state_hooks, others): (Vec<_>, Vec<_>) =
        insertions.drain(..).partition(|s| s.contains("state via"));

    // Put UI structure first, state hooks last
    insertions.extend(others);
    insertions.extend(state_hooks);
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
        // Exported function = public surface
        assert!(summary.public_surface_changed);
        assert!(!summary.added_dependencies.is_empty());
    }

    #[test]
    fn test_jsx_component_calls_in_summary() {
        let source = r#"
import { Button } from './ui';
import Header from './Header';
import { Icons } from './icons';

export function Page() {
    return (
        <div>
            <Header />
            <Button>Click</Button>
            <Button>Another</Button>
            <Icons.Home />
        </div>
    );
}
"#;

        let tree = parse_source(source, Lang::Tsx);
        let path = PathBuf::from("Page.tsx");
        let summary = extract(&path, source, &tree, Lang::Tsx).unwrap();

        // Verify PascalCase components appear in calls
        let call_names: Vec<&str> = summary.calls.iter().map(|c| c.name.as_str()).collect();
        assert!(
            call_names.contains(&"Header"),
            "Expected Header in calls, got: {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"Button"),
            "Expected Button in calls, got: {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"Icons.Home"),
            "Expected Icons.Home in calls, got: {:?}",
            call_names
        );

        // Verify lowercase HTML elements are NOT in calls
        assert!(
            !call_names.contains(&"div"),
            "div (HTML element) should not be in calls"
        );
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
        // pub fn = public surface
        assert!(summary.public_surface_changed);
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
        // Python: name without leading _ = public
        assert!(summary.public_surface_changed);
        assert!(!summary.added_dependencies.is_empty());
    }

    #[test]
    fn test_truncate_to_char_boundary() {
        // ASCII - should work normally
        assert_eq!(truncate_to_char_boundary("hello", 3), "hel");
        assert_eq!(truncate_to_char_boundary("hello", 10), "hello");

        // UTF-8 multi-byte chars - should find safe boundary
        let emoji_str = "Hello ⚠️ World"; // ⚠️ is multi-byte
        let truncated = truncate_to_char_boundary(emoji_str, 8);
        assert!(truncated.len() <= 8);
        assert!(truncated.is_char_boundary(truncated.len()));

        // Japanese characters (3 bytes each)
        let japanese = "こんにちは"; // 5 chars, 15 bytes
        let truncated = truncate_to_char_boundary(japanese, 7);
        assert!(truncated.len() <= 7);
        // Should truncate to 2 chars = 6 bytes
        assert_eq!(truncated, "こん");
    }
}
