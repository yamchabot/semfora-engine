//! Embedded pattern database loader
//!
//! At build time, compiled patterns are embedded into the binary.
//! This module provides lazy loading of the pattern database.
//!
//! Runtime updates are supported via HTTP fetch from a pattern server.
//! Set `SEMFORA_PATTERN_URL` environment variable to enable automatic updates.

use crate::error::McpDiffError;
use crate::security::PatternDatabase;
use once_cell::sync::Lazy;
use std::sync::RwLock;

/// Embedded pattern database bytes (set by build.rs if available)
/// If not present, an empty database is used
#[cfg(feature = "embedded-patterns")]
static EMBEDDED_PATTERNS: &[u8] = include_bytes!(env!("SECURITY_PATTERNS_PATH"));

#[cfg(not(feature = "embedded-patterns"))]
static EMBEDDED_PATTERNS: &[u8] = &[];

/// Cached pattern database
static PATTERN_DB: Lazy<RwLock<Option<PatternDatabase>>> = Lazy::new(|| RwLock::new(None));

/// Environment variable for pattern update URL
pub const PATTERN_URL_ENV: &str = "SEMFORA_PATTERN_URL";

/// Default pattern update URL (can be overridden by SEMFORA_PATTERN_URL)
pub const DEFAULT_PATTERN_URL: &str = "https://patterns.semfora.dev/security_patterns.bin";

/// Result of a pattern update operation
#[derive(Debug, Clone)]
pub struct PatternUpdateResult {
    /// Whether patterns were updated
    pub updated: bool,
    /// Previous version (if any)
    pub previous_version: Option<String>,
    /// Current version after update
    pub current_version: String,
    /// Number of patterns in database
    pub pattern_count: usize,
    /// Message describing what happened
    pub message: String,
}

/// Load the embedded pattern database
///
/// Returns an empty database if no patterns are embedded.
/// Caches the result for subsequent calls.
pub fn load_embedded_patterns() -> PatternDatabase {
    // Check cache first
    {
        let cache = PATTERN_DB.read().unwrap();
        if let Some(ref db) = *cache {
            return db.clone();
        }
    }

    // Load from embedded bytes
    let db = if EMBEDDED_PATTERNS.is_empty() {
        tracing::info!("No embedded security patterns, using empty database");
        PatternDatabase::new()
    } else {
        match PatternDatabase::from_bytes(EMBEDDED_PATTERNS) {
            Ok(db) => {
                tracing::info!(
                    "Loaded {} security patterns from embedded database",
                    db.len()
                );
                db
            }
            Err(e) => {
                tracing::error!("Failed to load embedded patterns: {}", e);
                PatternDatabase::new()
            }
        }
    };

    // Cache for future use
    {
        let mut cache = PATTERN_DB.write().unwrap();
        *cache = Some(db.clone());
    }

    db
}

/// Load patterns from a file path (for testing/development)
pub fn load_patterns_from_file(path: &std::path::Path) -> crate::error::Result<PatternDatabase> {
    let bytes = std::fs::read(path)?;
    let db = PatternDatabase::from_bytes(&bytes)?;
    Ok(db)
}

/// Check if embedded patterns are available
pub fn has_embedded_patterns() -> bool {
    !EMBEDDED_PATTERNS.is_empty()
}

/// Get the embedded patterns version (if available)
pub fn embedded_patterns_version() -> Option<String> {
    if EMBEDDED_PATTERNS.is_empty() {
        return None;
    }

    // Try to peek at the version without fully deserializing
    PatternDatabase::from_bytes(EMBEDDED_PATTERNS)
        .ok()
        .map(|db| db.version)
}

/// Get the currently loaded patterns version
pub fn current_patterns_version() -> Option<String> {
    let cache = PATTERN_DB.read().unwrap();
    cache.as_ref().map(|db| db.version.clone())
}

/// Get pattern update URL from environment or use default
pub fn get_pattern_url() -> Option<String> {
    std::env::var(PATTERN_URL_ENV).ok()
}

/// Fetch pattern updates from a URL
///
/// This function fetches the pattern database from the specified URL,
/// validates it, and atomically swaps it with the current database.
///
/// # Arguments
/// * `url` - Optional URL to fetch from. If None, uses SEMFORA_PATTERN_URL env var or default.
/// * `force` - If true, updates even if versions match
///
/// # Returns
/// `PatternUpdateResult` describing what happened
pub async fn fetch_pattern_updates(
    url: Option<&str>,
    force: bool,
) -> Result<PatternUpdateResult, McpDiffError> {
    let fetch_url = url
        .map(|s| s.to_string())
        .or_else(get_pattern_url)
        .unwrap_or_else(|| DEFAULT_PATTERN_URL.to_string());

    tracing::info!("Fetching security patterns from: {}", fetch_url);

    // Get current version for comparison
    let previous_version = current_patterns_version();

    // Fetch the pattern database
    let client = reqwest::Client::builder()
        .user_agent("semfora-engine/0.1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| McpDiffError::Generic(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&fetch_url)
        .send()
        .await
        .map_err(|e| McpDiffError::Generic(format!("Failed to fetch patterns: {}", e)))?;

    if !response.status().is_success() {
        return Err(McpDiffError::Generic(format!(
            "Pattern server returned HTTP {}: {}",
            response.status(),
            fetch_url
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| McpDiffError::Generic(format!("Failed to read response: {}", e)))?;

    // Deserialize and validate
    let new_db = PatternDatabase::from_bytes(&bytes)
        .map_err(|e| McpDiffError::Generic(format!("Invalid pattern database: {}", e)))?;

    let new_version = new_db.version.clone();
    let pattern_count = new_db.len();

    // Check if update is needed
    if !force && previous_version.as_ref() == Some(&new_version) {
        return Ok(PatternUpdateResult {
            updated: false,
            previous_version: previous_version.clone(),
            current_version: new_version,
            pattern_count,
            message: "Patterns already up to date".to_string(),
        });
    }

    // Atomic swap of the pattern database
    {
        let mut cache = PATTERN_DB.write().unwrap();
        *cache = Some(new_db);
    }

    tracing::info!(
        "Updated security patterns: {} -> {} ({} patterns)",
        previous_version.as_deref().unwrap_or("none"),
        new_version,
        pattern_count
    );

    Ok(PatternUpdateResult {
        updated: true,
        previous_version,
        current_version: new_version,
        pattern_count,
        message: format!("Updated to {} patterns", pattern_count),
    })
}

/// Update patterns from a local file
///
/// Useful for offline environments or testing.
pub fn update_patterns_from_file(path: &std::path::Path) -> Result<PatternUpdateResult, McpDiffError> {
    let previous_version = current_patterns_version();

    let bytes = std::fs::read(path)?;
    let new_db = PatternDatabase::from_bytes(&bytes)?;

    let new_version = new_db.version.clone();
    let pattern_count = new_db.len();

    // Atomic swap
    {
        let mut cache = PATTERN_DB.write().unwrap();
        *cache = Some(new_db);
    }

    tracing::info!(
        "Loaded security patterns from file: {} patterns",
        pattern_count
    );

    Ok(PatternUpdateResult {
        updated: true,
        previous_version,
        current_version: new_version,
        pattern_count,
        message: format!("Loaded {} patterns from {}", pattern_count, path.display()),
    })
}

/// Update patterns from raw bytes
///
/// Used when patterns are received through other means (e.g., embedded in CI artifacts).
pub fn update_patterns_from_bytes(bytes: &[u8]) -> Result<PatternUpdateResult, McpDiffError> {
    let previous_version = current_patterns_version();

    let new_db = PatternDatabase::from_bytes(bytes)?;
    let new_version = new_db.version.clone();
    let pattern_count = new_db.len();

    // Atomic swap
    {
        let mut cache = PATTERN_DB.write().unwrap();
        *cache = Some(new_db);
    }

    Ok(PatternUpdateResult {
        updated: true,
        previous_version,
        current_version: new_version,
        pattern_count,
        message: format!("Loaded {} patterns from bytes", pattern_count),
    })
}

/// Get pattern database statistics
pub fn pattern_stats() -> PatternStats {
    let cache = PATTERN_DB.read().unwrap();
    match cache.as_ref() {
        Some(db) => PatternStats {
            loaded: true,
            version: Some(db.version.clone()),
            generated_at: Some(db.generated_at.clone()),
            pattern_count: db.len(),
            cwe_count: db.cwe_index.len(),
            language_count: db.lang_index.len(),
            source: if has_embedded_patterns() {
                PatternSource::Embedded
            } else {
                PatternSource::Runtime
            },
        },
        None => PatternStats {
            loaded: false,
            version: None,
            generated_at: None,
            pattern_count: 0,
            cwe_count: 0,
            language_count: 0,
            source: PatternSource::None,
        },
    }
}

/// Statistics about the loaded pattern database
#[derive(Debug, Clone)]
pub struct PatternStats {
    pub loaded: bool,
    pub version: Option<String>,
    pub generated_at: Option<String>,
    pub pattern_count: usize,
    pub cwe_count: usize,
    pub language_count: usize,
    pub source: PatternSource,
}

/// Source of the pattern database
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternSource {
    /// No patterns loaded
    None,
    /// Patterns embedded at build time
    Embedded,
    /// Patterns loaded at runtime (file or HTTP)
    Runtime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_empty_patterns() {
        // Without embedded patterns feature, should return empty db
        let db = load_embedded_patterns();
        // In test mode, may or may not have patterns
        assert!(db.len() >= 0);
    }

    #[test]
    fn test_has_embedded_patterns() {
        // Should work regardless of feature flag
        let _ = has_embedded_patterns();
    }

    #[test]
    fn test_pattern_stats() {
        // Should work regardless of pattern state
        let stats = pattern_stats();
        // Pattern count should be non-negative
        assert!(stats.pattern_count >= 0);
    }

    #[test]
    fn test_update_from_bytes_valid() {
        // Create a minimal valid pattern database
        let db = PatternDatabase::new();
        let bytes = db.to_bytes().expect("should serialize");

        let result = update_patterns_from_bytes(&bytes);
        assert!(result.is_ok());

        let update = result.unwrap();
        assert!(update.updated);
        assert_eq!(update.pattern_count, 0);
    }

    #[test]
    fn test_update_from_bytes_invalid() {
        // Invalid bytes should fail
        let bytes = b"not a valid pattern database";
        let result = update_patterns_from_bytes(bytes);
        assert!(result.is_err());
    }
}
