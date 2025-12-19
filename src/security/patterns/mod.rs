//! Pre-compiled vulnerability patterns
//!
//! This module provides:
//! - Embedded pattern database (loaded from binary at runtime)
//! - Manually curated patterns for high-profile CVEs
//! - Runtime pattern updates via HTTP fetch

pub mod embedded;
pub mod manual;

pub use embedded::{
    current_patterns_version, embedded_patterns_version, fetch_pattern_updates,
    has_embedded_patterns, load_embedded_patterns, pattern_stats, update_patterns_from_bytes,
    update_patterns_from_file, PatternSource, PatternStats, PatternUpdateResult,
    DEFAULT_PATTERN_URL, PATTERN_URL_ENV,
};
pub use manual::all_patterns;
