//! Data structures for benchmark snapshots and results

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete snapshot of a single build step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSnapshot {
    /// Step number (1-indexed)
    pub step: usize,
    /// File created/modified in this step
    pub file: String,
    /// Total files in the project at this step
    pub total_files: usize,
    /// All symbols extracted
    pub symbols: Vec<SymbolSnapshot>,
    /// Call graph: caller_hash -> [callee_hashes]
    pub call_graph: HashMap<String, Vec<String>>,
    /// Complexity metrics
    pub complexity: ComplexitySnapshot,
    /// Timing with AST cache (incremental)
    pub timing_cached: TimingData,
    /// Timing without cache (full parse)
    pub timing_uncached: TimingData,
}

/// Symbol information snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSnapshot {
    /// Symbol hash (FNV-1a)
    pub hash: String,
    /// Symbol name
    pub name: String,
    /// Symbol kind (function, class, interface, etc.)
    pub kind: String,
    /// File path (relative)
    pub file: String,
    /// Line range (e.g., "10-25")
    pub lines: String,
    /// Module name (e.g., "services", "handlers")
    pub module: String,
    /// Risk level (low, medium, high)
    pub risk: String,
    /// Cognitive complexity
    pub cognitive_complexity: usize,
    /// Maximum nesting depth
    pub max_nesting: usize,
    /// Functions this symbol calls
    pub calls: Vec<String>,
}

/// Aggregated complexity snapshot for a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexitySnapshot {
    /// Per-symbol complexity entries
    pub symbols: Vec<SymbolComplexityEntry>,
    /// Total cognitive complexity across all symbols
    pub total_cognitive: usize,
    /// Total cyclomatic complexity across all symbols
    pub total_cyclomatic: usize,
    /// Average cognitive complexity
    pub avg_cognitive: f64,
    /// Maximum cognitive complexity in any symbol
    pub max_cognitive: usize,
    /// Number of high-risk symbols
    pub high_risk_count: usize,
}

/// Single symbol complexity entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolComplexityEntry {
    pub hash: String,
    pub name: String,
    pub cognitive: usize,
    pub cyclomatic: usize,
    pub max_nesting: usize,
}

/// Timing data for a single run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimingData {
    /// Total time in microseconds
    pub total_us: u64,
    /// Parse time in microseconds
    pub parse_us: u64,
    /// Semantic extraction time in microseconds
    pub extract_us: u64,
    /// Call graph building time in microseconds
    pub graph_us: u64,
    /// Whether AST cache was used
    pub cached: bool,
    /// Number of incremental parses
    pub incremental_parses: usize,
    /// Number of full parses
    pub full_parses: usize,
    /// Number of cache hits (no reparse needed)
    pub cache_hits: usize,
}

/// Overall benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Total number of steps
    pub total_steps: usize,
    /// Total time in milliseconds
    pub total_time_ms: u64,
    /// All step snapshots
    pub snapshots: Vec<StepSnapshot>,
    /// Call graph evolution over time
    pub call_graph_evolution: CallGraphEvolution,
}

/// Timing comparison across all steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingComparison {
    /// Per-step timing data
    pub steps: Vec<StepTiming>,
    /// Summary statistics
    pub summary: TimingSummary,
}

/// Timing for a single step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTiming {
    pub step: usize,
    pub file: String,
    pub cached_total_us: u64,
    pub uncached_total_us: u64,
    pub speedup_ratio: f64,
}

/// Summary timing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingSummary {
    /// Average speedup ratio
    pub avg_speedup_ratio: f64,
    /// Total time with cache in milliseconds
    pub total_cached_time_ms: f64,
    /// Total time without cache in milliseconds
    pub total_uncached_time_ms: f64,
}

/// Call graph evolution across steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphEvolution {
    /// Per-step graph data
    pub steps: Vec<GraphStep>,
}

/// Graph state at a single step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStep {
    pub step: usize,
    /// Total number of edges
    pub edges: usize,
    /// Total number of nodes (symbols with at least one connection)
    pub nodes: usize,
    /// Edges added in this step
    pub new_edges: Vec<GraphEdge>,
    /// Full graph at this step
    pub graph: HashMap<String, Vec<String>>,
}

/// A single edge in the call graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

/// A file template for generation
#[derive(Debug, Clone)]
pub struct FileTemplate {
    /// Step number when this file is created
    pub step: usize,
    /// Relative file path
    pub path: &'static str,
    /// File content
    pub content: &'static str,
    /// Purpose description
    pub purpose: &'static str,
}

/// Total number of build steps
pub const TOTAL_STEPS: usize = 65;
