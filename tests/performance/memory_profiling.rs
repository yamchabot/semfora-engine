//! Memory profiling tests
//!
//! Measures memory usage during indexing and querying operations.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use memory_stats::memory_stats;
use semfora_engine::cache::CacheDir;
use semfora_engine::socket_server::{index_directory, IndexOptions};

/// Get memory usage in MB
fn get_memory_mb() -> f64 {
    memory_stats()
        .map(|stats| stats.physical_mem as f64 / 1024.0 / 1024.0)
        .unwrap_or(0.0)
}

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

/// Find repos sorted by approximate size
fn find_test_repos() -> Vec<(String, PathBuf)> {
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
    repos
}

#[derive(Debug)]
struct MemoryProfile {
    operation: String,
    repo_name: String,
    start_memory_mb: f64,
    peak_memory_mb: f64,
    end_memory_mb: f64,
    duration_ms: u64,
}

impl std::fmt::Display for MemoryProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<20} | {:<25} | Start: {:>8.2} MB | Peak: {:>8.2} MB | End: {:>8.2} MB | Time: {:>6} ms",
            self.operation,
            self.repo_name,
            self.start_memory_mb,
            self.peak_memory_mb,
            self.end_memory_mb,
            self.duration_ms
        )
    }
}

/// Profile indexing memory usage
fn profile_indexing(repo_path: &PathBuf, repo_name: &str) -> MemoryProfile {
    let start_memory = get_memory_mb();
    let mut peak_memory = start_memory;

    let start_time = Instant::now();

    // Create temp cache
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = CacheDir {
        root: temp_dir.path().to_path_buf(),
        repo_root: repo_path.clone(),
        repo_hash: format!("memtest_{}", repo_name),
    };
    cache.init().unwrap();

    // Index with memory sampling
    let options = IndexOptions::default();

    // We can't easily sample during index_directory, so just measure before/after
    let _ = index_directory(repo_path, cache, &options);

    // Sample after indexing
    let after_index = get_memory_mb();
    if after_index > peak_memory {
        peak_memory = after_index;
    }

    let end_memory = get_memory_mb();
    let duration = start_time.elapsed();

    MemoryProfile {
        operation: "indexing".to_string(),
        repo_name: repo_name.to_string(),
        start_memory_mb: start_memory,
        peak_memory_mb: peak_memory.max(end_memory),
        end_memory_mb: end_memory,
        duration_ms: duration.as_millis() as u64,
    }
}

/// Profile query memory usage
fn profile_queries(cache: &CacheDir, repo_name: &str) -> Vec<MemoryProfile> {
    let mut profiles = Vec::new();

    // Profile search_symbols
    {
        let start_memory = get_memory_mb();
        let start_time = Instant::now();

        for _ in 0..100 {
            let _ = cache.search_symbols("function", None, None, None, 20);
        }

        let end_memory = get_memory_mb();
        let duration = start_time.elapsed();

        profiles.push(MemoryProfile {
            operation: "search_symbols x100".to_string(),
            repo_name: repo_name.to_string(),
            start_memory_mb: start_memory,
            peak_memory_mb: end_memory.max(start_memory),
            end_memory_mb: end_memory,
            duration_ms: duration.as_millis() as u64,
        });
    }

    // Profile get_repo_overview
    {
        let start_memory = get_memory_mb();
        let start_time = Instant::now();

        for _ in 0..10 {
            let overview_path = cache.repo_overview_path();
            let _ = std::fs::read_to_string(&overview_path);
        }

        let end_memory = get_memory_mb();
        let duration = start_time.elapsed();

        profiles.push(MemoryProfile {
            operation: "repo_overview x10".to_string(),
            repo_name: repo_name.to_string(),
            start_memory_mb: start_memory,
            peak_memory_mb: end_memory.max(start_memory),
            end_memory_mb: end_memory,
            duration_ms: duration.as_millis() as u64,
        });
    }

    // Profile get_call_graph
    {
        let start_memory = get_memory_mb();
        let start_time = Instant::now();

        for _ in 0..10 {
            let _ = cache.load_call_graph();
        }

        let end_memory = get_memory_mb();
        let duration = start_time.elapsed();

        profiles.push(MemoryProfile {
            operation: "call_graph x10".to_string(),
            repo_name: repo_name.to_string(),
            start_memory_mb: start_memory,
            peak_memory_mb: end_memory.max(start_memory),
            end_memory_mb: end_memory,
            duration_ms: duration.as_millis() as u64,
        });
    }

    profiles
}

#[test]
#[ignore] // Run with: cargo test --test performance memory_profiling -- --ignored --nocapture
fn test_memory_profiling() {
    let repos = find_test_repos();

    if repos.is_empty() {
        println!("No test repos found. Set SEMFORA_TEST_REPOS env var.");
        return;
    }

    println!("\n========================================");
    println!("  Memory Profiling Results");
    println!("========================================\n");

    // Test on up to 5 repos
    for (repo_name, repo_path) in repos.iter().take(5) {
        println!("\n--- {} ---", repo_name);

        // Profile indexing
        let indexing_profile = profile_indexing(repo_path, repo_name);
        println!("{}", indexing_profile);

        // Create a persistent index for query profiling
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = CacheDir {
            root: temp_dir.path().to_path_buf(),
            repo_root: repo_path.clone(),
            repo_hash: format!("query_test_{}", repo_name),
        };
        cache.init().unwrap();

        let options = IndexOptions::default();
        let _ = index_directory(repo_path, cache.clone(), &options);

        // Profile queries
        let query_profiles = profile_queries(&cache, repo_name);
        for profile in query_profiles {
            println!("{}", profile);
        }
    }

    println!("\n========================================");
}

#[test]
#[ignore]
fn test_large_repo_memory() {
    let repos = find_test_repos();

    // Find a large repo (by name heuristic)
    let large_repos = ["typescript-eslint", "babel", "puppeteer", "playwright"];

    for name in large_repos {
        if let Some((repo_name, repo_path)) = repos.iter().find(|(n, _)| n == name) {
            println!("\n=== Large Repo Memory Test: {} ===\n", repo_name);

            let profile = profile_indexing(repo_path, repo_name);
            println!("{}", profile);

            // Calculate throughput
            let file_count = count_source_files(repo_path);
            let throughput = if profile.duration_ms > 0 {
                (file_count as f64 / profile.duration_ms as f64) * 1000.0
            } else {
                0.0
            };

            println!(
                "Files: {} | Throughput: {:.1} files/sec | Memory delta: {:.2} MB",
                file_count,
                throughput,
                profile.peak_memory_mb - profile.start_memory_mb
            );

            return;
        }
    }

    println!("No large repos found for memory test");
}

/// Count source files in a directory
fn count_source_files(dir: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(walker) = ignore::WalkBuilder::new(dir).build().into_iter().flatten() {
        if let Some(ft) = walker.file_type() {
            if ft.is_file() {
                let path = walker.path();
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(
                        ext.as_str(),
                        "ts" | "tsx" | "js" | "jsx" | "rs" | "py" | "go" | "java"
                    ) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
