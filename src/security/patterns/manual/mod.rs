//! Manually curated vulnerability patterns
//!
//! These patterns are handcrafted for high-profile CVEs that may not have
//! easily extractable code samples, or where we want very specific detection.

mod log4shell;
mod spring4shell;
mod sql_injection;
mod xss;
mod deserialization;
mod command_injection;

use crate::security::CVEPattern;

/// Get all manually curated patterns
pub fn all_patterns() -> Vec<CVEPattern> {
    let mut patterns = Vec::new();

    patterns.extend(log4shell::patterns());
    patterns.extend(spring4shell::patterns());
    patterns.extend(sql_injection::patterns());
    patterns.extend(xss::patterns());
    patterns.extend(deserialization::patterns());
    patterns.extend(command_injection::patterns());

    patterns
}

/// Get pattern count by CWE
pub fn patterns_by_cwe(cwe: &str) -> Vec<CVEPattern> {
    all_patterns()
        .into_iter()
        .filter(|p| p.cwe_ids.iter().any(|c| c == cwe))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_patterns_not_empty() {
        let patterns = all_patterns();
        assert!(!patterns.is_empty(), "Should have manual patterns");
    }

    #[test]
    fn test_patterns_have_cve_ids() {
        for pattern in all_patterns() {
            assert!(!pattern.cve_id.is_empty(), "Pattern should have CVE ID");
            assert!(!pattern.cwe_ids.is_empty(), "Pattern should have CWE IDs");
        }
    }
}
