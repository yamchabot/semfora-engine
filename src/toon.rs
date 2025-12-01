//! TOON (Token-Oriented Object Notation) encoder
//!
//! TOON encoding rules from specification:
//! - Objects → indented blocks
//! - Uniform arrays → tabular blocks
//! - Strings quoted only if necessary
//! - Field headers emitted once per array
//! - Stable field ordering enforced

use std::fmt::Write;

use crate::schema::{RiskLevel, SemanticSummary};

/// Encode a semantic summary as TOON
pub fn encode_toon(summary: &SemanticSummary) -> String {
    let mut out = String::new();

    // Simple scalar fields
    writeln!(out, "file: {}", summary.file).unwrap();
    writeln!(out, "language: {}", summary.language).unwrap();

    if let Some(ref sym) = summary.symbol {
        writeln!(out, "symbol: {}", sym).unwrap();
    }

    if let Some(ref kind) = summary.symbol_kind {
        writeln!(out, "symbol_kind: {}", kind.as_str()).unwrap();
    }

    if let Some(ref ret) = summary.return_type {
        writeln!(out, "return_type: {}", quote_if_needed(ret)).unwrap();
    }

    writeln!(
        out,
        "public_surface_changed: {}",
        summary.public_surface_changed
    )
    .unwrap();

    writeln!(
        out,
        "behavioral_risk: {}",
        risk_to_string(summary.behavioral_risk)
    )
    .unwrap();

    out.push('\n');

    // Insertions array (indented block format)
    if !summary.insertions.is_empty() {
        writeln!(out, "insertions[{}]:", summary.insertions.len()).unwrap();
        for item in &summary.insertions {
            writeln!(out, "  {}", item).unwrap();
        }
        out.push('\n');
    }

    // Added dependencies (inline array format)
    if !summary.added_dependencies.is_empty() {
        writeln!(
            out,
            "added_dependencies[{}]: {}",
            summary.added_dependencies.len(),
            summary.added_dependencies.join(",")
        )
        .unwrap();
        out.push('\n');
    }

    // State changes (tabular format)
    if !summary.state_changes.is_empty() {
        writeln!(
            out,
            "state_changes[{}]{{name,type,initializer}}:",
            summary.state_changes.len()
        )
        .unwrap();
        for state in &summary.state_changes {
            writeln!(
                out,
                "  {},{},{}",
                state.name,
                state.state_type,
                quote_if_needed(&state.initializer)
            )
            .unwrap();
        }
        out.push('\n');
    }

    // Arguments (tabular format)
    if !summary.arguments.is_empty() {
        writeln!(
            out,
            "arguments[{}]{{name,type,default}}:",
            summary.arguments.len()
        )
        .unwrap();
        for arg in &summary.arguments {
            writeln!(
                out,
                "  {},{},{}",
                arg.name,
                arg.arg_type.as_deref().unwrap_or("_"),
                arg.default_value.as_deref().unwrap_or("_")
            )
            .unwrap();
        }
        out.push('\n');
    }

    // Props (tabular format)
    if !summary.props.is_empty() {
        writeln!(
            out,
            "props[{}]{{name,type,default,required}}:",
            summary.props.len()
        )
        .unwrap();
        for prop in &summary.props {
            writeln!(
                out,
                "  {},{},{},{}",
                prop.name,
                prop.prop_type.as_deref().unwrap_or("_"),
                prop.default_value.as_deref().unwrap_or("_"),
                prop.required
            )
            .unwrap();
        }
        out.push('\n');
    }

    // Control flow changes (inline array)
    if !summary.control_flow_changes.is_empty() {
        let kinds: Vec<_> = summary
            .control_flow_changes
            .iter()
            .map(|c| c.kind.as_str())
            .collect();
        writeln!(out, "control_flow[{}]: {}", kinds.len(), kinds.join(",")).unwrap();
        out.push('\n');
    }

    // Function calls with context (deduplicated, counted)
    if !summary.calls.is_empty() {
        // Deduplicate calls by (name, object, awaited, in_try) and count occurrences
        let mut call_counts: std::collections::HashMap<(String, String, bool, bool), usize> =
            std::collections::HashMap::new();

        for call in &summary.calls {
            let key = (
                call.name.clone(),
                call.object.clone().unwrap_or_default(),
                call.is_awaited,
                call.in_try,
            );
            *call_counts.entry(key).or_insert(0) += 1;
        }

        // Convert to sorted vec for deterministic output
        let mut unique_calls: Vec<_> = call_counts.into_iter().collect();
        unique_calls.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(&b.0.0))); // Sort by count desc, then name

        writeln!(
            out,
            "calls[{}]{{name,obj,await,try,count}}:",
            unique_calls.len()
        )
        .unwrap();

        for ((name, obj, awaited, in_try), count) in unique_calls {
            let obj_str = if obj.is_empty() { "_" } else { &obj };
            let awaited_str = if awaited { "Y" } else { "_" };
            let in_try_str = if in_try { "Y" } else { "_" };
            let count_str = if count > 1 { format!("{}", count) } else { "_".to_string() };
            writeln!(out, "  {},{},{},{},{}", name, obj_str, awaited_str, in_try_str, count_str).unwrap();
        }
        out.push('\n');
    }

    // Safety fallback - only include if truly needed (no extraction at all)
    // Skip raw_fallback for config files that were successfully parsed
    if let Some(ref raw) = summary.raw_fallback {
        // Only output raw if we have no semantic data at all
        if summary.added_dependencies.is_empty()
            && summary.calls.is_empty()
            && summary.state_changes.is_empty()
            && summary.control_flow_changes.is_empty()
            && summary.symbol.is_none()
        {
            // Use TOON-style block (indented, no markdown)
            out.push_str("raw_source:\n");
            for line in raw.lines().take(20) {  // Limit to 20 lines
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            if raw.lines().count() > 20 {
                out.push_str("  ...(truncated)\n");
            }
        }
    }

    out
}

/// Convert risk level to string
fn risk_to_string(risk: RiskLevel) -> &'static str {
    risk.as_str()
}

/// Quote a string if it contains special characters
fn quote_if_needed(s: &str) -> String {
    if s.contains(' ')
        || s.contains(':')
        || s.contains(',')
        || s.contains('\n')
        || s.contains('"')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ControlFlowChange, ControlFlowKind, Location, StateChange, SymbolKind};

    #[test]
    fn test_basic_toon_output() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("AppLayout".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            return_type: Some("JSX.Element".to_string()),
            public_surface_changed: false,
            behavioral_risk: RiskLevel::Medium,
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("file: test.tsx"));
        assert!(toon.contains("language: tsx"));
        assert!(toon.contains("symbol: AppLayout"));
        assert!(toon.contains("symbol_kind: component"));
        assert!(toon.contains("return_type: JSX.Element"));
        assert!(toon.contains("public_surface_changed: false"));
        assert!(toon.contains("behavioral_risk: medium"));
    }

    #[test]
    fn test_insertions_format() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            insertions: vec![
                "header container with nav".to_string(),
                "6 route links".to_string(),
            ],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("insertions[2]:"));
        assert!(toon.contains("  header container with nav"));
        assert!(toon.contains("  6 route links"));
    }

    #[test]
    fn test_state_changes_tabular() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            state_changes: vec![StateChange {
                name: "open".to_string(),
                state_type: "boolean".to_string(),
                initializer: "false".to_string(),
            }],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("state_changes[1]{name,type,initializer}:"));
        assert!(toon.contains("  open,boolean,false"));
    }

    #[test]
    fn test_dependencies_inline() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            added_dependencies: vec!["useState".to_string(), "Link".to_string()],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("added_dependencies[2]: useState,Link"));
    }

    #[test]
    fn test_control_flow_inline() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            control_flow_changes: vec![
                ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                },
                ControlFlowChange {
                    kind: ControlFlowKind::For,
                    location: Location::default(),
                },
            ],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("control_flow[2]: if,for"));
    }

    #[test]
    fn test_raw_fallback() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            raw_fallback: Some("function foo() {}".to_string()),
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("RAW BLOCK:"));
        assert!(toon.contains("```"));
        assert!(toon.contains("function foo() {}"));
    }

    #[test]
    fn test_quote_if_needed() {
        assert_eq!(quote_if_needed("simple"), "simple");
        assert_eq!(quote_if_needed("has space"), "\"has space\"");
        assert_eq!(quote_if_needed("has:colon"), "\"has:colon\"");
        assert_eq!(quote_if_needed("has,comma"), "\"has,comma\"");
    }
}
