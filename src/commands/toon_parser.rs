//! TOON Parser Module - Centralized parsing for cached TOON files
//!
//! This module provides utilities to parse TOON-formatted cache files
//! and convert them to JSON for flexible output formatting.
//!
//! ## TOON Format Overview
//!
//! TOON (Token-Oriented Object Notation) is a compact format optimized for AI consumption:
//! - `key: value` - Simple key-value pairs
//! - `key[N]: val1,val2` - Arrays with count
//! - `key[N]{fields}: \n  row1\n  row2` - Table format (symbols list)
//! - `_type: xxx` - Type identifier
//!
//! ## Usage
//!
//! ```rust,ignore
//! use semfora_engine::commands::toon_parser::{parse_toon_to_json, read_cached_toon};
//!
//! // Parse TOON string to JSON
//! let toon_content = "_type: test\nname: hello";
//! let json = parse_toon_to_json(toon_content);
//!
//! // Read and parse a cached file
//! let json = read_cached_toon(&path)?;
//! ```

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{McpDiffError, Result};

/// Parse TOON format content to a JSON Value
///
/// This handles the standard TOON format used in cached files:
/// - repo_overview.toon
/// - modules/*.toon
/// - symbols/*.toon
pub fn parse_toon_to_json(content: &str) -> Value {
    let mut result = Map::new();
    let mut current_table: Option<(String, Vec<String>, Vec<Value>)> = None;
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Check if we're in a table and this is a data row (starts with spaces)
        if let Some((ref table_name, ref fields, ref mut rows)) = current_table {
            // Table rows start with 2+ spaces - trust the indentation when in table context
            // Note: table data may contain colons (e.g., hash values like "47a37672:0b24ec7dc888d272")
            if lines[i].starts_with("  ") {
                // This is a table data row
                let row = parse_table_row(line, fields);
                rows.push(row);
                i += 1;
                continue;
            } else {
                // End of table - save it and continue processing this line
                result.insert(table_name.clone(), Value::Array(rows.clone()));
                current_table = None;
            }
        }

        // Parse key: value pairs
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value_part = line[colon_pos + 1..].trim();

            // Check for table format: key[N]{fields}:
            if let Some(bracket_pos) = key.find('[') {
                let base_key = &key[..bracket_pos];
                let rest = &key[bracket_pos..];

                // Check for {fields} pattern
                if let Some(brace_pos) = rest.find('{') {
                    if let Some(brace_end) = rest.find('}') {
                        let fields_str = &rest[brace_pos + 1..brace_end];
                        let fields: Vec<String> = fields_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect();
                        current_table = Some((base_key.to_string(), fields, Vec::new()));
                        i += 1;
                        continue;
                    }
                }

                // Simple array: key[N]: val1,val2
                let arr = parse_array_value(value_part);
                result.insert(base_key.to_string(), arr);
            } else {
                // Simple key: value
                let parsed_value = parse_simple_value(value_part);
                result.insert(key.to_string(), parsed_value);
            }
        }

        i += 1;
    }

    // Save any remaining table
    if let Some((table_name, _, rows)) = current_table {
        result.insert(table_name, Value::Array(rows));
    }

    Value::Object(result)
}

/// Parse a simple value (string, number, bool)
fn parse_simple_value(value: &str) -> Value {
    let value = value.trim();

    // Check for quoted string
    if value.starts_with('"') && value.ends_with('"') {
        return Value::String(value[1..value.len() - 1].to_string());
    }

    // Check for boolean
    if value == "true" {
        return Value::Bool(true);
    }
    if value == "false" {
        return Value::Bool(false);
    }

    // Check for number
    if let Ok(n) = value.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = value.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }

    // Default to string (unquoted)
    Value::String(value.to_string())
}

/// Parse an array value: val1,val2,"val3"
fn parse_array_value(value: &str) -> Value {
    let value = value.trim();

    // Handle empty array
    if value.is_empty() {
        return Value::Array(Vec::new());
    }

    // Parse comma-separated values, respecting quotes
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in value.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    items.push(parse_simple_value(trimmed));
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Don't forget the last item
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        items.push(parse_simple_value(trimmed));
    }

    Value::Array(items)
}

/// Parse a table row with known fields
fn parse_table_row(line: &str, fields: &[String]) -> Value {
    let line = line.trim();
    let mut obj = Map::new();

    // Split by comma, respecting quotes
    let values = split_csv_line(line);

    for (i, field) in fields.iter().enumerate() {
        let value = values.get(i).map(|s| s.as_str()).unwrap_or("_");
        // "_" represents null/undefined in TOON
        if value != "_" {
            obj.insert(field.clone(), parse_simple_value(value));
        }
    }

    Value::Object(obj)
}

/// Split a CSV line respecting quoted strings
fn split_csv_line(line: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ',' if !in_quotes => {
                result.push(current.trim().trim_matches('"').to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    result.push(current.trim().trim_matches('"').to_string());
    result
}

/// Read a cached TOON file and parse it to JSON
///
/// This is the primary function for reading module shards and other cached files.
pub fn read_cached_toon(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: path.display().to_string(),
        });
    }

    let content = fs::read_to_string(path)?;

    // Check if it's actually JSON (legacy files)
    if content.trim_start().starts_with('{') {
        return serde_json::from_str(&content).map_err(|e| McpDiffError::GitError {
            message: format!("JSON parse error: {}", e),
        });
    }

    // Parse as TOON
    Ok(parse_toon_to_json(&content))
}

/// Read a cached file, auto-detecting format (TOON or JSON)
///
/// Returns the content in the original format and the parsed JSON.
pub fn read_cached_file(path: &Path) -> Result<CachedContent> {
    if !path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: path.display().to_string(),
        });
    }

    let content = fs::read_to_string(path)?;
    let is_json = content.trim_start().starts_with('{');

    let json = if is_json {
        serde_json::from_str(&content).map_err(|e| McpDiffError::GitError {
            message: format!("JSON parse error: {}", e),
        })?
    } else {
        parse_toon_to_json(&content)
    };

    Ok(CachedContent {
        raw: content,
        json,
        is_json,
    })
}

/// Represents parsed cached content
pub struct CachedContent {
    /// Raw file content
    pub raw: String,
    /// Parsed JSON representation
    pub json: Value,
    /// Whether the original was JSON format
    pub is_json: bool,
}

impl CachedContent {
    /// Get the content as TOON format
    pub fn as_toon(&self) -> String {
        if self.is_json {
            // Convert JSON to TOON
            crate::commands::encode_toon(&self.json)
        } else {
            // Already TOON, return raw
            self.raw.clone()
        }
    }

    /// Get the content as JSON string
    pub fn as_json(&self) -> String {
        serde_json::to_string_pretty(&self.json).unwrap_or_default()
    }
}

/// Extract module symbols from TOON content
///
/// Parses the symbols table from a module shard and returns structured entries.
pub fn extract_module_symbols(content: &CachedContent, module_name: &str) -> Vec<ModuleSymbol> {
    let mut symbols = Vec::new();

    if let Some(sym_array) = content.json.get("symbols").and_then(|s| s.as_array()) {
        for sym in sym_array {
            symbols.push(ModuleSymbol {
                name: get_str(sym, &["symbol", "s", "name"]).to_string(),
                hash: get_str(sym, &["hash", "h"]).to_string(),
                kind: get_str(sym, &["kind", "k"]).to_string(),
                file: get_str(sym, &["file", "f"]).to_string(),
                lines: get_str(sym, &["lines", "l"]).to_string(),
                risk: get_str(sym, &["risk", "r"]).unwrap_or("low").to_string(),
                module: module_name.to_string(),
            });
        }
    }

    symbols
}

/// A symbol entry from a module shard
#[derive(Debug, Clone)]
pub struct ModuleSymbol {
    pub name: String,
    pub hash: String,
    pub kind: String,
    pub file: String,
    pub lines: String,
    pub risk: String,
    pub module: String,
}

/// Helper to get a string value from JSON with fallback keys
fn get_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = value.get(*key).and_then(|v| v.as_str()) {
            return Some(v);
        }
    }
    None
}

/// Get string with default
#[allow(dead_code)]
trait GetStrExt {
    fn to_string(self) -> String;
    fn unwrap_or(self, default: &str) -> String;
}

impl GetStrExt for Option<&str> {
    fn to_string(self) -> String {
        self.unwrap_or("?").to_string()
    }

    fn unwrap_or(self, default: &str) -> String {
        self.map(|s| s.to_string())
            .unwrap_or_else(|| default.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_kv() {
        let content = r#"_type: test
name: "hello"
count: 42
active: true"#;

        let json = parse_toon_to_json(content);
        assert_eq!(json.get("_type").unwrap().as_str().unwrap(), "test");
        assert_eq!(json.get("name").unwrap().as_str().unwrap(), "hello");
        assert_eq!(json.get("count").unwrap().as_i64().unwrap(), 42);
        assert_eq!(json.get("active").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn test_parse_array() {
        let content = r#"patterns[3]: "one","two","three"
tags[2]: alpha,beta"#;

        let json = parse_toon_to_json(content);
        let patterns = json.get("patterns").unwrap().as_array().unwrap();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].as_str().unwrap(), "one");
    }

    #[test]
    fn test_parse_table() {
        let content = r#"_type: module
symbols[2]{hash,name,kind}:
  abc123,"foo",function
  def456,"bar",class"#;

        let json = parse_toon_to_json(content);
        let symbols = json.get("symbols").unwrap().as_array().unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].get("name").unwrap().as_str().unwrap(), "foo");
        assert_eq!(symbols[1].get("kind").unwrap().as_str().unwrap(), "class");
    }

    #[test]
    fn test_auto_detect_json() {
        let json_content = r#"{"_type": "test", "value": 42}"#;
        // This would need a temp file to test properly
        let parsed = parse_toon_to_json(json_content);
        // TOON parser sees this as invalid TOON, but read_cached_toon handles it
        assert!(parsed.get("_type").is_none()); // Not valid TOON
    }
}
