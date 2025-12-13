//! Indexing performance benchmarks
//!
//! Measures time and throughput for creating semantic indexes on repos of various sizes.
//!
//! Run with: cargo bench --bench indexing

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::PathBuf;
use std::time::Duration;

use semfora_engine::cache::CacheDir;
use semfora_engine::socket_server::{index_directory, IndexOptions};

/// Get path to test repos directory
fn test_repos_dir() -> PathBuf {
    // Check common locations
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

/// Small repos for quick benchmarks
const SMALL_REPOS: &[&str] = &[
    "nestjs-starter",
    "react-realworld",
    "angular-realworld",
    "sample-hugo",
];

/// Medium repos for standard benchmarks
const MEDIUM_REPOS: &[&str] = &[
    "express-examples",
    "fastify-examples",
    "koa-examples",
    "zod",
    "routing-controllers",
];

/// Large repos for stress testing
const LARGE_REPOS: &[&str] = &[
    "typescript-eslint",
    "babel",
    "puppeteer",
    "playwright",
    "nextjs-examples",
];

/// Count source files in a directory
fn count_source_files(dir: &PathBuf) -> usize {
    let mut count = 0;
    let walker = ignore::WalkBuilder::new(dir).build();
    for entry in walker.flatten() {
        if let Some(ft) = entry.file_type() {
            if ft.is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(
                        ext.as_str(),
                        "ts" | "tsx"
                            | "js"
                            | "jsx"
                            | "rs"
                            | "py"
                            | "go"
                            | "java"
                            | "c"
                            | "cpp"
                            | "h"
                            | "hpp"
                    ) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

fn bench_index_creation(c: &mut Criterion) {
    let repos_dir = test_repos_dir();
    let options = IndexOptions::default();

    let mut group = c.benchmark_group("index_creation");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    for repo_name in SMALL_REPOS {
        let repo_path = repos_dir.join(repo_name);
        if !repo_path.exists() {
            eprintln!("Skipping {}: not found", repo_name);
            continue;
        }

        let file_count = count_source_files(&repo_path);
        group.throughput(Throughput::Elements(file_count as u64));

        group.bench_with_input(
            BenchmarkId::new("small", repo_name),
            &repo_path,
            |b, path| {
                b.iter(|| {
                    // Create temp cache dir for each iteration
                    let temp_dir = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_dir.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    // Run full indexing
                    let _ = index_directory(black_box(path), cache, &options);
                });
            },
        );
    }

    for repo_name in MEDIUM_REPOS {
        let repo_path = repos_dir.join(repo_name);
        if !repo_path.exists() {
            continue;
        }

        let file_count = count_source_files(&repo_path);
        group.throughput(Throughput::Elements(file_count as u64));

        group.bench_with_input(
            BenchmarkId::new("medium", repo_name),
            &repo_path,
            |b, path| {
                b.iter(|| {
                    let temp_dir = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_dir.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    let _ = index_directory(black_box(path), cache, &options);
                });
            },
        );
    }

    group.finish();
}

fn bench_large_repos(c: &mut Criterion) {
    let repos_dir = test_repos_dir();
    let options = IndexOptions::default();

    let mut group = c.benchmark_group("large_repo_indexing");
    group.sample_size(5);
    group.measurement_time(Duration::from_secs(120));

    for repo_name in LARGE_REPOS {
        let repo_path = repos_dir.join(repo_name);
        if !repo_path.exists() {
            continue;
        }

        let file_count = count_source_files(&repo_path);
        group.throughput(Throughput::Elements(file_count as u64));

        group.bench_with_input(BenchmarkId::new("large", repo_name), &repo_path, |b, path| {
            b.iter(|| {
                let temp_dir = tempfile::tempdir().unwrap();
                let cache = CacheDir {
                    root: temp_dir.path().to_path_buf(),
                    repo_root: path.clone(),
                    repo_hash: format!("bench_{}", repo_name),
                };
                cache.init().unwrap();

                let _ = index_directory(black_box(path), cache, &options);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_index_creation, bench_large_repos);
criterion_main!(benches);
