//! CLI entry point for the benchmark builder
//!
//! Usage:
//!   cargo run --release --bin semfora-benchmark-builder -- [OPTIONS]
//!
//! Options:
//!   --output-dir <PATH>   Directory for generated TypeScript files (default: benchmark_output)
//!   --results-dir <PATH>  Directory for JSON results (default: benchmark_results)
//!   --steps <N>           Only run first N steps (for testing)
//!   --quiet               Suppress progress output
//!   --help                Show this help message

use semfora_engine::benchmark_builder::{run_benchmark, BenchmarkConfig};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    let config = parse_args(&args);

    match run_benchmark(config) {
        Ok(results) => {
            println!("\nBenchmark Summary:");
            println!("  Total steps: {}", results.total_steps);
            println!("  Total time: {}ms", results.total_time_ms);

            if let Some(final_snapshot) = results.snapshots.last() {
                println!("  Final symbol count: {}", final_snapshot.symbols.len());
                println!("  Final cognitive complexity: {}", final_snapshot.complexity.total_cognitive);
                println!("  High-risk symbols: {}", final_snapshot.complexity.high_risk_count);

                // Calculate overall speedup
                let total_cached: u64 = results.snapshots.iter()
                    .map(|s| s.timing_cached.total_us)
                    .sum();
                let total_uncached: u64 = results.snapshots.iter()
                    .map(|s| s.timing_uncached.total_us)
                    .sum();

                if total_cached > 0 {
                    let speedup = total_uncached as f64 / total_cached as f64;
                    println!("  Average cache speedup: {:.1}x", speedup);
                }

                // Call graph stats
                let final_graph = &results.call_graph_evolution.steps.last();
                if let Some(graph_step) = final_graph {
                    println!("  Call graph nodes: {}", graph_step.nodes);
                    println!("  Call graph edges: {}", graph_step.edges);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_args(args: &[String]) -> BenchmarkConfig {
    let mut config = BenchmarkConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output-dir" => {
                if i + 1 < args.len() {
                    config.output_dir = args[i + 1].clone();
                    i += 1;
                }
            }
            "--results-dir" => {
                if i + 1 < args.len() {
                    config.results_dir = args[i + 1].clone();
                    i += 1;
                }
            }
            "--steps" => {
                if i + 1 < args.len() {
                    if let Ok(n) = args[i + 1].parse() {
                        config.max_steps = Some(n);
                    }
                    i += 1;
                }
            }
            "--quiet" | "-q" => {
                config.verbose = false;
            }
            _ => {}
        }
        i += 1;
    }

    config
}

fn print_help() {
    println!("Semfora Benchmark Builder");
    println!();
    println!("Incrementally builds an Event-driven API application (65 steps)");
    println!("while capturing semantic analysis snapshots at each step.");
    println!();
    println!("USAGE:");
    println!("    cargo run --release --bin semfora-benchmark-builder -- [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --output-dir <PATH>   Directory for generated TypeScript files");
    println!("                          (default: benchmark_output)");
    println!("    --results-dir <PATH>  Directory for JSON results");
    println!("                          (default: benchmark_results)");
    println!("    --steps <N>           Only run first N steps (for testing)");
    println!("    --quiet, -q           Suppress progress output");
    println!("    --help, -h            Show this help message");
    println!();
    println!("OUTPUT FILES:");
    println!("    benchmark_results.json     Full results with all snapshots");
    println!("    call_graph_evolution.json  Call graph growth over time");
    println!("    timing_summary.json        Timing comparison (cached vs uncached)");
    println!("    complexity_summary.json    Cognitive complexity metrics");
    println!("    step_001.json ... step_065.json  Individual step snapshots");
    println!();
    println!("EXAMPLE:");
    println!("    # Run first 10 steps for testing");
    println!("    cargo run --release --bin semfora-benchmark-builder -- --steps 10");
    println!();
    println!("    # Full benchmark with custom directories");
    println!("    cargo run --release --bin semfora-benchmark-builder -- \\");
    println!("        --output-dir ./temp_build --results-dir ./results");
}
