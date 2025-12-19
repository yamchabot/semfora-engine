//! Markup language detector (HTML, CSS, SCSS, Markdown)

use crate::error::Result;
use crate::lang::Lang;
use crate::schema::SemanticSummary;
use tree_sitter::Tree;

pub fn extract(
    summary: &mut SemanticSummary,
    _source: &str,
    _tree: &Tree,
    lang: Lang,
) -> Result<()> {
    // Markup files have simpler extraction - mainly structure
    // For now, just mark as complete with the file info

    // Add language-specific insertion
    match lang {
        Lang::Html => {
            summary.insertions.push("HTML document".to_string());
        }
        Lang::Css => {
            summary.insertions.push("CSS stylesheet".to_string());
        }
        Lang::Scss => {
            summary.insertions.push("SCSS stylesheet".to_string());
        }
        Lang::Markdown => {
            summary.insertions.push("Markdown document".to_string());
        }
        _ => {}
    }

    summary.extraction_complete = true;
    Ok(())
}
