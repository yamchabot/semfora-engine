//! Custom assertions for integration tests
//!
//! Provides helper functions for validating CLI output, symbol extraction,
//! and format consistency across text, TOON, and JSON formats.

#![allow(clippy::manual_strip)]

use serde_json::Value;

/// Assert that output is valid JSON and return parsed value
pub fn assert_valid_json(output: &str, context: &str) -> Value {
    serde_json::from_str(output).unwrap_or_else(|e| {
        panic!(
            "Expected valid JSON ({}): {}\nOutput:\n{}",
            context, e, output
        )
    })
}

/// Assert that output contains valid TOON markers
pub fn assert_valid_toon(output: &str, context: &str) {
    assert!(
        output.contains("_type:"),
        "Expected TOON output to contain '_type:' marker ({})\nOutput:\n{}",
        context,
        output
    );
}

/// Assert that JSON output has expected type
pub fn assert_json_type(json: &Value, expected_type: &str) {
    let actual_type = json["_type"]
        .as_str()
        .unwrap_or_else(|| panic!("JSON missing '_type' field"));
    assert_eq!(
        actual_type, expected_type,
        "Expected JSON type '{}' but got '{}'",
        expected_type, actual_type
    );
}

/// Assert that a symbol with given name exists in JSON output
pub fn assert_symbol_exists(json: &Value, symbol_name: &str) {
    let found = find_symbol_in_json(json, symbol_name);
    assert!(
        found,
        "Expected to find symbol '{}' in output:\n{}",
        symbol_name,
        serde_json::to_string_pretty(json).unwrap()
    );
}

/// Assert that a symbol with given name does NOT exist in JSON output
pub fn assert_symbol_not_exists(json: &Value, symbol_name: &str) {
    let found = find_symbol_in_json(json, symbol_name);
    assert!(
        !found,
        "Expected NOT to find symbol '{}' in output",
        symbol_name
    );
}

/// Find a symbol by name in JSON output (searches recursively)
pub fn find_symbol_in_json(json: &Value, symbol_name: &str) -> bool {
    match json {
        Value::Object(obj) => {
            // Check if this object has a "name" field matching
            if let Some(name) = obj.get("name") {
                if name.as_str() == Some(symbol_name) {
                    return true;
                }
            }
            // Recurse into all values
            obj.values().any(|v| find_symbol_in_json(v, symbol_name))
        }
        Value::Array(arr) => arr.iter().any(|v| find_symbol_in_json(v, symbol_name)),
        Value::String(s) => s == symbol_name,
        _ => false,
    }
}

/// Extract all symbol names from JSON output
pub fn extract_symbol_names(json: &Value) -> Vec<String> {
    let mut names = Vec::new();
    extract_symbol_names_recursive(json, &mut names);
    names
}

fn extract_symbol_names_recursive(json: &Value, names: &mut Vec<String>) {
    match json {
        Value::Object(obj) => {
            if let Some(Value::String(name)) = obj.get("name") {
                // Only add if it looks like a symbol entry (has other expected fields)
                if obj.contains_key("kind")
                    || obj.contains_key("hash")
                    || obj.contains_key("module")
                {
                    names.push(name.clone());
                }
            }
            for v in obj.values() {
                extract_symbol_names_recursive(v, names);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                extract_symbol_names_recursive(v, names);
            }
        }
        _ => {}
    }
}

/// Extract symbol hashes from JSON output
pub fn extract_symbol_hashes(json: &Value) -> Vec<String> {
    let mut hashes = Vec::new();
    extract_symbol_hashes_recursive(json, &mut hashes);
    hashes
}

fn extract_symbol_hashes_recursive(json: &Value, hashes: &mut Vec<String>) {
    match json {
        Value::Object(obj) => {
            if let Some(Value::String(hash)) = obj.get("hash") {
                hashes.push(hash.clone());
            }
            if let Some(Value::String(hash)) = obj.get("symbol_hash") {
                hashes.push(hash.clone());
            }
            for v in obj.values() {
                extract_symbol_hashes_recursive(v, hashes);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                extract_symbol_hashes_recursive(v, hashes);
            }
        }
        _ => {}
    }
}

/// Assert that a symbol is marked as exported/public
pub fn assert_symbol_exported(json: &Value, symbol_name: &str) {
    let exported = check_symbol_visibility(json, symbol_name, true);
    assert!(
        exported,
        "Expected symbol '{}' to be exported/public",
        symbol_name
    );
}

/// Assert that a symbol is NOT exported (private)
pub fn assert_symbol_private(json: &Value, symbol_name: &str) {
    let exported = check_symbol_visibility(json, symbol_name, true);
    assert!(
        !exported,
        "Expected symbol '{}' to be private (not exported)",
        symbol_name
    );
}

fn check_symbol_visibility(json: &Value, symbol_name: &str, looking_for_exported: bool) -> bool {
    match json {
        Value::Object(obj) => {
            // Check if this is the symbol we're looking for
            if let Some(Value::String(name)) = obj.get("name") {
                if name == symbol_name {
                    // Check visibility fields
                    if let Some(Value::Bool(exported)) = obj.get("is_exported") {
                        return *exported == looking_for_exported;
                    }
                    if let Some(Value::Bool(public)) = obj.get("is_public") {
                        return *public == looking_for_exported;
                    }
                    if let Some(Value::String(vis)) = obj.get("visibility") {
                        let is_public = vis == "public" || vis == "exported";
                        return is_public == looking_for_exported;
                    }
                }
            }
            // Recurse
            obj.values()
                .any(|v| check_symbol_visibility(v, symbol_name, looking_for_exported))
        }
        Value::Array(arr) => arr
            .iter()
            .any(|v| check_symbol_visibility(v, symbol_name, looking_for_exported)),
        _ => false,
    }
}

/// Assert module count in overview JSON
pub fn assert_module_count(json: &Value, expected_count: usize) {
    let actual = count_modules(json);
    assert_eq!(
        actual, expected_count,
        "Expected {} modules, found {}",
        expected_count, actual
    );
}

fn count_modules(json: &Value) -> usize {
    // Look for modules array
    if let Some(Value::Array(modules)) = json.get("modules") {
        return modules.len();
    }
    // Or count module entries
    if let Some(Value::Object(obj)) = json.get("modules") {
        return obj.len();
    }
    0
}

/// Assert that output contains a specific string (case-insensitive option)
pub fn assert_contains(output: &str, needle: &str, case_sensitive: bool, context: &str) {
    let found = if case_sensitive {
        output.contains(needle)
    } else {
        output.to_lowercase().contains(&needle.to_lowercase())
    };
    assert!(
        found,
        "Expected output to contain '{}' ({})\nOutput:\n{}",
        needle, context, output
    );
}

/// Assert that output does NOT contain a specific string
pub fn assert_not_contains(output: &str, needle: &str, context: &str) {
    assert!(
        !output.contains(needle),
        "Expected output NOT to contain '{}' ({})\nOutput:\n{}",
        needle,
        context,
        output
    );
}

/// Assert that a call graph edge exists (caller -> callee)
pub fn assert_call_edge_exists(json: &Value, caller: &str, callee: &str) {
    let found = find_call_edge(json, caller, callee);
    assert!(
        found,
        "Expected call edge from '{}' to '{}' in call graph",
        caller, callee
    );
}

fn find_call_edge(json: &Value, caller: &str, callee: &str) -> bool {
    match json {
        Value::Object(obj) => {
            // Check for edges array
            if let Some(Value::Array(edges)) = obj.get("edges") {
                for edge in edges {
                    if let Some(Value::String(from)) = edge.get("from") {
                        if let Some(Value::String(to)) = edge.get("to") {
                            if from.contains(caller) && to.contains(callee) {
                                return true;
                            }
                        }
                    }
                }
            }
            // Check for calls array within symbols
            if let Some(Value::String(name)) = obj.get("name") {
                if name.contains(caller) {
                    if let Some(Value::Array(calls)) = obj.get("calls") {
                        for call in calls {
                            if let Value::String(call_name) = call {
                                if call_name.contains(callee) {
                                    return true;
                                }
                            }
                            if let Some(Value::String(call_name)) = call.get("name") {
                                if call_name.contains(callee) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            obj.values().any(|v| find_call_edge(v, caller, callee))
        }
        Value::Array(arr) => arr.iter().any(|v| find_call_edge(v, caller, callee)),
        _ => false,
    }
}

/// Assert duplicate cluster exists with expected similarity
pub fn assert_duplicate_cluster_exists(json: &Value, min_similarity: f64) {
    let found = find_duplicate_cluster(json, min_similarity);
    assert!(
        found,
        "Expected duplicate cluster with similarity >= {}",
        min_similarity
    );
}

fn find_duplicate_cluster(json: &Value, min_similarity: f64) -> bool {
    match json {
        Value::Object(obj) => {
            // Check for duplicates array
            if let Some(Value::Array(clusters)) = obj.get("clusters") {
                for cluster in clusters {
                    if let Some(Value::Array(duplicates)) = cluster.get("duplicates") {
                        for dup in duplicates {
                            if let Some(Value::Number(sim)) = dup.get("similarity") {
                                if let Some(s) = sim.as_f64() {
                                    if s >= min_similarity {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            obj.values()
                .any(|v| find_duplicate_cluster(v, min_similarity))
        }
        Value::Array(arr) => arr
            .iter()
            .any(|v| find_duplicate_cluster(v, min_similarity)),
        _ => false,
    }
}

/// Parse TOON output into key-value pairs (simple parsing)
pub fn parse_toon_simple(output: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            map.insert(key, value);
        }
    }
    map
}

/// Get TOON type from output
pub fn get_toon_type(output: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("_type:") {
            return Some(line[6..].trim().trim_matches('"').to_string());
        }
    }
    None
}
