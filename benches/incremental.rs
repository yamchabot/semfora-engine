//! Incremental reindexing benchmarks
//!
//! Measures performance of incremental index updates after file changes.
//!
//! Run with: cargo bench --bench incremental

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
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

/// Repos to use for incremental benchmarks
const INCREMENTAL_REPOS: &[&str] = &[
    "zod",
    "react-realworld",
    "express-examples",
];

/// Sample TypeScript content for new files
const SAMPLE_TS_CONTENT: &str = r#"
import { z } from 'zod';

export interface UserProfile {
    id: string;
    name: string;
    email: string;
    createdAt: Date;
}

export const userProfileSchema = z.object({
    id: z.string().uuid(),
    name: z.string().min(1).max(100),
    email: z.string().email(),
    createdAt: z.date(),
});

export async function fetchUserProfile(id: string): Promise<UserProfile> {
    const response = await fetch(`/api/users/${id}`);
    if (!response.ok) {
        throw new Error(`Failed to fetch user: ${response.statusText}`);
    }
    const data = await response.json();
    return userProfileSchema.parse(data);
}

export function validateUserProfile(profile: unknown): UserProfile {
    return userProfileSchema.parse(profile);
}

export class UserService {
    private cache: Map<string, UserProfile> = new Map();

    async getUser(id: string): Promise<UserProfile> {
        const cached = this.cache.get(id);
        if (cached) {
            return cached;
        }
        const profile = await fetchUserProfile(id);
        this.cache.set(id, profile);
        return profile;
    }

    invalidateCache(id: string): void {
        this.cache.delete(id);
    }

    clearCache(): void {
        this.cache.clear();
    }
}
"#;

/// Create a temporary copy of a repo for modification testing
fn create_temp_repo_copy(repo_name: &str) -> Option<(tempfile::TempDir, PathBuf)> {
    let repos_dir = test_repos_dir();
    let source_path = repos_dir.join(repo_name);

    if !source_path.exists() {
        return None;
    }

    // Create temp directory
    let temp_dir = tempfile::tempdir().unwrap();
    let dest_path = temp_dir.path().join(repo_name);

    // Copy repo (shallow copy of top-level files only for speed)
    copy_dir_shallow(&source_path, &dest_path).ok()?;

    Some((temp_dir, dest_path))
}

/// Shallow copy of a directory (only immediate children)
fn copy_dir_shallow(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    let walker = ignore::WalkBuilder::new(src)
        .max_depth(Some(3)) // Shallow copy
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap();
        let dest = dst.join(relative);

        if path.is_dir() {
            fs::create_dir_all(&dest)?;
        } else if path.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest)?;
        }
    }

    Ok(())
}

fn bench_full_reindex(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_reindex");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    let options = IndexOptions::default();

    for repo_name in INCREMENTAL_REPOS {
        let Some((temp_repo_dir, repo_path)) = create_temp_repo_copy(repo_name) else {
            eprintln!("Skipping {}: not found", repo_name);
            continue;
        };

        group.bench_with_input(
            BenchmarkId::new(*repo_name, "full"),
            &repo_path,
            |b, path| {
                b.iter(|| {
                    let temp_cache = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_cache.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    let _ = index_directory(black_box(path), cache, &options);
                });
            },
        );

        drop(temp_repo_dir);
    }

    group.finish();
}

fn bench_single_file_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_file_add");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(20));
    let options = IndexOptions::default();

    for repo_name in INCREMENTAL_REPOS {
        let Some((_temp_repo_dir, repo_path)) = create_temp_repo_copy(repo_name) else {
            continue;
        };

        // Create initial index
        let temp_cache = tempfile::tempdir().unwrap();
        let cache = CacheDir {
            root: temp_cache.path().to_path_buf(),
            repo_root: repo_path.clone(),
            repo_hash: format!("bench_{}", repo_name),
        };
        cache.init().unwrap();

        let _ = index_directory(&repo_path, cache, &options);

        // Benchmark adding a single new file and reindexing
        let new_file_path = repo_path.join("src").join("benchmark_new_file.ts");

        group.bench_with_input(
            BenchmarkId::new(*repo_name, "add_file"),
            &(repo_path.clone(), new_file_path.clone()),
            |b, (path, new_file)| {
                b.iter(|| {
                    // Add new file
                    if let Some(parent) = new_file.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    fs::write(new_file, SAMPLE_TS_CONTENT).unwrap();

                    // Full reindex (simulating incremental)
                    let temp_cache = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_cache.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    let _ = index_directory(black_box(path), cache, &options);

                    // Clean up
                    let _ = fs::remove_file(new_file);
                });
            },
        );
    }

    group.finish();
}

fn bench_file_modification(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_modification");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(20));
    let options = IndexOptions::default();

    for repo_name in INCREMENTAL_REPOS {
        let Some((_temp_repo_dir, repo_path)) = create_temp_repo_copy(repo_name) else {
            continue;
        };

        // Find a TypeScript file to modify
        let ts_file = find_ts_file(&repo_path);
        let Some(ts_file) = ts_file else {
            continue;
        };

        let original_content = fs::read_to_string(&ts_file).unwrap_or_default();

        group.bench_with_input(
            BenchmarkId::new(*repo_name, "modify_file"),
            &(repo_path.clone(), ts_file.clone(), original_content.clone()),
            |b, (path, file, original)| {
                b.iter(|| {
                    // Modify file
                    let modified = format!("{}\n// Benchmark modification: {}", original, chrono::Utc::now());
                    fs::write(file, &modified).unwrap();

                    // Reindex
                    let temp_cache = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_cache.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    let _ = index_directory(black_box(path), cache, &options);

                    // Restore original
                    fs::write(file, original).unwrap();
                });
            },
        );
    }

    group.finish();
}

fn bench_multi_file_change(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_file_change");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    let options = IndexOptions::default();

    for repo_name in INCREMENTAL_REPOS {
        let Some((_temp_repo_dir, repo_path)) = create_temp_repo_copy(repo_name) else {
            continue;
        };

        // Find multiple TypeScript files
        let ts_files = find_multiple_ts_files(&repo_path, 5);
        if ts_files.is_empty() {
            continue;
        }

        let original_contents: Vec<_> = ts_files
            .iter()
            .map(|f| fs::read_to_string(f).unwrap_or_default())
            .collect();

        group.bench_with_input(
            BenchmarkId::new(*repo_name, "modify_5_files"),
            &(repo_path.clone(), ts_files.clone(), original_contents.clone()),
            |b, (path, files, originals)| {
                b.iter(|| {
                    // Modify all files
                    for (file, original) in files.iter().zip(originals.iter()) {
                        let modified = format!("{}\n// Multi-file benchmark: {}", original, chrono::Utc::now());
                        fs::write(file, &modified).unwrap();
                    }

                    // Reindex
                    let temp_cache = tempfile::tempdir().unwrap();
                    let cache = CacheDir {
                        root: temp_cache.path().to_path_buf(),
                        repo_root: path.clone(),
                        repo_hash: format!("bench_{}", repo_name),
                    };
                    cache.init().unwrap();

                    let _ = index_directory(black_box(path), cache, &options);

                    // Restore originals
                    for (file, original) in files.iter().zip(originals.iter()) {
                        fs::write(file, original).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Find a TypeScript file in the repo
fn find_ts_file(repo_path: &PathBuf) -> Option<PathBuf> {
    let walker = ignore::WalkBuilder::new(repo_path).build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ts" || ext == "tsx" {
                    return Some(path.to_path_buf());
                }
            }
        }
    }

    None
}

/// Find multiple TypeScript files
fn find_multiple_ts_files(repo_path: &PathBuf, count: usize) -> Vec<PathBuf> {
    let walker = ignore::WalkBuilder::new(repo_path).build();
    let mut files = Vec::new();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ts" || ext == "tsx" {
                    files.push(path.to_path_buf());
                    if files.len() >= count {
                        break;
                    }
                }
            }
        }
    }

    files
}

/// Benchmark tree-sitter incremental parsing vs full parsing
/// This directly measures the parsing performance gains from AST caching.
fn bench_incremental_parsing(c: &mut Criterion) {
    use semfora_engine::lang::Lang;
    use semfora_engine::server::AstCache;

    let mut group = c.benchmark_group("tree_sitter_parsing");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    // Sample TypeScript content for parsing benchmarks
    let source_original = r#"
import { z } from 'zod';

export interface User {
    id: string;
    name: string;
    email: string;
}

export const userSchema = z.object({
    id: z.string().uuid(),
    name: z.string().min(1),
    email: z.string().email(),
});

export function validateUser(data: unknown): User {
    return userSchema.parse(data);
}

export class UserService {
    private cache: Map<string, User> = new Map();

    async getUser(id: string): Promise<User | null> {
        const cached = this.cache.get(id);
        if (cached) return cached;
        return null;
    }

    setUser(id: string, user: User): void {
        this.cache.set(id, user);
    }
}
"#;

    // Simulate an edit: adding a new function
    let source_edited = r#"
import { z } from 'zod';

export interface User {
    id: string;
    name: string;
    email: string;
}

export const userSchema = z.object({
    id: z.string().uuid(),
    name: z.string().min(1),
    email: z.string().email(),
});

export function validateUser(data: unknown): User {
    return userSchema.parse(data);
}

export function formatUserName(user: User): string {
    return user.name.toUpperCase();
}

export class UserService {
    private cache: Map<string, User> = new Map();

    async getUser(id: string): Promise<User | null> {
        const cached = this.cache.get(id);
        if (cached) return cached;
        return null;
    }

    setUser(id: string, user: User): void {
        this.cache.set(id, user);
    }
}
"#;

    let path = PathBuf::from("/tmp/benchmark_test.ts");

    // Benchmark: Full parse (no cache)
    group.bench_function("full_parse", |b| {
        let cache = AstCache::new();
        b.iter(|| {
            // Clear cache to force full parse
            cache.clear();
            let _ = cache.parse_file(
                black_box(&path),
                black_box(source_original),
                Lang::TypeScript,
            );
        });
    });

    // Benchmark: Incremental parse (after initial cache)
    group.bench_function("incremental_parse", |b| {
        let cache = AstCache::new();
        // Initial full parse to populate cache
        let _ = cache.parse_file(&path, source_original, Lang::TypeScript);

        b.iter(|| {
            // Edit and incremental parse
            let _ = cache.parse_file(
                black_box(&path),
                black_box(source_edited),
                Lang::TypeScript,
            );
            // Reset for next iteration
            let _ = cache.parse_file(
                black_box(&path),
                black_box(source_original),
                Lang::TypeScript,
            );
        });
    });

    // Benchmark: Cache hit (unchanged file)
    group.bench_function("cache_hit", |b| {
        let cache = AstCache::new();
        // Initial full parse
        let _ = cache.parse_file(&path, source_original, Lang::TypeScript);

        b.iter(|| {
            // Same content - should be cache hit
            let _ = cache.parse_file(
                black_box(&path),
                black_box(source_original),
                Lang::TypeScript,
            );
        });
    });

    group.finish();
}

/// Benchmark AST cache with larger TypeScript files
fn bench_incremental_parsing_large(c: &mut Criterion) {
    use semfora_engine::lang::Lang;
    use semfora_engine::server::AstCache;

    let mut group = c.benchmark_group("tree_sitter_parsing_large");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(15));

    // Find a real TypeScript file from test repos if available
    let repos_dir = std::env::var("SEMFORA_TEST_REPOS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/kadajett/Dev/semfora-test-repos/repos"));

    let large_file = find_large_ts_file(&repos_dir);

    if let Some((file_path, content)) = large_file {
        let file_size = content.len();
        eprintln!("Using large file: {:?} ({} bytes)", file_path, file_size);

        // Create edited version (add a comment at the end)
        let edited_content = format!("{}\n// Benchmark edit: {}", content, chrono::Utc::now());
        let path = PathBuf::from("/tmp/benchmark_large.ts");

        group.bench_function(format!("full_parse_{}_bytes", file_size), |b| {
            let cache = AstCache::new();
            b.iter(|| {
                cache.clear();
                let _ = cache.parse_file(
                    black_box(&path),
                    black_box(&content),
                    Lang::TypeScript,
                );
            });
        });

        group.bench_function(format!("incremental_parse_{}_bytes", file_size), |b| {
            let cache = AstCache::new();
            let _ = cache.parse_file(&path, &content, Lang::TypeScript);

            b.iter(|| {
                let _ = cache.parse_file(
                    black_box(&path),
                    black_box(&edited_content),
                    Lang::TypeScript,
                );
                let _ = cache.parse_file(
                    black_box(&path),
                    black_box(&content),
                    Lang::TypeScript,
                );
            });
        });
    }

    group.finish();
}

/// Find a large TypeScript file for benchmarking
fn find_large_ts_file(repos_dir: &PathBuf) -> Option<(PathBuf, String)> {
    if !repos_dir.exists() {
        return None;
    }

    let walker = ignore::WalkBuilder::new(repos_dir)
        .max_depth(Some(5))
        .build();

    let mut best: Option<(PathBuf, String)> = None;
    let mut best_size = 0;

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ts" || ext == "tsx" {
                    if let Ok(content) = fs::read_to_string(path) {
                        let size = content.len();
                        // Look for files between 5KB and 50KB (good benchmark size)
                        if size > 5000 && size < 50000 && size > best_size {
                            best_size = size;
                            best = Some((path.to_path_buf(), content));
                        }
                    }
                }
            }
        }
    }

    best
}

criterion_group!(
    benches,
    bench_full_reindex,
    bench_single_file_add,
    bench_file_modification,
    bench_multi_file_change,
    bench_incremental_parsing,
    bench_incremental_parsing_large,
);
criterion_main!(benches);
