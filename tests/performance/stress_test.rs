//! Stress tests for concurrent operations
//!
//! Tests system behavior under heavy load with multiple repos and concurrent queries.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use semfora_engine::cache::CacheDir;
use semfora_engine::socket_server::{index_directory, IndexOptions};

/// Get test repos directory
fn test_repos_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/home/kadajett/Dev/semfora-test-repos/repos"),
        PathBuf::from("../semfora-test-repos/repos"),
        std::env::var("SEMFORA_TEST_REPOS")
            .map(PathBuf::from)
            .ok(),
    ];

    for path in candidates.into_iter().flatten() {
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Find test repos
fn find_test_repos(limit: usize) -> Vec<(String, PathBuf)> {
    let Some(repos_dir) = test_repos_dir() else {
        return vec![];
    };

    let mut repos = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&repos_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name() {
                    repos.push((name.to_string_lossy().to_string(), path));
                }
            }
        }
    }

    repos.sort_by(|a, b| a.0.cmp(&b.0));
    repos.into_iter().take(limit).collect()
}

#[derive(Debug)]
struct StressTestResults {
    total_operations: usize,
    successful_operations: usize,
    failed_operations: usize,
    duration_ms: u64,
    operations_per_second: f64,
    avg_latency_ms: f64,
}

impl std::fmt::Display for StressTestResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {} | Success: {} | Failed: {} | Duration: {}ms | OPS: {:.1} | Avg Latency: {:.2}ms",
            self.total_operations,
            self.successful_operations,
            self.failed_operations,
            self.duration_ms,
            self.operations_per_second,
            self.avg_latency_ms
        )
    }
}

/// Index multiple repos in parallel
fn parallel_indexing(repos: &[(String, PathBuf)]) -> StressTestResults {
    let start_time = Instant::now();
    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = repos
        .iter()
        .map(|(name, path)| {
            let name = name.clone();
            let path = path.clone();
            let success = Arc::clone(&success_count);
            let fail = Arc::clone(&fail_count);

            thread::spawn(move || {
                let temp_dir = tempfile::tempdir().unwrap();
                let cache = CacheDir {
                    root: temp_dir.path().to_path_buf(),
                    repo_root: path.clone(),
                    repo_hash: format!("stress_{}", name),
                };

                if cache.init().is_err() {
                    fail.fetch_add(1, Ordering::SeqCst);
                    return;
                }

                let options = IndexOptions::default();
                match index_directory(&path, cache, &options) {
                    Ok(_) => {
                        success.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        fail.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }

    let duration = start_time.elapsed();
    let successful = success_count.load(Ordering::SeqCst);
    let failed = fail_count.load(Ordering::SeqCst);
    let total = successful + failed;

    StressTestResults {
        total_operations: total,
        successful_operations: successful,
        failed_operations: failed,
        duration_ms: duration.as_millis() as u64,
        operations_per_second: if duration.as_secs_f64() > 0.0 {
            total as f64 / duration.as_secs_f64()
        } else {
            0.0
        },
        avg_latency_ms: if total > 0 {
            duration.as_millis() as f64 / total as f64
        } else {
            0.0
        },
    }
}

/// Run concurrent queries on a single index
fn concurrent_query_stress(cache: &CacheDir, num_threads: usize, queries_per_thread: usize) -> StressTestResults {
    let cache = Arc::new(cache.clone());
    let start_time = Instant::now();
    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let total_latency_us = Arc::new(AtomicUsize::new(0));

    let patterns = ["function", "export", "async", "const", "class", "interface"];

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let cache = Arc::clone(&cache);
            let success = Arc::clone(&success_count);
            let fail = Arc::clone(&fail_count);
            let latency = Arc::clone(&total_latency_us);

            thread::spawn(move || {
                for i in 0..queries_per_thread {
                    let pattern = patterns[(thread_id + i) % patterns.len()];
                    let query_start = Instant::now();

                    match cache.search_symbols(pattern, None, None, None, 20) {
                        Ok(_) => {
                            success.fetch_add(1, Ordering::SeqCst);
                        }
                        Err(_) => {
                            fail.fetch_add(1, Ordering::SeqCst);
                        }
                    }

                    let query_time = query_start.elapsed().as_micros() as usize;
                    latency.fetch_add(query_time, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }

    let duration = start_time.elapsed();
    let successful = success_count.load(Ordering::SeqCst);
    let failed = fail_count.load(Ordering::SeqCst);
    let total = successful + failed;
    let total_latency = total_latency_us.load(Ordering::SeqCst);

    StressTestResults {
        total_operations: total,
        successful_operations: successful,
        failed_operations: failed,
        duration_ms: duration.as_millis() as u64,
        operations_per_second: if duration.as_secs_f64() > 0.0 {
            total as f64 / duration.as_secs_f64()
        } else {
            0.0
        },
        avg_latency_ms: if total > 0 {
            (total_latency as f64 / total as f64) / 1000.0
        } else {
            0.0
        },
    }
}

#[test]
#[ignore] // Run with: cargo test --test performance stress_test -- --ignored --nocapture
fn test_parallel_indexing_stress() {
    let repos = find_test_repos(10);

    if repos.is_empty() {
        println!("No test repos found. Set SEMFORA_TEST_REPOS env var.");
        return;
    }

    println!("\n========================================");
    println!("  Parallel Indexing Stress Test");
    println!("  Repos: {}", repos.len());
    println!("========================================\n");

    let results = parallel_indexing(&repos);
    println!("{}", results);

    // Assert basic success
    assert!(results.successful_operations > 0, "At least one index should succeed");
}

#[test]
#[ignore]
fn test_concurrent_query_stress() {
    let repos = find_test_repos(1);

    if repos.is_empty() {
        println!("No test repos found. Set SEMFORA_TEST_REPOS env var.");
        return;
    }

    let (repo_name, repo_path) = &repos[0];

    println!("\n========================================");
    println!("  Concurrent Query Stress Test");
    println!("  Repo: {}", repo_name);
    println!("========================================\n");

    // First, create an index
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = CacheDir {
        root: temp_dir.path().to_path_buf(),
        repo_root: repo_path.clone(),
        repo_hash: format!("query_stress_{}", repo_name),
    };
    cache.init().unwrap();

    println!("Creating index...");
    let options = IndexOptions::default();
    let _ = index_directory(repo_path, cache.clone(), &options);
    println!("Index created.\n");

    // Run stress tests with different thread counts
    for num_threads in [1, 2, 4, 8, 16] {
        let queries_per_thread = 100;

        println!("--- {} threads, {} queries each ---", num_threads, queries_per_thread);
        let results = concurrent_query_stress(&cache, num_threads, queries_per_thread);
        println!("{}\n", results);
    }
}

#[test]
#[ignore]
fn test_multi_repo_concurrent_queries() {
    let repos = find_test_repos(5);

    if repos.len() < 2 {
        println!("Need at least 2 test repos. Set SEMFORA_TEST_REPOS env var.");
        return;
    }

    println!("\n========================================");
    println!("  Multi-Repo Concurrent Query Test");
    println!("  Repos: {}", repos.len());
    println!("========================================\n");

    let options = IndexOptions::default();

    // Create indexes for all repos
    let mut caches = Vec::new();
    let temp_dirs: Vec<_> = repos
        .iter()
        .map(|(name, path)| {
            let temp_dir = tempfile::tempdir().unwrap();
            let cache = CacheDir {
                root: temp_dir.path().to_path_buf(),
                repo_root: path.clone(),
                repo_hash: format!("multi_{}", name),
            };
            cache.init().unwrap();

            println!("Indexing {}...", name);
            let _ = index_directory(path, cache.clone(), &options);

            caches.push(cache);
            temp_dir
        })
        .collect();

    println!("\nRunning concurrent queries across all repos...\n");

    let start_time = Instant::now();
    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let queries_per_repo = 50;

    let handles: Vec<_> = caches
        .into_iter()
        .enumerate()
        .map(|(idx, cache)| {
            let success = Arc::clone(&success_count);
            let fail = Arc::clone(&fail_count);

            thread::spawn(move || {
                let patterns = ["function", "export", "async", "const", "class"];

                for i in 0..queries_per_repo {
                    let pattern = patterns[(idx + i) % patterns.len()];
                    match cache.search_symbols(pattern, None, None, None, 20) {
                        Ok(_) => success.fetch_add(1, Ordering::SeqCst),
                        Err(_) => fail.fetch_add(1, Ordering::SeqCst),
                    };
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }

    let duration = start_time.elapsed();
    let successful = success_count.load(Ordering::SeqCst);
    let failed = fail_count.load(Ordering::SeqCst);
    let total = successful + failed;

    let results = StressTestResults {
        total_operations: total,
        successful_operations: successful,
        failed_operations: failed,
        duration_ms: duration.as_millis() as u64,
        operations_per_second: if duration.as_secs_f64() > 0.0 {
            total as f64 / duration.as_secs_f64()
        } else {
            0.0
        },
        avg_latency_ms: if total > 0 {
            duration.as_millis() as f64 / total as f64
        } else {
            0.0
        },
    };

    println!("{}", results);
    assert!(results.successful_operations > 0);

    // Keep temp_dirs alive
    drop(temp_dirs);
}

#[test]
#[ignore]
fn test_sustained_load() {
    let repos = find_test_repos(1);

    if repos.is_empty() {
        println!("No test repos found. Set SEMFORA_TEST_REPOS env var.");
        return;
    }

    let (repo_name, repo_path) = &repos[0];

    println!("\n========================================");
    println!("  Sustained Load Test (30 seconds)");
    println!("  Repo: {}", repo_name);
    println!("========================================\n");

    // Create index
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = CacheDir {
        root: temp_dir.path().to_path_buf(),
        repo_root: repo_path.clone(),
        repo_hash: format!("sustained_{}", repo_name),
    };
    cache.init().unwrap();

    let options = IndexOptions::default();
    let _ = index_directory(repo_path, cache.clone(), &options);

    let cache = Arc::new(cache);
    let test_duration = Duration::from_secs(30);
    let start_time = Instant::now();

    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let num_threads = 4;
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let cache = Arc::clone(&cache);
            let success = Arc::clone(&success_count);
            let fail = Arc::clone(&fail_count);
            let start = start_time;
            let duration = test_duration;

            thread::spawn(move || {
                let patterns = ["function", "export", "async", "const", "class"];
                let mut i = 0;

                while start.elapsed() < duration {
                    let pattern = patterns[(thread_id + i) % patterns.len()];
                    match cache.search_symbols(pattern, None, None, None, 20) {
                        Ok(_) => success.fetch_add(1, Ordering::SeqCst),
                        Err(_) => fail.fetch_add(1, Ordering::SeqCst),
                    };
                    i += 1;
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }

    let actual_duration = start_time.elapsed();
    let successful = success_count.load(Ordering::SeqCst);
    let failed = fail_count.load(Ordering::SeqCst);
    let total = successful + failed;

    println!("Duration: {:.2}s", actual_duration.as_secs_f64());
    println!("Total queries: {}", total);
    println!("Successful: {}", successful);
    println!("Failed: {}", failed);
    println!("QPS: {:.1}", total as f64 / actual_duration.as_secs_f64());

    assert!(failed == 0, "No queries should fail under sustained load");
}
