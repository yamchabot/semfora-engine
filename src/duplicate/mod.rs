//! Duplicate function detection for semantic code analysis
//!
//! This module provides detection of duplicate and near-duplicate functions
//! across codebases using semantic fingerprinting. It uses a two-phase matching
//! approach for sub-5ms query performance on large codebases.
//!
//! # Architecture
//!
//! 1. **Signature Generation**: Create lightweight fingerprints (~200 bytes) per function
//! 2. **Coarse Filter**: O(n) scan with early exit conditions using fingerprint hamming distance
//! 3. **Fine Similarity**: Jaccard-based semantic similarity for filtered candidates
//!
//! # Performance Budget
//!
//! - Coarse filter: ~500μs for 50K functions
//! - Fine similarity: ~2ms for k≈0.05n candidates
//! - Total: <5ms for full repository scan

pub mod boilerplate;

use crate::schema::{fnv1a_hash, Call, ControlFlowChange, ControlFlowKind, StateChange, SymbolInfo};
use boilerplate::{classify_boilerplate, BoilerplateCategory, BoilerplateConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Reference to a symbol in the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    /// Symbol hash for lookup
    pub hash: String,
    /// Symbol name
    pub name: String,
    /// File path
    pub file: String,
    /// Start line
    pub start_line: usize,
    /// End line
    pub end_line: usize,
}

impl SymbolRef {
    /// Create from SymbolInfo and file path
    pub fn from_symbol_info(info: &SymbolInfo, hash: &str, file: &str) -> Self {
        Self {
            hash: hash.to_string(),
            name: info.name.clone(),
            file: file.to_string(),
            start_line: info.start_line,
            end_line: info.end_line,
        }
    }
}

/// Lightweight function signature for duplicate detection (~200 bytes)
///
/// Generated at index time for each function, enabling fast O(n) coarse filtering
/// before expensive similarity computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Symbol hash for lookup
    pub symbol_hash: String,

    /// Symbol name
    pub name: String,

    /// File path
    pub file: String,

    /// Name tokens for similarity matching
    /// e.g., "handleUserLogin" → ["handle", "user", "login"]
    pub name_tokens: Vec<String>,

    /// Hash of sorted, filtered call names (64-bit for fast hamming distance)
    pub call_fingerprint: u64,

    /// Hash of control flow pattern sequence
    pub control_flow_fingerprint: u64,

    /// Hash of state mutation patterns
    pub state_fingerprint: u64,

    /// Business-logic calls (excluding utilities like console.log, Array.map)
    pub business_calls: Vec<String>,

    /// Parameter count (arguments + props)
    pub param_count: u8,

    /// Whether this function has business logic (non-empty business_calls)
    pub has_business_logic: bool,

    /// Boilerplate classification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boilerplate_category: Option<BoilerplateCategory>,

    /// Line count
    pub line_count: usize,
}

impl FunctionSignature {
    /// Generate a signature from a SymbolInfo
    pub fn from_symbol_info(
        info: &SymbolInfo,
        symbol_hash: &str,
        file: &str,
        config: Option<&BoilerplateConfig>,
    ) -> Self {
        // 1. Tokenize name: "handleUserLogin" → ["handle", "user", "login"]
        let name_tokens = tokenize_camel_snake(&info.name);

        // 2. Filter utility calls to get business calls
        let business_calls: Vec<String> = info
            .calls
            .iter()
            .filter(|c| !is_utility_call(&c.name, c.object.as_deref()))
            .map(|c| format_call_name(c))
            .collect();

        // Sort for deterministic fingerprint
        let mut sorted_calls = business_calls.clone();
        sorted_calls.sort();

        // 3. Generate fingerprints
        let call_fingerprint = compute_set_fingerprint(&sorted_calls);
        let control_flow_fingerprint = compute_control_flow_fingerprint(&info.control_flow);
        let state_fingerprint = compute_state_fingerprint(&info.state_changes);

        // 4. Classify boilerplate
        let boilerplate_category = classify_boilerplate(info, config);

        Self {
            symbol_hash: symbol_hash.to_string(),
            name: info.name.clone(),
            file: file.to_string(),
            name_tokens,
            call_fingerprint,
            control_flow_fingerprint,
            state_fingerprint,
            has_business_logic: !business_calls.is_empty(),
            business_calls,
            param_count: (info.arguments.len() + info.props.len()) as u8,
            boilerplate_category,
            line_count: info.end_line.saturating_sub(info.start_line) + 1,
        }
    }

    /// Create a SymbolRef from this signature
    pub fn to_symbol_ref(&self) -> SymbolRef {
        SymbolRef {
            hash: self.symbol_hash.clone(),
            name: self.name.clone(),
            file: self.file.clone(),
            start_line: 0, // Not stored in signature
            end_line: 0,
        }
    }
}

/// Kind of duplicate match
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DuplicateKind {
    /// >= 98% similarity - effectively identical
    Exact,
    /// 90-97% similarity - very similar, likely copy-paste
    Near,
    /// 80-89% similarity - same function evolved differently
    Divergent,
}

impl DuplicateKind {
    /// Determine kind from similarity score
    pub fn from_similarity(similarity: f64) -> Self {
        if similarity >= 0.98 {
            DuplicateKind::Exact
        } else if similarity >= 0.90 {
            DuplicateKind::Near
        } else {
            DuplicateKind::Divergent
        }
    }
}

/// Specific difference between two functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Difference {
    /// Function has an extra call not in the other
    ExtraCall(String),
    /// Function is missing a call from the other
    MissingCall(String),
    /// Different control flow structure
    DifferentControlFlow { expected: String, actual: String },
    /// Different state mutations
    DifferentStateMutation { expected: String, actual: String },
    /// Different parameter count
    DifferentParamCount { expected: u8, actual: u8 },
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Difference::ExtraCall(name) => write!(f, "+call: {}", name),
            Difference::MissingCall(name) => write!(f, "-call: {}", name),
            Difference::DifferentControlFlow { expected, actual } => {
                write!(f, "control_flow: {} vs {}", expected, actual)
            }
            Difference::DifferentStateMutation { expected, actual } => {
                write!(f, "state: {} vs {}", expected, actual)
            }
            Difference::DifferentParamCount { expected, actual } => {
                write!(f, "params: {} vs {}", expected, actual)
            }
        }
    }
}

/// A match indicating a potential duplicate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateMatch {
    /// Reference to the duplicate symbol
    pub symbol: SymbolRef,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,
    /// Kind of duplicate (Exact, Near, Divergent)
    pub kind: DuplicateKind,
    /// Specific differences found
    pub differences: Vec<Difference>,
}

impl DuplicateMatch {
    /// Create a new DuplicateMatch
    pub fn new(
        symbol: SymbolRef,
        similarity: f64,
        differences: Vec<Difference>,
    ) -> Self {
        Self {
            symbol,
            kind: DuplicateKind::from_similarity(similarity),
            similarity,
            differences,
        }
    }
}

/// A cluster of similar functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCluster {
    /// The "canonical" version (typically longest/most documented)
    pub primary: SymbolRef,
    /// Other functions similar to primary
    pub duplicates: Vec<DuplicateMatch>,
    /// Human-readable summary
    pub summary: String,
}

impl DuplicateCluster {
    /// Create a new cluster with a primary symbol
    pub fn new(primary: SymbolRef) -> Self {
        Self {
            primary,
            duplicates: Vec::new(),
            summary: String::new(),
        }
    }

    /// Add a duplicate to this cluster
    pub fn add_duplicate(&mut self, duplicate: DuplicateMatch) {
        self.duplicates.push(duplicate);
    }

    /// Generate a human-readable summary
    pub fn generate_summary(&mut self) {
        let count = self.duplicates.len() + 1;
        let exact_count = self
            .duplicates
            .iter()
            .filter(|d| d.kind == DuplicateKind::Exact)
            .count();
        let near_count = self
            .duplicates
            .iter()
            .filter(|d| d.kind == DuplicateKind::Near)
            .count();

        if exact_count > 0 {
            self.summary = format!(
                "{} identical implementations of '{}' (consolidation recommended)",
                exact_count + 1,
                self.primary.name
            );
        } else if near_count > 0 {
            self.summary = format!(
                "{} similar functions like '{}' (review for consolidation)",
                count, self.primary.name
            );
        } else {
            self.summary = format!(
                "{} divergent versions of '{}' (may cause bugs)",
                count, self.primary.name
            );
        }
    }
}

/// Duplicate detection engine
pub struct DuplicateDetector {
    /// Minimum similarity threshold (default: 0.90)
    pub threshold: f64,
    /// Whether to exclude boilerplate patterns (default: true)
    pub exclude_boilerplate: bool,
    /// Minimum similarity for divergent detection (default: 0.80)
    pub divergent_threshold: f64,
    /// Boilerplate configuration
    pub boilerplate_config: Option<BoilerplateConfig>,
}

impl Default for DuplicateDetector {
    fn default() -> Self {
        Self {
            threshold: 0.90,
            exclude_boilerplate: true,
            divergent_threshold: 0.80,
            boilerplate_config: None,
        }
    }
}

impl DuplicateDetector {
    /// Create a new detector with custom threshold
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            ..Default::default()
        }
    }

    /// Set whether to exclude boilerplate
    pub fn with_boilerplate_exclusion(mut self, exclude: bool) -> Self {
        self.exclude_boilerplate = exclude;
        self
    }

    /// Set boilerplate configuration
    pub fn with_boilerplate_config(mut self, config: BoilerplateConfig) -> Self {
        self.boilerplate_config = Some(config);
        self
    }

    /// Set divergent threshold
    pub fn with_divergent_threshold(mut self, threshold: f64) -> Self {
        self.divergent_threshold = threshold;
        self
    }

    /// Find duplicates of a single function against a set of signatures
    ///
    /// Uses two-phase matching:
    /// 1. Coarse filter with fingerprint hamming distance (O(n), ~500μs for 50K)
    /// 2. Fine similarity computation (O(k), ~2ms for k≈0.05n)
    pub fn find_duplicates(
        &self,
        target: &FunctionSignature,
        all_signatures: &[FunctionSignature],
    ) -> Vec<DuplicateMatch> {
        // Phase A: Coarse filter
        let candidates = self.coarse_filter(target, all_signatures);

        // Phase B: Fine similarity
        candidates
            .into_iter()
            .filter_map(|candidate| {
                let similarity = self.compute_similarity(target, candidate);
                if similarity >= self.divergent_threshold {
                    let differences = self.compute_differences(target, candidate);
                    let symbol_ref = candidate.to_symbol_ref();
                    Some(DuplicateMatch::new(symbol_ref, similarity, differences))
                } else {
                    None
                }
            })
            .filter(|m| m.similarity >= self.threshold || m.kind == DuplicateKind::Divergent)
            .collect()
    }

    /// Find all duplicate clusters in a set of signatures
    pub fn find_all_clusters(&self, signatures: &[FunctionSignature]) -> Vec<DuplicateCluster> {
        let mut processed: HashSet<String> = HashSet::new();
        let mut clusters: Vec<DuplicateCluster> = Vec::new();

        for sig in signatures {
            if processed.contains(&sig.symbol_hash) {
                continue;
            }

            let duplicates = self.find_duplicates(sig, signatures);

            if !duplicates.is_empty() {
                let mut cluster = DuplicateCluster::new(sig.to_symbol_ref());

                for dup in duplicates {
                    processed.insert(dup.symbol.hash.clone());
                    cluster.add_duplicate(dup);
                }

                cluster.generate_summary();
                clusters.push(cluster);
            }

            processed.insert(sig.symbol_hash.clone());
        }

        clusters
    }

    /// Phase A: Coarse filter with early exit conditions
    ///
    /// Filters to ~5% of total functions using:
    /// - Self-exclusion
    /// - Boilerplate mismatch exclusion
    /// - Parameter count difference > 2
    /// - Business call count difference > 3
    /// - Fingerprint hamming distance > 12
    fn coarse_filter<'a>(
        &self,
        target: &FunctionSignature,
        all: &'a [FunctionSignature],
    ) -> Vec<&'a FunctionSignature> {
        all.iter()
            .filter(|c| {
                // Skip self
                if c.symbol_hash == target.symbol_hash {
                    return false;
                }

                // Skip if boilerplate exclusion is on and categories don't match
                if self.exclude_boilerplate {
                    match (&c.boilerplate_category, &target.boilerplate_category) {
                        // Both boilerplate but different categories - skip
                        (Some(cat_c), Some(cat_t)) if cat_c != cat_t => return false,
                        // One is boilerplate, one isn't - skip
                        (Some(_), None) | (None, Some(_)) => return false,
                        _ => {}
                    }
                }

                // Skip if no business logic
                if !c.has_business_logic && !target.has_business_logic {
                    return false;
                }

                // Parameter count difference > 2 - unlikely duplicate
                if c.param_count.abs_diff(target.param_count) > 2 {
                    return false;
                }

                // Business call count difference > 3 - unlikely duplicate
                if c.business_calls.len().abs_diff(target.business_calls.len()) > 3 {
                    return false;
                }

                // Fingerprint hamming distance (bit differences)
                let call_dist = (target.call_fingerprint ^ c.call_fingerprint).count_ones();
                if call_dist > 12 {
                    return false;
                }

                true
            })
            .collect()
    }

    /// Phase B: Fine-grained similarity computation
    ///
    /// Weighted combination of:
    /// - Call similarity (0.45) - most important
    /// - Name similarity (0.20)
    /// - Control flow similarity (0.20)
    /// - State similarity (0.15)
    fn compute_similarity(&self, a: &FunctionSignature, b: &FunctionSignature) -> f64 {
        // Call similarity (Jaccard)
        let call_sim = jaccard_similarity(&a.business_calls, &b.business_calls);

        // Name similarity (token Jaccard)
        let name_sim = jaccard_similarity(&a.name_tokens, &b.name_tokens);

        // Control flow similarity (fingerprint comparison)
        let control_sim = fingerprint_similarity(a.control_flow_fingerprint, b.control_flow_fingerprint);

        // State similarity (fingerprint comparison)
        let state_sim = fingerprint_similarity(a.state_fingerprint, b.state_fingerprint);

        // Weighted combination
        call_sim * 0.45 + name_sim * 0.20 + control_sim * 0.20 + state_sim * 0.15
    }

    /// Compute specific differences between two functions
    fn compute_differences(&self, a: &FunctionSignature, b: &FunctionSignature) -> Vec<Difference> {
        let mut differences = Vec::new();

        let a_calls: HashSet<_> = a.business_calls.iter().collect();
        let b_calls: HashSet<_> = b.business_calls.iter().collect();

        // Extra calls in b
        for call in b_calls.difference(&a_calls) {
            differences.push(Difference::ExtraCall((*call).clone()));
        }

        // Missing calls in b
        for call in a_calls.difference(&b_calls) {
            differences.push(Difference::MissingCall((*call).clone()));
        }

        // Parameter count difference
        if a.param_count != b.param_count {
            differences.push(Difference::DifferentParamCount {
                expected: a.param_count,
                actual: b.param_count,
            });
        }

        differences
    }
}

// =============================================================================
// Fingerprint and Similarity Utilities
// =============================================================================

/// Tokenize camelCase and snake_case names
///
/// "handleUserLogin" → ["handle", "user", "login"]
/// "handle_user_login" → ["handle", "user", "login"]
pub fn tokenize_camel_snake(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
                current.clear();
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            tokens.push(current.to_lowercase());
            current.clear();
            current.push(ch);
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }

    tokens
}

/// Check if a call is a utility (should be excluded from similarity)
fn is_utility_call(name: &str, object: Option<&str>) -> bool {
    // Console/logging
    if let Some(obj) = object {
        if obj == "console" {
            return true;
        }
    }

    matches!(
        name,
        // Console/logging
        "log" | "error" | "warn" | "info" | "debug" |
        // JSON operations
        "stringify" | "parse" |
        // Type conversions
        "toString" | "parseInt" | "parseFloat" | "String" | "Number" | "Boolean" |
        // Common array methods (too common to be distinctive)
        "map" | "filter" | "reduce" | "forEach" | "find" | "some" | "every" |
        "push" | "pop" | "shift" | "unshift" | "slice" | "splice" | "concat" |
        "join" | "split" | "includes" | "indexOf" | "sort" |
        // Object utilities
        "keys" | "values" | "entries" | "assign" | "freeze" | "seal" |
        // Array utilities
        "from" | "isArray" | "of" |
        // Promise utilities
        "resolve" | "reject" | "all" | "allSettled" | "race" | "any" |
        // String utilities
        "trim" | "toLowerCase" | "toUpperCase" | "replace" | "match" | "test" |
        "startsWith" | "endsWith" | "charAt" | "charCodeAt" | "substring" | "substr"
    )
}

/// Format a call name for fingerprinting
fn format_call_name(call: &Call) -> String {
    match &call.object {
        Some(obj) => format!("{}.{}", obj, call.name),
        None => call.name.clone(),
    }
}

/// Compute a fingerprint from a sorted set of strings
fn compute_set_fingerprint(items: &[String]) -> u64 {
    let combined = items.join("|");
    fnv1a_hash(&combined)
}

/// Compute a fingerprint from control flow patterns
fn compute_control_flow_fingerprint(control_flow: &[ControlFlowChange]) -> u64 {
    let pattern: String = control_flow
        .iter()
        .map(|cf| match cf.kind {
            ControlFlowKind::If => "I",
            ControlFlowKind::For => "F",
            ControlFlowKind::While => "W",
            ControlFlowKind::Switch => "S",
            ControlFlowKind::Match => "M",
            ControlFlowKind::Try => "T",
            ControlFlowKind::Loop => "L",
        })
        .collect();

    fnv1a_hash(&pattern)
}

/// Compute a fingerprint from state changes
fn compute_state_fingerprint(state_changes: &[StateChange]) -> u64 {
    let mut names: Vec<_> = state_changes.iter().map(|s| s.name.as_str()).collect();
    names.sort();
    let combined = names.join("|");
    fnv1a_hash(&combined)
}

/// Jaccard similarity between two sets
fn jaccard_similarity<T: Eq + std::hash::Hash>(a: &[T], b: &[T]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let set_a: HashSet<_> = a.iter().collect();
    let set_b: HashSet<_> = b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Similarity based on fingerprint hamming distance
fn fingerprint_similarity(a: u64, b: u64) -> f64 {
    if a == b {
        return 1.0;
    }

    let hamming = (a ^ b).count_ones() as f64;
    // Max hamming distance is 64 bits
    1.0 - (hamming / 64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_camel_case() {
        assert_eq!(
            tokenize_camel_snake("handleUserLogin"),
            vec!["handle", "user", "login"]
        );
    }

    #[test]
    fn test_tokenize_snake_case() {
        assert_eq!(
            tokenize_camel_snake("handle_user_login"),
            vec!["handle", "user", "login"]
        );
    }

    #[test]
    fn test_tokenize_mixed() {
        assert_eq!(
            tokenize_camel_snake("handleUser_login"),
            vec!["handle", "user", "login"]
        );
    }

    #[test]
    fn test_jaccard_identical() {
        let a = vec!["foo", "bar", "baz"];
        let b = vec!["foo", "bar", "baz"];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_jaccard_partial() {
        let a = vec!["foo", "bar", "baz"];
        let b = vec!["foo", "bar", "qux"];
        // Intersection: foo, bar (2), Union: foo, bar, baz, qux (4)
        assert!((jaccard_similarity(&a, &b) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_jaccard_empty() {
        let a: Vec<&str> = vec![];
        let b: Vec<&str> = vec![];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fingerprint_similarity_identical() {
        let a = 0x123456789ABCDEF0u64;
        let b = 0x123456789ABCDEF0u64;
        assert!((fingerprint_similarity(a, b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_is_utility_call() {
        assert!(is_utility_call("log", Some("console")));
        assert!(is_utility_call("map", None));
        assert!(is_utility_call("filter", None));
        assert!(!is_utility_call("fetchUser", None));
        assert!(!is_utility_call("validateInput", None));
    }
}
