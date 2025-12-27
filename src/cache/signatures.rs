//! Signature loading utilities for the cache module.
//!
//! This module provides unified function signature loading used by both
//! CLI commands and MCP tools.

use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::duplicate::FunctionSignature;

use super::CacheDir;

/// Load function signatures from the cache.
///
/// Reads the signature index file (JSON Lines format) and returns all valid signatures.
/// Invalid/malformed signatures are logged and skipped.
///
/// This is the unified function for CLI/MCP (DEDUP-105).
///
/// # Arguments
///
/// * `cache` - The cache directory containing the signature index
///
/// # Returns
///
/// * `Ok(Vec<FunctionSignature>)` - Successfully loaded signatures (may be empty)
/// * `Err(String)` - If the signature file cannot be opened/read
///
/// # Example
///
/// ```ignore
/// use semfora_engine::cache::{CacheDir, load_function_signatures};
///
/// let cache = CacheDir::for_repo(Path::new("/my/repo"))?;
/// let signatures = load_function_signatures(&cache)?;
/// println!("Loaded {} signatures", signatures.len());
/// ```
pub fn load_function_signatures(cache: &CacheDir) -> Result<Vec<FunctionSignature>, String> {
    let sig_path = cache.signature_index_path();

    // Return empty if file doesn't exist (graceful degradation)
    if !sig_path.exists() {
        return Ok(Vec::new());
    }

    let file =
        File::open(&sig_path).map_err(|e| format!("Failed to open signature index: {}", e))?;
    let reader = BufReader::new(file);

    let mut signatures = Vec::new();
    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<FunctionSignature>(&line) {
            Ok(sig) => signatures.push(sig),
            Err(e) => {
                // Log malformed lines for debugging but continue processing
                tracing::warn!(
                    "Skipping malformed signature at line {}: {}",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(signatures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Helper to create a test CacheDir with required fields
    fn test_cache(root: PathBuf, repo_root: PathBuf) -> CacheDir {
        CacheDir {
            root,
            repo_root,
            repo_hash: "test_hash".to_string(),
        }
    }

    #[test]
    fn test_load_empty_file() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".semfora");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create empty signature file (must match signature_index_path())
        let sig_path = cache_dir.join("signature_index.jsonl");
        File::create(&sig_path).unwrap();

        let cache = test_cache(cache_dir, dir.path().to_path_buf());
        let result = load_function_signatures(&cache);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".semfora");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cache = test_cache(cache_dir, dir.path().to_path_buf());
        let result = load_function_signatures(&cache);
        // Should return empty vec, not error
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// Create a valid JSON signature for testing
    fn valid_sig_json(name: &str, hash: &str) -> String {
        format!(
            r#"{{"symbol_hash":"{}","name":"{}","file":"test.rs","module":"test","start_line":1,"name_tokens":["{}"],"call_fingerprint":0,"control_flow_fingerprint":0,"state_fingerprint":0,"business_calls":[],"param_count":0,"has_business_logic":false,"line_count":10}}"#,
            hash, name, name
        )
    }

    #[test]
    fn test_load_valid_signatures() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".semfora");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create signature file with valid JSON Lines (must match signature_index_path())
        let sig_path = cache_dir.join("signature_index.jsonl");
        let mut file = File::create(&sig_path).unwrap();
        writeln!(file, "{}", valid_sig_json("foo", "abc123")).unwrap();

        let cache = test_cache(cache_dir, dir.path().to_path_buf());
        let result = load_function_signatures(&cache);
        assert!(result.is_ok());

        let sigs = result.unwrap();
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "foo");
    }

    #[test]
    fn test_skip_malformed_lines() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join(".semfora");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create signature file with mix of valid and invalid lines (must match signature_index_path())
        let sig_path = cache_dir.join("signature_index.jsonl");
        let mut file = File::create(&sig_path).unwrap();
        writeln!(file, "{}", valid_sig_json("foo", "abc123")).unwrap();
        writeln!(file, "not valid json").unwrap();
        writeln!(file, "{}", valid_sig_json("bar", "def456")).unwrap();

        let cache = test_cache(cache_dir, dir.path().to_path_buf());
        let result = load_function_signatures(&cache);
        assert!(result.is_ok());

        let sigs = result.unwrap();
        // Should have 2 valid signatures, skipping the malformed one
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[0].name, "foo");
        assert_eq!(sigs[1].name, "bar");
    }
}
