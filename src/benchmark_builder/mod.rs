//! Incremental Application Builder Benchmark
//!
//! This module builds an Event-driven API incrementally (60+ steps)
//! while capturing semantic analysis snapshots at each step.
//!
//! The benchmark compares cached (tree-sitter incremental) vs uncached (full parse)
//! performance and tracks call graph evolution, cognitive complexity, and timing.

pub mod generator;
pub mod runner;
pub mod snapshot;
pub mod templates;
pub mod types;

pub use runner::{run_benchmark, BenchmarkConfig};
pub use types::*;
