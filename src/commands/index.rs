//! Index command handler - Manage the semantic index

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::cache::CacheDir;
use crate::cli::{IndexArgs, IndexOperation, OutputFormat};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::indexing::{analyze_files_parallel, IndexingProgressCallback};
use crate::shard::{ShardProgressCallback, ShardWriter};
use crate::Lang;

struct ProgressState {
    last_line_len: usize,
    last_size_check: Instant,
    last_mem_check: Instant,
    last_size_bytes: u64,
    last_mem_bytes: Option<u64>,
    step_start: Instant,
    current_step: String,
    step_timings: std::collections::HashMap<String, f64>,
    total_start: Instant,
}

struct ProgressReporter {
    cache_root: PathBuf,
    state: Mutex<ProgressState>,
}

impl ProgressReporter {
    fn new(cache_root: PathBuf) -> Self {
        Self {
            cache_root,
            state: Mutex::new(ProgressState {
                last_line_len: 0,
                last_size_check: Instant::now()
                    .checked_sub(Duration::from_secs(5))
                    .unwrap_or_else(Instant::now),
                last_mem_check: Instant::now()
                    .checked_sub(Duration::from_secs(5))
                    .unwrap_or_else(Instant::now),
                last_size_bytes: 0,
                last_mem_bytes: None,
                step_start: Instant::now(),
                current_step: String::new(),
                step_timings: std::collections::HashMap::new(),
                total_start: Instant::now(),
            }),
        }
    }

    fn update(&self, step: &str, current: usize, total: usize) {
        let percent = if total == 0 {
            0.0
        } else {
            (current as f64 / total as f64) * 100.0
        };

        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        if state.current_step != step {
            let previous_step = state.current_step.clone();
            if !previous_step.is_empty() {
                let elapsed = now.duration_since(state.step_start).as_secs_f64();
                *state.step_timings.entry(previous_step).or_insert(0.0) += elapsed;
            }
            state.current_step = step.to_string();
            state.step_start = now;
        }

        if now.duration_since(state.last_mem_check) >= Duration::from_millis(500) {
            state.last_mem_bytes = read_rss_bytes();
            state.last_mem_check = now;
        }

        if now.duration_since(state.last_size_check) >= Duration::from_secs(1) {
            state.last_size_bytes = dir_size_bytes(&self.cache_root);
            state.last_size_check = now;
        }

        let mem_str = state
            .last_mem_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "n/a".to_string());
        let size_str = format_bytes(state.last_size_bytes);
        let step_elapsed = now.duration_since(state.step_start);
        let total_elapsed = now.duration_since(state.total_start);

        let mut rate_str = String::new();
        if step == "BM25 index" && current > 0 {
            let secs = step_elapsed.as_secs_f64();
            if secs > 0.0 {
                let rate = current as f64 / secs;
                if rate.is_finite() && rate > 0.0 {
                    let remaining = total.saturating_sub(current) as f64 / rate;
                    rate_str = format!(
                        " | Rate: {:.1}/s ETA: {}",
                        rate,
                        format_duration(Duration::from_secs_f64(remaining))
                    );
                }
            }
        }

        let line = format!(
            "Progress: {:5.1}% | Step: {} ({}/{}) | Step: {} | Total: {} | RSS: {} | Output: {}{}",
            percent,
            step,
            current,
            total,
            format_duration(step_elapsed),
            format_duration(total_elapsed),
            mem_str,
            size_str,
            rate_str
        );

        let pad_len = state.last_line_len.saturating_sub(line.len());
        if pad_len > 0 {
            eprint!("\r{}{}", line, " ".repeat(pad_len));
        } else {
            eprint!("\r{}", line);
        }
        let _ = std::io::stderr().flush();
        state.last_line_len = line.len();
    }

    fn finish(&self) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let previous_step = state.current_step.clone();
        if !previous_step.is_empty() {
            let elapsed = now.duration_since(state.step_start).as_secs_f64();
            *state.step_timings.entry(previous_step).or_insert(0.0) += elapsed;
        }
        let total_elapsed = now.duration_since(state.total_start).as_secs_f64();
        let timings_path = self.cache_root.join("timings.json");
        let _ = fs::write(
            timings_path,
            serde_json::json!({
                "steps": state.step_timings,
                "total_seconds": total_elapsed
            })
            .to_string(),
        );
        eprintln!();
    }
}

fn read_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let content = fs::read_to_string("/proc/self/status").ok()?;
        for line in content.lines() {
            if let Some(value) = line.strip_prefix("VmRSS:") {
                let kb = value
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())?;
                return Some(kb * 1024);
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn dir_size_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            let meta = match fs::symlink_metadata(&entry_path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry_path);
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }

    total
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut idx = 0usize;
    while size >= 1024.0 && idx < UNITS.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, UNITS[idx])
    } else {
        format!("{:.1} {}", size, UNITS[idx])
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}
/// Run the index command
pub fn run_index(args: &IndexArgs, ctx: &CommandContext) -> Result<String> {
    match &args.operation {
        IndexOperation::Generate {
            path,
            force,
            incremental,
            max_depth,
            extensions,
        } => run_generate(
            path.clone(),
            *force,
            *incremental,
            *max_depth,
            extensions.clone(),
            ctx,
        ),
        IndexOperation::Check {
            auto_refresh,
            max_age,
        } => run_check(*auto_refresh, *max_age, ctx),
        IndexOperation::Export { path } => run_export(path.clone(), ctx),
    }
}

/// Generate or refresh the semantic index
fn run_generate(
    path: Option<PathBuf>,
    force: bool,
    incremental: bool,
    max_depth: usize,
    extensions: Vec<String>,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir =
        path.unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let cache = CacheDir::for_repo(&repo_dir)?;

    // Check if we should skip (unless force)
    if !force && cache.exists() {
        // Check freshness
        let meta_path = cache.root.join("meta.json");
        if meta_path.exists() {
            if let Ok(meta_content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                    if let Some(indexed_at) = meta.get("indexed_at").and_then(|v| v.as_str()) {
                        if let Ok(indexed_time) = chrono::DateTime::parse_from_rfc3339(indexed_at) {
                            let age = chrono::Utc::now().signed_duration_since(indexed_time);
                            if age < chrono::Duration::hours(1) && !incremental {
                                return Ok(format!(
                                    "Index is fresh ({}). Use --force to regenerate.",
                                    indexed_at
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if incremental && cache.exists() {
        // For incremental, just check if anything changed
        // For now, we just regenerate if forced or no cache exists
        return Ok("Incremental mode: Index exists. Use --force to regenerate.".to_string());
    }

    run_full_index(&repo_dir, &cache, max_depth, &extensions, ctx, force)
}

/// Run full index generation
fn run_full_index(
    repo_dir: &std::path::Path,
    cache: &CacheDir,
    max_depth: usize,
    extensions: &[String],
    ctx: &CommandContext,
    force: bool,
) -> Result<String> {
    // Clear existing cache
    if cache.exists() {
        let progress_path = cache.root.join("progress.json");
        if force || !progress_path.exists() {
            cache.clear()?;
        }
    }

    let reporter = if ctx.progress {
        Some(Arc::new(ProgressReporter::new(cache.root.clone())))
    } else {
        None
    };

    if let Some(reporter) = &reporter {
        reporter.update("Collecting files", 0, 1);
    }

    // Collect files
    let files = collect_files(repo_dir, max_depth, extensions)?;

    if let Some(reporter) = &reporter {
        reporter.update("Collecting files", 1, 1);
    }

    if files.is_empty() {
        return Ok("No supported files found to index.".to_string());
    }

    if ctx.verbose && !ctx.progress {
        eprintln!("Indexing {} files...", files.len());
    }

    // Create shard writer (takes repo path)
    let mut writer = ShardWriter::new(repo_dir)?;

    // Process files in parallel (DEDUP-102: fixes the parallelism bug)
    // Previously used sequential for loop, now uses Rayon par_iter()
    let progress_cb: Option<IndexingProgressCallback> = if let Some(reporter) = &reporter {
        let reporter = Arc::clone(reporter);
        Some(Box::new(move |current: usize, total: usize| {
            reporter.update("Indexing files", current, total);
        }))
    } else {
        None
    };

    let result = analyze_files_parallel(&files, progress_cb, ctx.verbose);
    let summaries = result.summaries;
    let errors = result.errors;

    // Add all summaries and write
    writer.add_summaries(summaries.clone());
    let stats = if let Some(reporter) = &reporter {
        let reporter = Arc::clone(reporter);
        let progress: ShardProgressCallback = Arc::new(move |step, current, total| {
            reporter.update(step, current, total);
        });
        writer.write_all_with_progress(&repo_dir.display().to_string(), Some(progress))?
    } else {
        writer.write_all(&repo_dir.display().to_string())?
    };

    if let Some(reporter) = &reporter {
        reporter.finish();
    }

    let progress_path = cache.root.join("progress.json");
    let _ = fs::remove_file(progress_path);

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_generate",
        "action": "generate",
        "path": repo_dir.to_string_lossy(),
        "files_found": files.len(),
        "files_processed": summaries.len(),
        "errors": errors,
        "modules": stats.modules_written,
        "symbols": stats.symbols_written
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("Index generation complete:\n");
            output.push_str(&format!("  path: {}\n", repo_dir.display()));
            output.push_str(&format!("  files_found: {}\n", files.len()));
            output.push_str(&format!("  files_processed: {}\n", summaries.len()));
            output.push_str(&format!("  errors: {}\n", errors));
            output.push_str(&format!("  modules: {}\n", stats.modules_written));
            output.push_str(&format!("  symbols: {}\n", stats.symbols_written));
        }
    }

    Ok(output)
}

/// Check if the index is fresh or stale
fn run_check(auto_refresh: bool, max_age: u64, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        if auto_refresh {
            return run_full_index(&repo_dir, &cache, 10, &[], ctx, false);
        }
        return Ok("No index found. Run `semfora index generate` to create one.".to_string());
    }

    // Read metadata
    let meta_path = cache.root.join("meta.json");
    if !meta_path.exists() {
        if auto_refresh {
            return run_full_index(&repo_dir, &cache, 10, &[], ctx, false);
        }
        return Ok(
            "Index metadata not found. Run `semfora index generate` to regenerate.".to_string(),
        );
    }

    let meta_content = fs::read_to_string(&meta_path)?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_content).map_err(|e| McpDiffError::GitError {
            message: format!("Parse error: {}", e),
        })?;

    let indexed_at = meta
        .get("indexed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let is_stale = if let Ok(indexed_time) = chrono::DateTime::parse_from_rfc3339(indexed_at) {
        let age = chrono::Utc::now().signed_duration_since(indexed_time);
        age.num_seconds() as u64 > max_age
    } else {
        true
    };

    if is_stale && auto_refresh {
        eprintln!("Index is stale. Refreshing...");
        return run_full_index(&repo_dir, &cache, 10, &[], ctx, false);
    }

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_check",
        "path": repo_dir.to_string_lossy(),
        "indexed_at": indexed_at,
        "is_stale": is_stale,
        "max_age_seconds": max_age
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("path: {}\n", repo_dir.display()));
            output.push_str(&format!("indexed_at: {}\n", indexed_at));
            output.push_str(&format!(
                "status: {}\n",
                if is_stale { "STALE" } else { "FRESH" }
            ));
            if is_stale {
                output.push_str("\nRun `semfora index generate` to refresh.\n");
            }
        }
    }

    Ok(output)
}

/// Export the index to SQLite
fn run_export(path: Option<String>, ctx: &CommandContext) -> Result<String> {
    use crate::sqlite_export::{default_export_path, SqliteExporter};

    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    let output_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path(&cache));

    if ctx.verbose {
        eprintln!("Exporting to: {}", output_path.display());
    }

    let exporter = SqliteExporter::new();
    let stats = exporter.export(&cache, &output_path, None, false)?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_export",
        "path": output_path.to_string_lossy(),
        "nodes": stats.nodes_inserted,
        "edges": stats.edges_inserted,
        "file_size": stats.file_size_bytes
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("Export complete:\n");
            output.push_str(&format!("  path: {}\n", output_path.display()));
            output.push_str(&format!("  nodes: {}\n", stats.nodes_inserted));
            output.push_str(&format!("  edges: {}\n", stats.edges_inserted));
            output.push_str(&format!("  file_size: {} bytes\n", stats.file_size_bytes));
        }
    }

    Ok(output)
}

// ============================================
// Helper Functions
// ============================================

/// Collect files to index
fn collect_files(
    dir: &std::path::Path,
    max_depth: usize,
    extensions: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, max_depth, 0, extensions, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(
    _root: &std::path::Path,
    dir: &std::path::Path,
    max_depth: usize,
    current_depth: usize,
    extensions: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    if current_depth > max_depth {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| McpDiffError::FileNotFound {
        path: format!("{}: {}", dir.display(), e),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files and common non-source directories
        if file_name.starts_with('.')
            || file_name == "node_modules"
            || file_name == "target"
            || file_name == "dist"
            || file_name == "build"
        {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(
                _root,
                &path,
                max_depth,
                current_depth + 1,
                extensions,
                files,
            )?;
        } else if path.is_file() {
            // Check extension filter
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if !extensions.is_empty() && !extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
                {
                    continue;
                }

                // Check if it's a supported language
                if Lang::from_path(&path).is_ok() {
                    files.push(path);
                }
            }
        }
    }

    Ok(())
}
