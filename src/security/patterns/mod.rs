//! Pre-compiled vulnerability patterns
//!
//! This module provides:
//! - Embedded pattern database (loaded from binary at runtime)
//! - Manually curated patterns for high-profile CVEs
//! - Runtime pattern updates via HTTP fetch

pub mod embedded;
pub mod manual;

pub use embedded::{
    load_embedded_patterns,
    fetch_pattern_updates,
    update_patterns_from_file,
    update_patterns_from_bytes,
    pattern_stats,
    current_patterns_version,
    has_embedded_patterns,
    embedded_patterns_version,
    PatternUpdateResult,
    PatternStats,
    PatternSource,
    PATTERN_URL_ENV,
    DEFAULT_PATTERN_URL,
};
pub use manual::all_patterns;
