//! Main benchmark runner
//!
//! Orchestrates the incremental build process, generating files step by step
//! and capturing semantic analysis snapshots at each point.

use super::generator::{cleanup, generate_step, get_step_relative_path};
use super::snapshot::{calculate_graph_evolution, capture_snapshot};
use super::templates::total_steps;
use super::types::*;
use crate::server::AstCache;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Configuration for benchmark run
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Output directory for generated TypeScript files
    pub output_dir: String,
    /// Directory to save JSON results
    pub results_dir: String,
    /// Only run first N steps (for testing)
    pub max_steps: Option<usize>,
    /// Print progress to stdout
    pub verbose: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            output_dir: "benchmark_output".to_string(),
            results_dir: "benchmark_results".to_string(),
            max_steps: None,
            verbose: true,
        }
    }
}

/// Run the full incremental build benchmark
pub fn run_benchmark(config: BenchmarkConfig) -> Result<BenchmarkResults, String> {
    let output_dir = Path::new(&config.output_dir);
    let results_dir = Path::new(&config.results_dir);

    // Setup directories
    cleanup(output_dir).map_err(|e| format!("Failed to cleanup output dir: {}", e))?;
    fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create output dir: {}", e))?;
    fs::create_dir_all(results_dir).map_err(|e| format!("Failed to create results dir: {}", e))?;

    let total = config.max_steps.unwrap_or(total_steps());
    let benchmark_start = Instant::now();

    if config.verbose {
        println!("Starting incremental build benchmark ({} steps)", total);
        println!("Output: {}", config.output_dir);
        println!("Results: {}", config.results_dir);
        println!();
    }

    // Create AST cache for incremental parsing
    let ast_cache = Arc::new(AstCache::new());

    let mut snapshots: Vec<StepSnapshot> = Vec::new();
    let mut graph_evolution: Vec<GraphStep> = Vec::new();
    let mut prev_graph: HashMap<String, Vec<String>> = HashMap::new();

    for step in 1..=total {
        // Generate the file for this step
        let _file_path = generate_step(output_dir, step)
            .map_err(|e| format!("Failed to generate step {}: {}", step, e))?;

        let relative_path = get_step_relative_path(step).unwrap_or("unknown");

        if config.verbose {
            print!("Step {:>2}/{}: {:40}", step, total, relative_path);
        }

        // Capture snapshot
        let snapshot = capture_snapshot(output_dir, step, relative_path, Some(ast_cache.clone()))?;

        // Track call graph evolution
        let graph_step = calculate_graph_evolution(&prev_graph, &snapshot.call_graph, step);
        prev_graph = snapshot.call_graph.clone();

        // Print progress
        if config.verbose {
            let speedup = if snapshot.timing_uncached.total_us > 0 {
                snapshot.timing_uncached.total_us as f64
                    / snapshot.timing_cached.total_us.max(1) as f64
            } else {
                1.0
            };

            println!(
                " | symbols: {:>3} | cached: {:>6}µs | uncached: {:>6}µs | speedup: {:>5.1}x",
                snapshot.symbols.len(),
                snapshot.timing_cached.total_us,
                snapshot.timing_uncached.total_us,
                speedup
            );
        }

        // Save individual step snapshot
        save_step_snapshot(results_dir, &snapshot)?;

        graph_evolution.push(graph_step);
        snapshots.push(snapshot);
    }

    let total_time_ms = benchmark_start.elapsed().as_millis() as u64;

    // Build final results
    let results = BenchmarkResults {
        total_steps: total,
        total_time_ms,
        snapshots,
        call_graph_evolution: CallGraphEvolution {
            steps: graph_evolution,
        },
    };

    // Save summary results
    save_results(results_dir, &results)?;

    if config.verbose {
        println!();
        println!("Benchmark complete!");
        println!("Total time: {}ms", total_time_ms);
        println!("Results saved to: {}", config.results_dir);
    }

    Ok(results)
}

/// Save individual step snapshot to JSON
fn save_step_snapshot(results_dir: &Path, snapshot: &StepSnapshot) -> Result<(), String> {
    let filename = format!("step_{:03}.json", snapshot.step);
    let path = results_dir.join(filename);

    let json = serde_json::to_string_pretty(snapshot)
        .map_err(|e| format!("Failed to serialize step {}: {}", snapshot.step, e))?;

    fs::write(&path, json)
        .map_err(|e| format!("Failed to write step {} snapshot: {}", snapshot.step, e))?;

    Ok(())
}

/// Save final results summary
fn save_results(results_dir: &Path, results: &BenchmarkResults) -> Result<(), String> {
    // Save full results
    let full_path = results_dir.join("benchmark_results.json");
    let full_json = serde_json::to_string_pretty(results)
        .map_err(|e| format!("Failed to serialize results: {}", e))?;
    fs::write(&full_path, full_json).map_err(|e| format!("Failed to write results: {}", e))?;

    // Save call graph evolution separately for easy visualization
    let graph_path = results_dir.join("call_graph_evolution.json");
    let graph_json = serde_json::to_string_pretty(&results.call_graph_evolution)
        .map_err(|e| format!("Failed to serialize call graph: {}", e))?;
    fs::write(&graph_path, graph_json)
        .map_err(|e| format!("Failed to write call graph evolution: {}", e))?;

    // Save timing summary
    let timing_summary = create_timing_summary(results);
    let timing_path = results_dir.join("timing_summary.json");
    let timing_json = serde_json::to_string_pretty(&timing_summary)
        .map_err(|e| format!("Failed to serialize timing summary: {}", e))?;
    fs::write(&timing_path, timing_json)
        .map_err(|e| format!("Failed to write timing summary: {}", e))?;

    // Save complexity summary
    let complexity_summary = create_complexity_summary(results);
    let complexity_path = results_dir.join("complexity_summary.json");
    let complexity_json = serde_json::to_string_pretty(&complexity_summary)
        .map_err(|e| format!("Failed to serialize complexity summary: {}", e))?;
    fs::write(&complexity_path, complexity_json)
        .map_err(|e| format!("Failed to write complexity summary: {}", e))?;

    Ok(())
}

/// Create timing summary across all steps
fn create_timing_summary(results: &BenchmarkResults) -> serde_json::Value {
    let mut steps: Vec<serde_json::Value> = Vec::new();

    for snapshot in &results.snapshots {
        let speedup = if snapshot.timing_uncached.total_us > 0 {
            snapshot.timing_uncached.total_us as f64 / snapshot.timing_cached.total_us.max(1) as f64
        } else {
            1.0
        };

        steps.push(serde_json::json!({
            "step": snapshot.step,
            "file": snapshot.file,
            "total_files": snapshot.total_files,
            "cached_us": snapshot.timing_cached.total_us,
            "uncached_us": snapshot.timing_uncached.total_us,
            "speedup": speedup,
            "parse_cached_us": snapshot.timing_cached.parse_us,
            "parse_uncached_us": snapshot.timing_uncached.parse_us,
            "cache_hits": snapshot.timing_cached.cache_hits,
            "incremental_parses": snapshot.timing_cached.incremental_parses,
            "full_parses": snapshot.timing_cached.full_parses,
        }));
    }

    // Calculate averages
    let total_cached: u64 = results
        .snapshots
        .iter()
        .map(|s| s.timing_cached.total_us)
        .sum();
    let total_uncached: u64 = results
        .snapshots
        .iter()
        .map(|s| s.timing_uncached.total_us)
        .sum();
    let avg_speedup = if total_cached > 0 {
        total_uncached as f64 / total_cached as f64
    } else {
        1.0
    };

    serde_json::json!({
        "total_steps": results.total_steps,
        "total_time_ms": results.total_time_ms,
        "total_cached_us": total_cached,
        "total_uncached_us": total_uncached,
        "avg_speedup": avg_speedup,
        "steps": steps,
    })
}

/// Create complexity summary across all steps
fn create_complexity_summary(results: &BenchmarkResults) -> serde_json::Value {
    let mut steps: Vec<serde_json::Value> = Vec::new();

    for snapshot in &results.snapshots {
        steps.push(serde_json::json!({
            "step": snapshot.step,
            "file": snapshot.file,
            "symbols_count": snapshot.symbols.len(),
            "total_cognitive": snapshot.complexity.total_cognitive,
            "avg_cognitive": snapshot.complexity.avg_cognitive,
            "max_cognitive": snapshot.complexity.max_cognitive,
            "high_risk_count": snapshot.complexity.high_risk_count,
        }));
    }

    // Get final totals
    let final_snapshot = results.snapshots.last();
    let (total_symbols, total_cognitive, high_risk) = if let Some(s) = final_snapshot {
        (
            s.symbols.len(),
            s.complexity.total_cognitive,
            s.complexity.high_risk_count,
        )
    } else {
        (0, 0, 0)
    };

    serde_json::json!({
        "total_steps": results.total_steps,
        "final_symbol_count": total_symbols,
        "final_cognitive_complexity": total_cognitive,
        "final_high_risk_count": high_risk,
        "steps": steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_run_benchmark_first_5_steps() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path().join("output");
        let results_dir = temp_dir.path().join("results");

        let config = BenchmarkConfig {
            output_dir: output_dir.to_string_lossy().to_string(),
            results_dir: results_dir.to_string_lossy().to_string(),
            max_steps: Some(5),
            verbose: false,
        };

        let results = run_benchmark(config).unwrap();

        assert_eq!(results.total_steps, 5);
        assert_eq!(results.snapshots.len(), 5);
        assert_eq!(results.call_graph_evolution.steps.len(), 5);

        // Check that result files were created
        assert!(results_dir.join("benchmark_results.json").exists());
        assert!(results_dir.join("call_graph_evolution.json").exists());
        assert!(results_dir.join("timing_summary.json").exists());
        assert!(results_dir.join("complexity_summary.json").exists());

        // Check individual step files
        for i in 1..=5 {
            let step_file = results_dir.join(format!("step_{:03}.json", i));
            assert!(
                step_file.exists(),
                "Missing step file: {}",
                step_file.display()
            );
        }
    }
}
