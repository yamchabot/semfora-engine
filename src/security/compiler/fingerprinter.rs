//! Fingerprint generator for vulnerable code patterns
//!
//! This module generates fingerprints from AST analysis of vulnerable code,
//! using the same algorithm as duplicate detection for compatibility.

use crate::error::Result;
use crate::lang::Lang;
use crate::security::compiler::commit_parser::VulnerableCode;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Generated fingerprints for a vulnerable code pattern
#[derive(Debug, Clone, Default)]
pub struct Fingerprints {
    /// Hash of call sequence
    pub call: u64,
    /// Hash of control flow pattern
    pub control_flow: u64,
    /// Hash of state mutation pattern
    pub state: u64,
}

/// Result of fingerprint generation
#[derive(Debug, Clone)]
pub struct FingerprintResult {
    /// The fingerprints
    pub fingerprints: Fingerprints,
    /// Extracted dangerous call names
    pub calls: Vec<String>,
    /// Control flow pattern string
    pub control_flow_pattern: String,
    /// State variable patterns
    pub state_patterns: Vec<String>,
}

/// Generate fingerprints from vulnerable code blocks
pub fn generate_fingerprints(code_blocks: &[VulnerableCode]) -> Result<FingerprintResult> {
    let mut all_calls = Vec::new();
    let mut all_control_flow = Vec::new();
    let mut all_state = Vec::new();

    for block in code_blocks {
        let (calls, control_flow, state) = analyze_code(&block.source, block.language);
        all_calls.extend(calls);
        all_control_flow.extend(control_flow);
        all_state.extend(state);
    }

    // Deduplicate and sort
    all_calls.sort();
    all_calls.dedup();
    all_state.sort();
    all_state.dedup();

    // Generate fingerprints using FNV-1a style hashing
    let call_fingerprint = compute_set_fingerprint(&all_calls);
    let control_flow_pattern = all_control_flow.join("");
    let control_flow_fingerprint = compute_string_fingerprint(&control_flow_pattern);
    let state_fingerprint = compute_set_fingerprint(&all_state);

    Ok(FingerprintResult {
        fingerprints: Fingerprints {
            call: call_fingerprint,
            control_flow: control_flow_fingerprint,
            state: state_fingerprint,
        },
        calls: all_calls,
        control_flow_pattern,
        state_patterns: all_state,
    })
}

/// Analyze code to extract calls, control flow, and state patterns
fn analyze_code(source: &str, lang: Lang) -> (Vec<String>, Vec<String>, Vec<String>) {
    let calls = extract_calls(source, lang);
    let control_flow = extract_control_flow(source, lang);
    let state = extract_state_patterns(source, lang);
    (calls, control_flow, state)
}

/// Extract function/method calls from source code
fn extract_calls(source: &str, lang: Lang) -> Vec<String> {
    let mut calls = Vec::new();

    // Regex-based extraction (simplified - full implementation would use tree-sitter)
    for line in source.lines() {
        let trimmed = line.trim();

        // Extract function calls: name(...)
        let mut chars = trimmed.chars().peekable();
        let mut current_name = String::new();

        while let Some(c) = chars.next() {
            if c.is_alphanumeric() || c == '_' || c == '.' || c == ':' {
                current_name.push(c);
            } else if c == '(' && !current_name.is_empty() {
                // Found a call
                let call_name = current_name.trim_matches('.').trim_matches(':');

                // Filter out common non-dangerous calls
                if !is_utility_call(call_name) && !call_name.is_empty() {
                    calls.push(normalize_call_name(call_name, lang));
                }
                current_name.clear();
            } else {
                current_name.clear();
            }
        }
    }

    calls
}

/// Extract control flow patterns
fn extract_control_flow(source: &str, lang: Lang) -> Vec<String> {
    let mut patterns = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        match lang {
            Lang::JavaScript | Lang::TypeScript | Lang::Jsx | Lang::Tsx => {
                if trimmed.starts_with("if ") || trimmed.starts_with("if(") {
                    patterns.push("I".to_string());
                } else if trimmed.starts_with("else if") {
                    patterns.push("E".to_string());
                } else if trimmed.starts_with("else") {
                    patterns.push("L".to_string());
                } else if trimmed.starts_with("for ") || trimmed.starts_with("for(") {
                    patterns.push("F".to_string());
                } else if trimmed.starts_with("while ") || trimmed.starts_with("while(") {
                    patterns.push("W".to_string());
                } else if trimmed.starts_with("try ") || trimmed.starts_with("try{") {
                    patterns.push("T".to_string());
                } else if trimmed.starts_with("catch")
                    || trimmed.contains("} catch")
                    || trimmed.contains("}catch")
                {
                    patterns.push("C".to_string());
                } else if trimmed.starts_with("finally")
                    || trimmed.contains("} finally")
                    || trimmed.contains("}finally")
                {
                    patterns.push("Y".to_string());
                } else if trimmed.starts_with("switch") {
                    patterns.push("S".to_string());
                } else if trimmed.starts_with("case ") {
                    patterns.push("A".to_string());
                }
            }
            Lang::Python => {
                if trimmed.starts_with("if ") {
                    patterns.push("I".to_string());
                } else if trimmed.starts_with("elif ") {
                    patterns.push("E".to_string());
                } else if trimmed.starts_with("else:") {
                    patterns.push("L".to_string());
                } else if trimmed.starts_with("for ") {
                    patterns.push("F".to_string());
                } else if trimmed.starts_with("while ") {
                    patterns.push("W".to_string());
                } else if trimmed.starts_with("try:") {
                    patterns.push("T".to_string());
                } else if trimmed.starts_with("except") {
                    patterns.push("C".to_string());
                } else if trimmed.starts_with("finally:") {
                    patterns.push("Y".to_string());
                }
            }
            Lang::Rust => {
                if trimmed.starts_with("if ") {
                    patterns.push("I".to_string());
                } else if trimmed.starts_with("else if") {
                    patterns.push("E".to_string());
                } else if trimmed.starts_with("else") {
                    patterns.push("L".to_string());
                } else if trimmed.starts_with("for ") {
                    patterns.push("F".to_string());
                } else if trimmed.starts_with("while ") {
                    patterns.push("W".to_string());
                } else if trimmed.starts_with("loop ") || trimmed == "loop" || trimmed == "loop {" {
                    patterns.push("O".to_string());
                } else if trimmed.starts_with("match ") {
                    patterns.push("M".to_string());
                } else if trimmed.contains("?") || trimmed.contains(".unwrap()") {
                    patterns.push("U".to_string()); // Unwrap/error propagation
                }
            }
            Lang::Java | Lang::CSharp => {
                if trimmed.starts_with("if ") || trimmed.starts_with("if(") {
                    patterns.push("I".to_string());
                } else if trimmed.starts_with("else if") {
                    patterns.push("E".to_string());
                } else if trimmed.starts_with("else") {
                    patterns.push("L".to_string());
                } else if trimmed.starts_with("for ") || trimmed.starts_with("for(") {
                    patterns.push("F".to_string());
                } else if trimmed.starts_with("while ") || trimmed.starts_with("while(") {
                    patterns.push("W".to_string());
                } else if trimmed.starts_with("try ") || trimmed.starts_with("try{") {
                    patterns.push("T".to_string());
                } else if trimmed.starts_with("catch")
                    || trimmed.contains("} catch")
                    || trimmed.contains("}catch")
                {
                    patterns.push("C".to_string());
                } else if trimmed.starts_with("finally")
                    || trimmed.contains("} finally")
                    || trimmed.contains("}finally")
                {
                    patterns.push("Y".to_string());
                } else if trimmed.starts_with("switch") {
                    patterns.push("S".to_string());
                }
            }
            _ => {}
        }
    }

    patterns
}

/// Extract state mutation patterns
fn extract_state_patterns(source: &str, _lang: Lang) -> Vec<String> {
    let mut patterns = Vec::new();

    // Look for common dangerous variable patterns
    let dangerous_patterns = [
        "userInput",
        "user_input",
        "query",
        "sql",
        "cmd",
        "command",
        "exec",
        "eval",
        "input",
        "request",
        "req",
        "params",
        "body",
        "data",
        "raw",
        "unsafe",
        "untrusted",
        "external",
        "payload",
        "args",
        "argv",
    ];

    for line in source.lines() {
        let lower = line.to_lowercase();
        for pattern in &dangerous_patterns {
            if lower.contains(pattern) {
                patterns.push(pattern.to_string());
            }
        }

        // Look for string concatenation with variables (common in injection vulnerabilities)
        if lower.contains(" + ")
            && (lower.contains("\"") || lower.contains("'") || lower.contains("`"))
        {
            patterns.push("string_concat".to_string());
        }

        // Template literals with variables
        if lower.contains("${") {
            patterns.push("template_literal".to_string());
        }

        // f-strings in Python
        if lower.contains("f\"") || lower.contains("f'") {
            patterns.push("f_string".to_string());
        }

        // format! in Rust
        if lower.contains("format!") {
            patterns.push("format_macro".to_string());
        }
    }

    patterns
}

/// Check if a call is a utility function (not business logic)
fn is_utility_call(name: &str) -> bool {
    let utilities = [
        "console",
        "log",
        "debug",
        "info",
        "warn",
        "error",
        "print",
        "println",
        "trace",
        "len",
        "length",
        "size",
        "toString",
        "to_string",
        "clone",
        "map",
        "filter",
        "reduce",
        "forEach",
        "for_each",
        "push",
        "pop",
        "append",
        "extend",
        "iter",
        "into_iter",
        "collect",
        "unwrap",
        "expect",
        "ok",
        "err",
        "Some",
        "None",
        "Ok",
        "Err",
    ];

    utilities
        .iter()
        .any(|u| name == *u || name.ends_with(&format!(".{}", u)))
}

/// Normalize call name for consistent fingerprinting
fn normalize_call_name(name: &str, _lang: Lang) -> String {
    // Extract the actual function name from method chains
    // e.g., "db.query" -> "query", "conn.execute" -> "execute"
    name.rsplit('.').next().unwrap_or(name).to_string()
}

/// Compute fingerprint for a set of strings (order-independent)
fn compute_set_fingerprint(items: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Sort for consistency
    let mut sorted: Vec<_> = items.iter().collect();
    sorted.sort();

    for item in sorted {
        item.hash(&mut hasher);
    }

    hasher.finish()
}

/// Compute fingerprint for a string (order-dependent)
fn compute_string_fingerprint(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Generate fingerprints from a source string (for manual patterns)
pub fn fingerprint_from_source(source: &str, lang: Lang) -> FingerprintResult {
    let block = VulnerableCode {
        language: lang,
        source: source.to_string(),
        file_path: String::new(),
        modified_functions: Vec::new(),
        start_line: 0,
        end_line: 0,
    };

    generate_fingerprints(&[block]).unwrap_or_else(|_| FingerprintResult {
        fingerprints: Fingerprints::default(),
        calls: Vec::new(),
        control_flow_pattern: String::new(),
        state_patterns: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_calls() {
        let source = r#"
            db.query(sql);
            eval(userInput);
            console.log("debug");
        "#;

        let calls = extract_calls(source, Lang::JavaScript);
        assert!(calls.contains(&"query".to_string()));
        assert!(calls.contains(&"eval".to_string()));
        // console.log should be filtered out as utility
        assert!(!calls.contains(&"log".to_string()));
    }

    #[test]
    fn test_extract_control_flow() {
        let source = r#"
            if (condition) {
                try {
                    doSomething();
                } catch (e) {
                    handleError(e);
                }
            }
        "#;

        let patterns = extract_control_flow(source, Lang::JavaScript);
        assert_eq!(patterns, vec!["I", "T", "C"]);
    }

    #[test]
    fn test_extract_state_patterns() {
        let source = r#"
            const sql = "SELECT * FROM users WHERE id = " + userInput;
            db.query(sql);
        "#;

        let patterns = extract_state_patterns(source, Lang::JavaScript);
        assert!(patterns.contains(&"sql".to_string()));
        assert!(patterns.contains(&"string_concat".to_string()));
    }

    #[test]
    fn test_compute_set_fingerprint() {
        let set1 = vec!["a".to_string(), "b".to_string()];
        let set2 = vec!["b".to_string(), "a".to_string()];

        // Order shouldn't matter
        assert_eq!(
            compute_set_fingerprint(&set1),
            compute_set_fingerprint(&set2)
        );
    }
}
