//! Query performance benchmarks
//!
//! Measures latency for various query operations on pre-built indexes.
//!
//! Run with: cargo bench --bench queries

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::time::Duration;

use semfora_engine::cache::CacheDir;
use semfora_engine::socket_server::{index_directory, IndexOptions};

/// Get path to test repos directory
fn test_repos_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("/home/kadajett/Dev/semfora-test-repos/repos"),
        PathBuf::from("../semfora-test-repos/repos"),
        std::env::var("SEMFORA_TEST_REPOS")
            .map(PathBuf::from)
            .unwrap_or_default(),
    ];

    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    panic!("Test repos directory not found. Set SEMFORA_TEST_REPOS env var.");
}

/// Repos to use for query benchmarks (need existing indexes)
const QUERY_REPOS: &[&str] = &["zod", "express-examples", "react-realworld"];

/// Common search patterns
const SEARCH_PATTERNS: &[&str] = &[
    "function", "export", "handler", "error", "async", "render", "parse",
];

/// Set up a pre-indexed repo for query benchmarks
fn setup_indexed_repo(repo_name: &str) -> Option<(CacheDir, PathBuf)> {
    let repos_dir = test_repos_dir();
    let repo_path = repos_dir.join(repo_name);

    if !repo_path.exists() {
        return None;
    }

    // Create temp cache and index the repo
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = CacheDir {
        root: temp_dir.path().to_path_buf(),
        repo_root: repo_path.clone(),
        repo_hash: format!("bench_{}", repo_name),
    };
    cache.init().unwrap();

    // Index the repo
    let options = IndexOptions::default();
    let _ = index_directory(&repo_path, cache.clone(), &options);

    // Leak the tempdir so it persists for the benchmark
    std::mem::forget(temp_dir);

    Some((cache, repo_path))
}

fn bench_search_symbols(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_symbols");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            eprintln!("Skipping {}: not found or failed to index", repo_name);
            continue;
        };

        for pattern in SEARCH_PATTERNS {
            let cache_clone = cache.clone();
            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}", repo_name, pattern), pattern),
                pattern,
                |b, pattern| {
                    b.iter(|| {
                        let _ =
                            cache_clone.search_symbols(black_box(pattern), None, None, None, 20);
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_get_symbol(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_symbol");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            continue;
        };

        // Get some symbol hashes to look up
        let symbols = cache
            .search_symbols("function", None, None, None, 10)
            .unwrap_or_default();

        if symbols.is_empty() {
            continue;
        }

        // Extract just the hashes
        let hashes: Vec<String> = symbols.iter().map(|s| s.hash.clone()).collect();

        // Benchmark looking up symbols by hash
        let cache_clone = cache.clone();
        group.bench_with_input(
            BenchmarkId::new(*repo_name, "by_hash"),
            &hashes,
            |b, hashes| {
                let mut idx = 0;
                b.iter(|| {
                    let hash = &hashes[idx % hashes.len()];
                    // Read symbol file directly
                    let symbol_path = cache_clone.symbol_path(black_box(hash));
                    let _ = std::fs::read_to_string(&symbol_path);
                    idx += 1;
                });
            },
        );
    }

    group.finish();
}

fn bench_list_symbols(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_symbols");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            continue;
        };

        // Get available modules
        let modules = cache.list_modules();

        for module in modules.iter().take(3) {
            let cache_clone = cache.clone();
            let module_clone = module.clone();
            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}", repo_name, module), module),
                module,
                |b, _module| {
                    b.iter(|| {
                        let _ = cache_clone.list_module_symbols(
                            black_box(&module_clone),
                            None,
                            None,
                            50,
                        );
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_get_module(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_module");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(15));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            continue;
        };

        let modules = cache.list_modules();

        for module in modules.iter().take(3) {
            let cache_clone = cache.clone();
            let module_clone = module.clone();
            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}", repo_name, module), module),
                module,
                |b, _module| {
                    b.iter(|| {
                        // Load full module summaries
                        let _ = cache_clone.load_module_summaries(black_box(&module_clone));
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_get_call_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_call_graph");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(15));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            continue;
        };

        let cache_clone = cache.clone();
        group.bench_with_input(BenchmarkId::new(*repo_name, "full"), repo_name, |b, _| {
            b.iter(|| {
                let _ = cache_clone.load_call_graph();
            });
        });
    }

    group.finish();
}

fn bench_repo_overview(c: &mut Criterion) {
    let mut group = c.benchmark_group("repo_overview");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    for repo_name in QUERY_REPOS {
        let Some((cache, _)) = setup_indexed_repo(repo_name) else {
            continue;
        };

        let cache_clone = cache.clone();
        group.bench_with_input(
            BenchmarkId::new(*repo_name, "overview"),
            repo_name,
            |b, _| {
                b.iter(|| {
                    // Read repo overview file
                    let overview_path = cache_clone.repo_overview_path();
                    let _ = std::fs::read_to_string(&overview_path);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_search_symbols,
    bench_get_symbol,
    bench_list_symbols,
    bench_get_module,
    bench_get_call_graph,
    bench_repo_overview,
);
criterion_main!(benches);
