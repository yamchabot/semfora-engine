//! Security vulnerability detection via pre-compiled CVE fingerprints
//!
//! This module provides air-gapped security analysis by matching code against
//! pre-compiled vulnerability patterns derived from NVD CVE data and GitHub
//! Security Advisories.
//!
//! # Architecture
//!
//! The security detection reuses the duplicate detection infrastructure:
//! - Same 2-pass algorithm (coarse Hamming filter → fine Jaccard similarity)
//! - Same fingerprint types (call, control_flow, state)
//! - Patterns embedded at build time for air-gapped operation
//!
//! # Example
//!
//! ```ignore
//! use semfora_engine::security::{PatternDatabase, CVEMatch};
//! use semfora_engine::duplicate::FunctionSignature;
//!
//! let pattern_db = PatternDatabase::load_embedded()?;
//! let matches = pattern_db.match_function(&signature, 0.75)?;
//!
//! for m in matches {
//!     println!("{}: {:.0}% similar - {}", m.cve_id, m.similarity * 100.0, m.description);
//! }
//! ```

pub mod compiler;
pub mod patterns;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::lang::Lang;

/// Severity levels from CVSS v3
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// CVSS 0.0
    None,
    /// CVSS 0.1-3.9
    Low,
    /// CVSS 4.0-6.9
    Medium,
    /// CVSS 7.0-8.9
    High,
    /// CVSS 9.0-10.0
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Medium
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::None => write!(f, "NONE"),
            Severity::Low => write!(f, "LOW"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl Severity {
    /// Parse from CVSS v3 score
    pub fn from_cvss(score: f32) -> Self {
        match score {
            s if s >= 9.0 => Severity::Critical,
            s if s >= 7.0 => Severity::High,
            s if s >= 4.0 => Severity::Medium,
            s if s > 0.0 => Severity::Low,
            _ => Severity::None,
        }
    }
}

/// Source of the CVE pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternSource {
    /// Extracted from GitHub Security Advisory fix commit
    GitHubAdvisory {
        ghsa_id: String,
        commit_sha: String,
    },
    /// Manually curated by security team
    ManualCuration {
        author: String,
        date: String,
    },
    /// Extracted from NVD reference URL
    NvdReference {
        url: String,
    },
}

/// Pre-compiled vulnerability pattern (embedded in binary)
///
/// This struct parallels `FunctionSignature` from duplicate detection,
/// using the same fingerprint types for compatibility with the 2-pass
/// matching algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CVEPattern {
    // === Identification ===
    /// CVE identifier (e.g., "CVE-2021-44228")
    pub cve_id: String,

    /// CWE identifiers this pattern relates to (e.g., ["CWE-502", "CWE-917"])
    pub cwe_ids: Vec<String>,

    /// Unique pattern ID within the CVE (one CVE may have multiple patterns)
    pub pattern_id: u32,

    // === Fingerprints (64-bit for fast Hamming distance) ===
    /// Hash of dangerous call sequence (FNV-1a)
    pub call_fingerprint: u64,

    /// Hash of control flow pattern (e.g., "ITT" for if-try-try)
    pub control_flow_fingerprint: u64,

    /// Hash of state mutation patterns
    pub state_fingerprint: u64,

    // === Expanded data for fine matching ===
    /// Dangerous/vulnerable API calls to match
    /// e.g., ["lookup", "JNDI", "RMI"] for Log4Shell
    pub vulnerable_calls: Vec<String>,

    /// Control flow pattern string (e.g., "ITT", "IFTF")
    pub control_flow_pattern: String,

    /// State variable patterns to match
    pub state_patterns: Vec<String>,

    // === Metadata from NVD ===
    /// CVSS v3 score (0.0-10.0)
    pub cvss_v3_score: Option<f32>,

    /// Severity level derived from CVSS
    pub severity: Severity,

    /// Languages this pattern applies to
    pub languages: Vec<Lang>,

    /// Short description of the vulnerability
    pub description: String,

    /// Remediation guidance
    pub remediation: Option<String>,

    // === Source tracking ===
    /// Where this pattern came from
    pub source: PatternSource,

    /// Pattern confidence (0.0-1.0) - affects matching threshold
    /// Higher confidence = more specific pattern = higher threshold
    pub confidence: f32,
}

impl CVEPattern {
    /// Create a new CVE pattern with required fields
    pub fn new(cve_id: impl Into<String>, cwe_ids: Vec<String>, pattern_id: u32) -> Self {
        Self {
            cve_id: cve_id.into(),
            cwe_ids,
            pattern_id,
            call_fingerprint: 0,
            control_flow_fingerprint: 0,
            state_fingerprint: 0,
            vulnerable_calls: Vec::new(),
            control_flow_pattern: String::new(),
            state_patterns: Vec::new(),
            cvss_v3_score: None,
            severity: Severity::Medium,
            languages: Vec::new(),
            description: String::new(),
            remediation: None,
            source: PatternSource::ManualCuration {
                author: "unknown".into(),
                date: "unknown".into(),
            },
            confidence: 0.8,
        }
    }

    /// Set the fingerprints for this pattern
    pub fn with_fingerprints(
        mut self,
        call: u64,
        control_flow: u64,
        state: u64,
    ) -> Self {
        self.call_fingerprint = call;
        self.control_flow_fingerprint = control_flow;
        self.state_fingerprint = state;
        self
    }

    /// Set the vulnerable calls for fine matching
    pub fn with_vulnerable_calls(mut self, calls: Vec<String>) -> Self {
        self.vulnerable_calls = calls;
        self
    }

    /// Set severity from CVSS score
    pub fn with_cvss(mut self, score: f32) -> Self {
        self.cvss_v3_score = Some(score);
        self.severity = Severity::from_cvss(score);
        self
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the languages this pattern applies to
    pub fn with_languages(mut self, languages: Vec<Lang>) -> Self {
        self.languages = languages;
        self
    }

    /// Set the pattern source
    pub fn with_source(mut self, source: PatternSource) -> Self {
        self.source = source;
        self
    }

    /// Set the confidence level
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Compiled pattern database (serialized with bincode)
///
/// This is the main structure that gets embedded in the binary at build time.
/// It contains all CVE patterns organized for efficient lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDatabase {
    /// Database version (for compatibility checking)
    pub version: String,

    /// When the database was generated (ISO 8601)
    pub generated_at: String,

    /// All CVE patterns
    pub patterns: Vec<CVEPattern>,

    /// Index: CWE ID → pattern indices
    /// e.g., "CWE-89" → [0, 5, 12, ...]
    pub cwe_index: HashMap<String, Vec<usize>>,

    /// Index: Language → pattern indices
    /// e.g., Lang::Rust → [2, 8, 15, ...]
    pub lang_index: HashMap<Lang, Vec<usize>>,
}

impl Default for PatternDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternDatabase {
    /// Create an empty pattern database
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            patterns: Vec::new(),
            cwe_index: HashMap::new(),
            lang_index: HashMap::new(),
        }
    }

    /// Add a pattern and update indices
    pub fn add_pattern(&mut self, pattern: CVEPattern) {
        let idx = self.patterns.len();

        // Update CWE index
        for cwe in &pattern.cwe_ids {
            self.cwe_index
                .entry(cwe.clone())
                .or_default()
                .push(idx);
        }

        // Update language index
        for lang in &pattern.languages {
            self.lang_index
                .entry(*lang)
                .or_default()
                .push(idx);
        }

        self.patterns.push(pattern);
    }

    /// Get patterns for a specific CWE
    pub fn patterns_for_cwe(&self, cwe: &str) -> Vec<&CVEPattern> {
        self.cwe_index
            .get(cwe)
            .map(|indices| indices.iter().map(|&i| &self.patterns[i]).collect())
            .unwrap_or_default()
    }

    /// Get patterns for a specific language
    pub fn patterns_for_lang(&self, lang: Lang) -> Vec<&CVEPattern> {
        self.lang_index
            .get(&lang)
            .map(|indices| indices.iter().map(|&i| &self.patterns[i]).collect())
            .unwrap_or_default()
    }

    /// Total number of patterns
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Check if database is empty
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Create a database from a vector of patterns
    pub fn from_patterns(patterns: Vec<CVEPattern>) -> Self {
        let mut db = Self::new();
        for pattern in patterns {
            db.add_pattern(pattern);
        }
        db
    }

    /// Serialize to bincode format
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bincode format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// A CVE pattern match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CVEMatch {
    /// The matched CVE ID
    pub cve_id: String,

    /// CWE categories
    pub cwe_ids: Vec<String>,

    /// Similarity score (0.0-1.0)
    pub similarity: f32,

    /// Severity level
    pub severity: Severity,

    /// Description of the vulnerability
    pub description: String,

    /// Remediation guidance (if available)
    pub remediation: Option<String>,

    /// File where the match was found
    pub file: String,

    /// Function name that matched
    pub function: String,

    /// Line number in the file
    pub line: u32,
}

/// Summary of a CVE scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CVEScanSummary {
    /// Number of functions scanned
    pub functions_scanned: usize,

    /// Number of patterns checked
    pub patterns_checked: usize,

    /// Total matches found
    pub total_matches: usize,

    /// Matches by severity
    pub by_severity: HashMap<Severity, usize>,

    /// Scan duration in milliseconds
    pub scan_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_from_cvss() {
        assert_eq!(Severity::from_cvss(10.0), Severity::Critical);
        assert_eq!(Severity::from_cvss(9.0), Severity::Critical);
        assert_eq!(Severity::from_cvss(8.5), Severity::High);
        assert_eq!(Severity::from_cvss(7.0), Severity::High);
        assert_eq!(Severity::from_cvss(5.5), Severity::Medium);
        assert_eq!(Severity::from_cvss(4.0), Severity::Medium);
        assert_eq!(Severity::from_cvss(2.0), Severity::Low);
        assert_eq!(Severity::from_cvss(0.0), Severity::None);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::None);
    }

    #[test]
    fn test_pattern_database_indexing() {
        let mut db = PatternDatabase::new();

        let pattern = CVEPattern::new("CVE-2021-44228", vec!["CWE-502".into()], 0)
            .with_languages(vec![Lang::Java])
            .with_cvss(10.0)
            .with_description("Log4Shell");

        db.add_pattern(pattern);

        assert_eq!(db.len(), 1);
        assert_eq!(db.patterns_for_cwe("CWE-502").len(), 1);
        assert_eq!(db.patterns_for_lang(Lang::Java).len(), 1);
        assert!(db.patterns_for_cwe("CWE-79").is_empty());
    }

    #[test]
    fn test_pattern_serialization() {
        let mut db = PatternDatabase::new();
        db.add_pattern(
            CVEPattern::new("CVE-2021-44228", vec!["CWE-502".into()], 0)
                .with_cvss(10.0),
        );

        let bytes = db.to_bytes().unwrap();
        let restored = PatternDatabase::from_bytes(&bytes).unwrap();

        assert_eq!(restored.len(), 1);
        assert_eq!(restored.patterns[0].cve_id, "CVE-2021-44228");
    }
}
