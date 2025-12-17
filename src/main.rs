//! Semfora Engine CLI entry point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use semfora_engine::cli::{Commands, ConfigOperation, OperationMode, TokenAnalysisMode};
use semfora_engine::installer::{
    self, print_available_clients, ConfigArgs, SetupArgs, UninstallArgs,
};
use semfora_engine::git::{
    get_changed_files, get_commit_changed_files, get_commits_since, get_file_at_ref,
    get_merge_base, get_repo_root, is_git_repo, ChangedFile, ChangeType,
    get_staged_changes, get_unstaged_changes,
};
use semfora_engine::{
    encode_toon, encode_toon_directory, extract, format_analysis_compact, format_analysis_report,
    generate_repo_overview, Cli, Lang, McpDiffError, OutputFormat, SemanticSummary, TokenAnalyzer,
    CacheDir, ShardWriter, get_cache_base_dir, list_cached_repos, prune_old_caches,
    analyze_repo_tokens, is_test_file,
    // Drift detection for incremental indexing (SEM-47)
    count_tracked_files, DriftDetector, UpdateStrategy, LayerKind,
};
use semfora_engine::truncate_to_char_boundary;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{}", output);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            e.exit_code()
        }
    }
}

fn run() -> semfora_engine::Result<String> {
    let cli = Cli::parse();

    // Handle subcommands (setup, uninstall, config) first
    if let Some(ref cmd) = cli.command {
        return handle_subcommand(cmd);
    }

    // Handle cache commands first (they don't require operation mode)
    if cli.cache_info {
        return run_cache_info();
    }

    if cli.cache_clear {
        return run_cache_clear();
    }

    if let Some(days) = cli.cache_prune {
        return run_cache_prune(days);
    }

    // Handle benchmark mode
    if cli.benchmark {
        let dir_path = cli.file.as_ref()
            .filter(|p| p.is_dir())
            .or(cli.dir.as_ref())
            .map(|p| p.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        return run_benchmark(&dir_path);
    }

    // Handle shard query commands
    if cli.list_modules {
        return run_list_modules(&cli);
    }

    if let Some(ref module_name) = cli.get_module {
        return run_get_module(&cli, module_name);
    }

    if let Some(ref query) = cli.search_symbols {
        return run_search_symbols(&cli, query);
    }

    if let Some(ref module_name) = cli.list_symbols {
        return run_list_module_symbols(&cli, module_name);
    }

    if let Some(ref symbol_hash) = cli.get_symbol {
        return run_get_symbol(&cli, symbol_hash);
    }

    if cli.get_overview {
        return run_get_overview(&cli);
    }

    if cli.get_call_graph {
        return run_get_call_graph(&cli);
    }

    // Handle SQLite export
    if cli.export_sqlite.is_some() {
        return run_export_sqlite(&cli);
    }

    // Handle static analysis
    if cli.analyze {
        return run_static_analysis(&cli);
    }

    // Handle duplicate detection
    if cli.find_duplicates {
        return run_find_duplicates(&cli);
    }

    if cli.check_duplicates.is_some() {
        return run_check_duplicates(&cli);
    }

    // Handle new query commands
    if let Some(ref file_path) = cli.get_source {
        return run_get_source(&cli, file_path);
    }

    if let Some(ref pattern) = cli.raw_search {
        return run_raw_search(&cli, pattern);
    }

    if let Some(ref query) = cli.semantic_search {
        return run_semantic_search(&cli, query);
    }

    if let Some(ref file_path) = cli.file_symbols {
        return run_file_symbols(&cli, file_path);
    }

    if let Some(ref symbol_hash) = cli.get_callers {
        return run_get_callers(&cli, symbol_hash);
    }

    // Handle prep-commit mode
    if cli.prep_commit {
        return run_prep_commit(&cli);
    }

    let mode = cli.operation_mode()?;

    // Handle sharded output mode
    if cli.shard {
        return match mode {
            OperationMode::Directory { path, max_depth } => run_shard(&cli, &path, max_depth),
            OperationMode::SingleFile(path) if path.is_dir() => run_shard(&cli, &path, cli.max_depth),
            _ => Err(McpDiffError::GitError {
                message: "Shard mode requires a directory. Use --dir or provide a directory path.".to_string(),
            }),
        };
    }

    match mode {
        OperationMode::SingleFile(path) => run_single_file(&cli, &path),
        OperationMode::Directory { path, max_depth } => run_directory(&cli, &path, max_depth),
        OperationMode::DiffBranch { base_ref } => run_diff_branch(&cli, &base_ref),
        OperationMode::Uncommitted { base_ref } => run_uncommitted(&cli, &base_ref),
        OperationMode::SingleCommit { sha } => run_single_commit(&cli, &sha),
        OperationMode::AllCommits { base_ref } => run_all_commits(&cli, &base_ref),
    }
}

/// Handle installer subcommands (setup, uninstall, config)
fn handle_subcommand(cmd: &Commands) -> semfora_engine::Result<String> {
    match cmd {
        Commands::Setup(args) => {
            // Handle --list-clients flag
            if args.list_clients {
                print_available_clients();
                return Ok(String::new());
            }

            // Convert CLI args to installer args
            let setup_args = SetupArgs {
                non_interactive: args.non_interactive,
                clients: args.clients.clone(),
                export_config: args.export_config.clone(),
                binary_path: args.binary_path.clone(),
                cache_dir: args.cache_dir.clone(),
                log_level: args.log_level.clone(),
                dry_run: args.dry_run,
            };

            installer::run_setup(setup_args)?;
            Ok(String::new())
        }
        Commands::Uninstall(args) => {
            let uninstall_args = UninstallArgs {
                target: args.target.clone(),
                client: args.client.clone(),
                keep_cache: args.keep_cache,
                force: args.force,
            };

            installer::run_uninstall(uninstall_args)?;
            Ok(String::new())
        }
        Commands::Config(args) => {
            let config_args = match &args.operation {
                ConfigOperation::Show => ConfigArgs {
                    command: "show".to_string(),
                    key: None,
                    value: None,
                },
                ConfigOperation::Set { key, value } => ConfigArgs {
                    command: "set".to_string(),
                    key: Some(key.clone()),
                    value: Some(value.clone()),
                },
                ConfigOperation::Reset => ConfigArgs {
                    command: "reset".to_string(),
                    key: None,
                    value: None,
                },
            };

            installer::run_config(config_args)?;
            Ok(String::new())
        }
    }
}

/// Run single file analysis (Phase 1 mode)
fn run_single_file(cli: &Cli, file_path: &Path) -> semfora_engine::Result<String> {
    // 1. Check file exists
    if !file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: file_path.display().to_string(),
        });
    }

    // 2. Detect language from file extension
    let lang = Lang::from_path(file_path)?;

    if cli.verbose {
        eprintln!(
            "Detected language: {} ({})",
            lang.name(),
            lang.family().name()
        );
    }

    // 3. Read source file
    let source = fs::read_to_string(file_path)?;

    if cli.verbose {
        eprintln!("Read {} bytes from {}", source.len(), file_path.display());
    }

    // 4. Parse and extract
    let summary = parse_and_extract(file_path, &source, lang, cli)?;

    // 5. Encode output in all formats for token analysis
    let toon_output = encode_toon(&summary);
    let json_pretty =
        serde_json::to_string_pretty(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;
    let json_compact =
        serde_json::to_string(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;

    // 6. Run token analysis if requested
    if let Some(mode) = cli.analyze_tokens {
        let analyzer = TokenAnalyzer::new();
        let analysis = analyzer.analyze(&source, &json_pretty, &json_compact, &toon_output);

        let report = match mode {
            TokenAnalysisMode::Full => format_analysis_report(&analysis, cli.compare_compact),
            TokenAnalysisMode::Compact => format_analysis_compact(&analysis, cli.compare_compact),
        };
        eprintln!("{}", report);
    }

    // 7. Return output in requested format
    let output = match cli.format {
        OutputFormat::Toon => toon_output,
        OutputFormat::Json => json_pretty,
    };

    Ok(format!("{}\n", output))
}

/// Run directory analysis mode
fn run_directory(cli: &Cli, dir_path: &Path, max_depth: usize) -> semfora_engine::Result<String> {
    if !dir_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: dir_path.display().to_string(),
        });
    }

    if !dir_path.is_dir() {
        return Err(McpDiffError::GitError {
            message: format!("{} is not a directory", dir_path.display()),
        });
    }

    // Collect all supported files
    let files = collect_files(dir_path, max_depth, cli);

    if files.is_empty() {
        return Ok(format!("directory: {}\nfiles_found: 0\n", dir_path.display()));
    }

    if cli.verbose {
        eprintln!("Found {} files to analyze", files.len());
    }

    // First pass: collect all summaries (parallel with rayon)
    let all_source_len_atomic = AtomicUsize::new(0);
    let total_lines_atomic = AtomicUsize::new(0);
    let verbose = cli.verbose;
    let show_progress = cli.progress;

    let summaries: Vec<SemanticSummary> = files
        .par_iter()
        .filter_map(|file_path| {
            // Try to detect language
            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            // Read and analyze file
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    if verbose {
                        let relative_path = file_path
                            .strip_prefix(dir_path)
                            .unwrap_or(file_path)
                            .display()
                            .to_string();
                        eprintln!("Skipping {}: {}", relative_path, e);
                    }
                    return None;
                }
            };

            all_source_len_atomic.fetch_add(source.len(), Ordering::Relaxed);
            total_lines_atomic.fetch_add(source.lines().count(), Ordering::Relaxed);

            match parse_and_extract_string(file_path, &source, lang) {
                Ok(s) => Some(s),
                Err(e) => {
                    if verbose {
                        let relative_path = file_path
                            .strip_prefix(dir_path)
                            .unwrap_or(file_path)
                            .display()
                            .to_string();
                        eprintln!("Failed to analyze {}: {}", relative_path, e);
                    }
                    None
                }
            }
        })
        .collect();

    let all_source_len = all_source_len_atomic.load(Ordering::Relaxed);
    let total_lines = total_lines_atomic.load(Ordering::Relaxed);

    // Generate repository overview
    let dir_str = dir_path.display().to_string();
    let overview = generate_repo_overview(&summaries, &dir_str);

    // Generate output
    let output = if cli.summary_only {
        // Just the overview
        encode_toon_directory(&overview, &[])
    } else {
        // Full output with overview and all files
        encode_toon_directory(&overview, &summaries)
    };

    // Run token analysis if requested
    if let Some(mode) = cli.analyze_tokens {
        let toon_len = output.len();
        let compression_ratio = if all_source_len > 0 {
            ((all_source_len as f64 - toon_len as f64) / all_source_len as f64) * 100.0
        } else {
            0.0
        };

        let report = match mode {
            TokenAnalysisMode::Full => format!(
                "=== Token Analysis (Directory) ===\n\
                 source_chars: {}\n\
                 toon_chars: {}\n\
                 compression: {:.1}%\n\
                 files: {}\n\
                 lines: {}\n\
                 ================================",
                all_source_len, toon_len, compression_ratio, summaries.len(), total_lines
            ),
            TokenAnalysisMode::Compact => format!(
                "compression: {:.1}% ({} source → {} toon, {} files)",
                compression_ratio, all_source_len, toon_len, summaries.len()
            ),
        };
        eprintln!("{}", report);
    }

    Ok(output)
}

/// Run sharded indexing mode for large repositories
fn run_shard(cli: &Cli, dir_path: &Path, max_depth: usize) -> semfora_engine::Result<String> {
    if !dir_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: dir_path.display().to_string(),
        });
    }

    if !dir_path.is_dir() {
        return Err(McpDiffError::GitError {
            message: format!("{} is not a directory", dir_path.display()),
        });
    }

    eprintln!("Initializing sharded index for: {}", dir_path.display());

    // Create shard writer
    let mut shard_writer = ShardWriter::new(dir_path)?;
    let cache = CacheDir::for_repo(dir_path)?;

    eprintln!("Cache location: {}", shard_writer.cache_path().display());

    // Check for incremental mode (SEM-47 drift detection)
    let mut update_strategy = UpdateStrategy::FullRebuild;

    // Always get current SHA for git repos (needed for staleness tracking)
    let current_sha: Option<String> = if is_git_repo(Some(dir_path)) {
        semfora_engine::git::git_command(&["rev-parse", "HEAD"], Some(dir_path)).ok()
    } else {
        None
    };

    if cli.incremental {
        if let Some(ref sha) = current_sha {
            let indexed_sha = cache.get_indexed_sha();

            if let Some(ref idx_sha) = indexed_sha {
                if idx_sha == sha {
                    // Same SHA = fresh, no update needed
                    eprintln!("Index is fresh (SHA: {})", &sha[..8]);
                    return Ok(format!(
                        "═══════════════════════════════════════════════════════\n\
                         INDEX IS FRESH\n\
                         ═══════════════════════════════════════════════════════\n\n\
                         directory: {}\n\
                         indexed_sha: {}\n\
                         status: No changes detected, index is up to date.\n",
                        dir_path.display(), sha
                    ));
                }

                // Different SHA - use drift detection
                let file_count = count_tracked_files(dir_path).unwrap_or(0);
                let detector = DriftDetector::with_file_count(dir_path.to_path_buf(), file_count);
                let drift = detector.check_drift(LayerKind::Base, Some(idx_sha), None)?;
                update_strategy = drift.strategy(file_count);

                eprintln!(
                    "Incremental mode: {} ({} files changed, {:.1}% drift)",
                    update_strategy.description(),
                    drift.changed_files.len(),
                    drift.drift_percentage
                );
            } else {
                eprintln!("No previous index found, performing full rebuild");
            }
        }
    } else if cli.incremental {
        eprintln!("Warning: --incremental requires a git repository, performing full rebuild");
    }

    // Collect files to analyze based on update strategy
    let files_to_analyze: Vec<PathBuf> = match &update_strategy {
        UpdateStrategy::Fresh => {
            // Should not reach here (returned early above)
            Vec::new()
        }
        UpdateStrategy::Incremental(changed_files) => {
            // Only analyze changed files
            eprintln!("Incremental update: analyzing {} changed files", changed_files.len());
            changed_files
                .iter()
                .map(|p| dir_path.join(p))
                .filter(|p| p.exists())
                .collect()
        }
        UpdateStrategy::Rebase | UpdateStrategy::FullRebuild => {
            // Analyze all files
            collect_files(dir_path, max_depth, cli)
        }
    };

    if files_to_analyze.is_empty() && !matches!(update_strategy, UpdateStrategy::Fresh) {
        return Ok(format!(
            "directory: {}\nfiles_found: 0\nNo files to shard.\n",
            dir_path.display()
        ));
    }

    let total = files_to_analyze.len();
    eprintln!("Found {} files to analyze", total);

    // First pass: collect all summaries (parallel with rayon)
    let processed = AtomicUsize::new(0);
    let total_source_bytes_atomic = AtomicUsize::new(0);
    let verbose = cli.verbose;
    let show_progress = cli.progress;

    let summaries: Vec<SemanticSummary> = files_to_analyze
        .par_iter()
        .filter_map(|file_path| {
            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if show_progress && (current % 500 == 0 || current == total) {
                eprintln!("Processing: {}/{} ({:.1}%)", current, total, (current as f64 / total as f64) * 100.0);
            }

            // Try to detect language
            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            // Read and analyze file
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    if verbose {
                        eprintln!("Skipping {}: {}", file_path.display(), e);
                    }
                    return None;
                }
            };

            total_source_bytes_atomic.fetch_add(source.len(), Ordering::Relaxed);

            match parse_and_extract_string(file_path, &source, lang) {
                Ok(s) => Some(s),
                Err(e) => {
                    if verbose {
                        eprintln!("Failed to analyze {}: {}", file_path.display(), e);
                    }
                    None
                }
            }
        })
        .collect();

    let total_source_bytes = total_source_bytes_atomic.load(Ordering::Relaxed);
    eprintln!("Analyzed {} files ({} bytes source)", summaries.len(), total_source_bytes);

    // Add summaries to shard writer
    shard_writer.add_summaries(summaries);

    // Write all shards
    let dir_str = dir_path.display().to_string();
    let stats = shard_writer.write_all(&dir_str)?;

    // Save the indexed SHA for future incremental updates
    if let Some(ref sha) = current_sha {
        cache.set_indexed_sha(sha)?;
        eprintln!("Saved indexed SHA: {}", sha);
    }

    // Save status hash so we don't re-index the same uncommitted changes
    if let Some(status_hash) = cache.compute_status_hash() {
        let _ = cache.set_status_hash(&status_hash);
    }

    // Format output
    let (cache_size, _module_count) = shard_writer.cache_stats();
    let compression = if total_source_bytes > 0 {
        ((total_source_bytes as f64 - stats.total_bytes() as f64) / total_source_bytes as f64) * 100.0
    } else {
        0.0
    };

    let mut output = String::new();
    let header = match update_strategy {
        UpdateStrategy::Incremental(_) => "INCREMENTAL INDEX UPDATE",
        UpdateStrategy::Rebase => "INDEX REBASED",
        _ => "SHARDED INDEX CREATED",
    };
    output.push_str(&format!("═══════════════════════════════════════════════════════\n"));
    output.push_str(&format!("  {}\n", header));
    output.push_str(&format!("═══════════════════════════════════════════════════════\n\n"));

    output.push_str(&format!("directory: {}\n", dir_path.display()));
    output.push_str(&format!("cache: {}\n", shard_writer.cache_path().display()));
    if let Some(ref sha) = current_sha {
        output.push_str(&format!("indexed_sha: {}\n", sha));
    }
    output.push_str(&format!("strategy: {}\n", update_strategy.description()));
    output.push_str(&format!("\n"));

    output.push_str(&format!("Files:\n"));
    output.push_str(&format!("  files_written: {}\n", stats.files_written));
    output.push_str(&format!("  modules: {}\n", stats.modules_written));
    output.push_str(&format!("  symbols: {}\n", stats.symbols_written));
    output.push_str(&format!("\n"));

    output.push_str(&format!("Size:\n"));
    output.push_str(&format!("  source_bytes: {}\n", total_source_bytes));
    output.push_str(&format!("  shard_bytes: {}\n", stats.total_bytes()));
    output.push_str(&format!("  compression: {:.1}%\n", compression));
    output.push_str(&format!("  cache_total: {} bytes\n", cache_size));
    output.push_str(&format!("\n"));

    output.push_str(&format!("Breakdown:\n"));
    output.push_str(&format!("  overview: {} bytes\n", stats.overview_bytes));
    output.push_str(&format!("  modules: {} bytes\n", stats.module_bytes));
    output.push_str(&format!("  symbols: {} bytes\n", stats.symbol_bytes));
    output.push_str(&format!("  graphs: {} bytes\n", stats.graph_bytes));

    Ok(output)
}

/// Show cache information
fn run_cache_info() -> semfora_engine::Result<String> {
    let base_dir = get_cache_base_dir();
    let cached_repos = list_cached_repos();

    let mut output = String::new();
    output.push_str(&format!("═══════════════════════════════════════════════════════\n"));
    output.push_str(&format!("  SEMFORA CACHE INFO\n"));
    output.push_str(&format!("═══════════════════════════════════════════════════════\n\n"));

    output.push_str(&format!("cache_base: {}\n", base_dir.display()));
    output.push_str(&format!("cached_repos: {}\n\n", cached_repos.len()));

    if cached_repos.is_empty() {
        output.push_str("No cached repositories found.\n");
    } else {
        let total_size: u64 = cached_repos.iter().map(|(_, _, s)| *s).sum();
        output.push_str(&format!("total_size: {} bytes ({:.2} MB)\n\n", total_size, total_size as f64 / (1024.0 * 1024.0)));

        output.push_str(&format!("repos:\n"));
        for (hash, path, size) in &cached_repos {
            output.push_str(&format!("  - hash: {}\n", hash));
            output.push_str(&format!("    path: {}\n", path.display()));
            output.push_str(&format!("    size: {} bytes ({:.2} MB)\n", size, *size as f64 / (1024.0 * 1024.0)));
        }
    }

    Ok(output)
}

/// Clear the cache for the current directory
fn run_cache_clear() -> semfora_engine::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

    let mut output = String::new();

    if cache.exists() {
        let size = cache.size();
        cache.clear()?;
        output.push_str(&format!("Cache cleared for: {}\n", current_dir.display()));
        output.push_str(&format!("Freed: {} bytes ({:.2} MB)\n", size, size as f64 / (1024.0 * 1024.0)));
    } else {
        output.push_str(&format!("No cache exists for: {}\n", current_dir.display()));
    }

    Ok(output)
}

/// Prune caches older than specified days
fn run_cache_prune(days: u32) -> semfora_engine::Result<String> {
    let count = prune_old_caches(days)?;

    let mut output = String::new();
    output.push_str(&format!("Pruned {} cache(s) older than {} days.\n", count, days));

    Ok(output)
}

/// Run token efficiency benchmark
fn run_benchmark(dir_path: &Path) -> semfora_engine::Result<String> {
    let metrics = analyze_repo_tokens(dir_path)?;
    Ok(metrics.report())
}

// ============================================================================
// Shard Query Commands
// ============================================================================

/// Get the repository directory - uses --dir if provided, falls back to CWD.
/// This is critical for CLI tools that spawn semfora-engine from a different directory
/// than the target repository (e.g., Tauri apps running from their own cwd).
fn get_repo_dir(cli: &Cli) -> semfora_engine::Result<PathBuf> {
    match &cli.dir {
        Some(dir) => Ok(dir.clone()),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        }),
    }
}

/// List all modules in the cached index
fn run_list_modules(cli: &Cli) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No index found. Run with --shard first to generate index."),
        });
    }

    let modules = cache.list_modules();

    let mut output = String::new();

    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "modules": modules,
                "count": modules.len()
            });
            output = serde_json::to_string_pretty(&json).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&format!("modules[{}]:\n", modules.len()));
            for module in &modules {
                output.push_str(&format!("  {}\n", module));
            }
        }
    }

    Ok(output)
}

/// Get a specific module's content from the cache
fn run_get_module(cli: &Cli, module_name: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No index found. Run with --shard first to generate index."),
        });
    }

    let module_path = cache.module_path(module_name);

    if !module_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Module '{}' not found in index", module_name),
        });
    }

    let content = fs::read_to_string(&module_path)?;
    Ok(content)
}

/// Search for symbols by name in the cached index (with ripgrep fallback)
fn run_search_symbols(cli: &Cli, query: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Use fallback-aware search that automatically uses ripgrep when no index exists
    let search_result = cache.search_symbols_with_fallback(
        query,
        None, // module filter - could add later
        cli.kind.as_deref(),
        cli.risk.as_deref(),
        cli.limit,
    )?;

    let mut output = String::new();

    if search_result.fallback_used {
        // Ripgrep fallback results
        let ripgrep_results = search_result.ripgrep_results.unwrap_or_default();

        match cli.format {
            OutputFormat::Json => {
                let json = serde_json::json!({
                    "query": query,
                    "results": ripgrep_results,
                    "count": ripgrep_results.len(),
                    "fallback": true,
                    "note": "Using ripgrep fallback (no semantic index). Run with --shard to generate index."
                });
                output = serde_json::to_string_pretty(&json).unwrap_or_default();
            }
            OutputFormat::Toon => {
                output.push_str("_note: Using ripgrep fallback (no semantic index)\n");
                output.push_str(&format!("query: \"{}\"\n", query));
                output.push_str(&format!("results[{}]:\n", ripgrep_results.len()));
                for entry in &ripgrep_results {
                    let content_preview = if entry.content.len() > 60 {
                        format!("{}...", truncate_to_char_boundary(&entry.content, 60))
                    } else {
                        entry.content.clone()
                    };
                    output.push_str(&format!(
                        "  {}:{}:{}: {}\n",
                        entry.file, entry.line, entry.column, content_preview.trim()
                    ));
                }
            }
        }
    } else {
        // Normal indexed search results
        let results = search_result.indexed_results.unwrap_or_default();

        match cli.format {
            OutputFormat::Json => {
                let json = serde_json::json!({
                    "query": query,
                    "results": results,
                    "count": results.len()
                });
                output = serde_json::to_string_pretty(&json).unwrap_or_default();
            }
            OutputFormat::Toon => {
                output.push_str(&format!("query: \"{}\"\n", query));
                output.push_str(&format!("results[{}]:\n", results.len()));
                for entry in &results {
                    output.push_str(&format!(
                        "  {} ({}) - {} [{}] {}:{}\n",
                        entry.symbol, entry.kind, entry.module, entry.risk, entry.file, entry.lines
                    ));
                }
            }
        }
    }

    Ok(output)
}

/// List all symbols in a module from the cached index
fn run_list_module_symbols(cli: &Cli, module_name: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.has_symbol_index() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No symbol index found. Run with --shard first to generate index."),
        });
    }

    let results = cache.list_module_symbols(
        module_name,
        cli.kind.as_deref(),
        cli.risk.as_deref(),
        cli.limit,
    )?;

    let mut output = String::new();

    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "module": module_name,
                "symbols": results,
                "count": results.len()
            });
            output = serde_json::to_string_pretty(&json).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&format!("module: \"{}\"\n", module_name));
            output.push_str(&format!("symbols[{}]:\n", results.len()));
            for entry in &results {
                output.push_str(&format!(
                    "  {} ({}) [{}] - {}:{}\n",
                    entry.symbol, entry.kind, entry.risk, entry.file, entry.lines
                ));
            }
        }
    }

    Ok(output)
}

/// Get a specific symbol's details by hash
fn run_get_symbol(cli: &Cli, symbol_hash: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No index found. Run with --shard first to generate index."),
        });
    }

    let symbol_path = cache.symbol_path(symbol_hash);

    if !symbol_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Symbol '{}' not found in index", symbol_hash),
        });
    }

    let content = fs::read_to_string(&symbol_path)?;
    Ok(content)
}

/// Get the repository overview from the cache
fn run_get_overview(cli: &Cli) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No index found. Run with --shard first to generate index."),
        });
    }

    let overview_path = cache.repo_overview_path();

    if !overview_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Repository overview not found in index"),
        });
    }

    let content = fs::read_to_string(&overview_path)?;
    Ok(content)
}

/// Get the call graph from the cache
fn run_get_call_graph(cli: &Cli) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No index found. Run with --shard first to generate index."),
        });
    }

    let call_graph_path = cache.call_graph_path();

    if !call_graph_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Call graph not found in index. The index may need to be regenerated."),
        });
    }

    // Load the call graph (parses TOON format into HashMap)
    let graph = cache.load_call_graph()?;

    // Output as JSON for easy parsing
    let json = serde_json::to_string(&graph).map_err(|e| McpDiffError::FileNotFound {
        path: format!("Failed to serialize call graph: {}", e),
    })?;

    Ok(json)
}

/// Export call graph to SQLite database
fn run_export_sqlite(cli: &Cli) -> semfora_engine::Result<String> {
    use semfora_engine::{default_export_path, ExportProgress, SqliteExporter};
    use std::io::{stderr, Write};

    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "No index found. Run with --shard first to generate index.".to_string(),
        });
    }

    // Determine output path
    let output_arg = cli.export_sqlite.as_deref().unwrap_or("");
    let output_path = if output_arg.is_empty() {
        default_export_path(&cache)
    } else {
        let path = std::path::Path::new(output_arg);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            repo_dir.join(output_arg)
        }
    };

    eprintln!("Exporting call graph to SQLite: {}", output_path.display());

    // Progress callback for stderr
    let progress_callback: Option<Box<dyn Fn(ExportProgress) + Send>> =
        Some(Box::new(|progress| {
            eprint!(
                "\r  {:?}: {}/{} - {}          ",
                progress.phase, progress.current, progress.total, progress.message
            );
            let _ = stderr().flush();
        }));

    let exporter = SqliteExporter::new().with_batch_size(5000);
    let stats = exporter.export(&cache, &output_path, progress_callback)?;

    eprintln!("\n");
    eprintln!("Export complete:");
    eprintln!("  Output: {}", stats.output_path);
    eprintln!(
        "  Size: {:.2} MB",
        stats.file_size_bytes as f64 / 1024.0 / 1024.0
    );
    eprintln!("  Nodes: {}", stats.nodes_inserted);
    eprintln!("  Edges: {}", stats.edges_inserted);
    eprintln!("  Module edges: {}", stats.module_edges_inserted);
    eprintln!("  Duration: {}ms", stats.duration_ms);

    // Return minimal output to stdout
    Ok(format!(
        "Exported to: {}\nNodes: {}, Edges: {}, Size: {:.2} MB",
        stats.output_path,
        stats.nodes_inserted,
        stats.edges_inserted,
        stats.file_size_bytes as f64 / 1024.0 / 1024.0
    ))
}

/// Recursively collect supported files from a directory
fn collect_files(dir: &Path, max_depth: usize, cli: &Cli) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, max_depth, 0, cli, &mut files);
    files
}

fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    cli: &Cli,
    files: &mut Vec<PathBuf>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories and common non-source directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
                || name == ".next"
                || name == "coverage"
                || name == "__pycache__"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, current_depth + 1, cli, files);
        } else if path.is_file() {
            // Check if it's a supported extension
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Check extension filter
                if !cli.should_process_extension(ext) {
                    continue;
                }

                // Check if language is supported
                if Lang::from_extension(ext).is_ok() {
                    // Skip test files unless --allow-tests is set
                    if !cli.allow_tests {
                        if let Some(path_str) = path.to_str() {
                            if is_test_file(path_str) {
                                continue;
                            }
                        }
                    }
                    files.push(path);
                }
            }
        }
    }
}

/// Run diff branch analysis (Phase 2 mode)
fn run_diff_branch(cli: &Cli, base_ref: &str) -> semfora_engine::Result<String> {
    // Verify we're in a git repo
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root = get_repo_root(None)?;

    // Find the merge base for accurate diff
    let merge_base = get_merge_base(base_ref, "HEAD", None).unwrap_or_else(|_| base_ref.to_string());

    if cli.verbose {
        eprintln!("Base ref: {} (merge-base: {})", base_ref, merge_base);
    }

    // Get changed files
    let changed_files = get_changed_files(&merge_base, "HEAD", None)?;

    if changed_files.is_empty() {
        return Ok("No files changed.\n".to_string());
    }

    if cli.verbose {
        eprintln!("Found {} changed files", changed_files.len());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  DIFF: {} → HEAD ({} files)\n",
        base_ref,
        changed_files.len()
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    let mut total_stats = DiffStats::default();

    for changed_file in &changed_files {
        // Check extension filter
        if let Some(ext) = Path::new(&changed_file.path).extension() {
            if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                continue;
            }
        }

        let file_output = process_changed_file(cli, &repo_root, changed_file, &merge_base)?;
        if let Some((summary_output, stats)) = file_output {
            output.push_str(&summary_output);
            total_stats.merge(&stats);
        }
    }

    // Summary statistics
    if !cli.summary_only {
        output.push_str(&format!(
            "\n═══════════════════════════════════════════════════════\n"
        ));
        output.push_str(&format!("  SUMMARY\n"));
        output.push_str(&format!(
            "═══════════════════════════════════════════════════════\n"
        ));
    }
    output.push_str(&format!(
        "files: {} added, {} modified, {} deleted\n",
        total_stats.added, total_stats.modified, total_stats.deleted
    ));
    output.push_str(&format!(
        "risk: {} high, {} medium, {} low\n",
        total_stats.high_risk, total_stats.medium_risk, total_stats.low_risk
    ));

    Ok(output)
}

/// Run uncommitted changes analysis (working directory vs base_ref)
fn run_uncommitted(cli: &Cli, base_ref: &str) -> semfora_engine::Result<String> {
    // Verify we're in a git repo
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root_str = get_repo_root(None)?;
    let repo_root = Path::new(&repo_root_str);

    if cli.verbose {
        eprintln!("Analyzing uncommitted changes against: {}", base_ref);
    }

    // Get uncommitted changes (working directory vs base_ref)
    let changed_files = semfora_engine::git::get_uncommitted_changes(base_ref, None)?;

    if changed_files.is_empty() {
        return Ok("No uncommitted changes.\n".to_string());
    }

    if cli.verbose {
        eprintln!("Found {} uncommitted files", changed_files.len());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  UNCOMMITTED: {} → WORKING ({} files)\n",
        base_ref,
        changed_files.len()
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    let mut total_stats = DiffStats::default();

    for changed_file in &changed_files {
        // Check extension filter
        if let Some(ext) = Path::new(&changed_file.path).extension() {
            if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                continue;
            }
        }

        // For uncommitted changes, we analyze the current working copy
        let file_output = process_uncommitted_file(cli, &repo_root, changed_file, base_ref)?;
        if let Some((summary_output, stats)) = file_output {
            output.push_str(&summary_output);
            total_stats.merge(&stats);
        }
    }

    // Summary statistics
    if !cli.summary_only {
        output.push_str(&format!(
            "\n═══════════════════════════════════════════════════════\n"
        ));
        output.push_str(&format!("  SUMMARY\n"));
        output.push_str(&format!(
            "═══════════════════════════════════════════════════════\n"
        ));
    }
    output.push_str(&format!(
        "files: {} added, {} modified, {} deleted\n",
        total_stats.added, total_stats.modified, total_stats.deleted
    ));
    output.push_str(&format!(
        "risk: {} high, {} medium, {} low\n",
        total_stats.high_risk, total_stats.medium_risk, total_stats.low_risk
    ));

    Ok(output)
}

/// Process an uncommitted file change
fn process_uncommitted_file(
    cli: &Cli,
    repo_root: &Path,
    changed_file: &semfora_engine::git::ChangedFile,
    _base_ref: &str,
) -> semfora_engine::Result<Option<(String, DiffStats)>> {
    let file_path = repo_root.join(&changed_file.path);
    let mut stats = DiffStats::default();

    // Update stats based on change type
    match changed_file.change_type {
        semfora_engine::git::ChangeType::Added => stats.added += 1,
        semfora_engine::git::ChangeType::Modified => stats.modified += 1,
        semfora_engine::git::ChangeType::Deleted => stats.deleted += 1,
        _ => stats.modified += 1,
    }

    // Skip deleted files (can't analyze what doesn't exist)
    if changed_file.change_type == semfora_engine::git::ChangeType::Deleted {
        if cli.summary_only {
            return Ok(Some((String::new(), stats)));
        }
        return Ok(Some((
            format!("[-] {} (deleted)\n", changed_file.path),
            stats,
        )));
    }

    // Check if file exists in working directory
    if !file_path.exists() {
        return Ok(None);
    }

    // Try to detect language
    let lang = match Lang::from_path(&file_path) {
        Ok(l) => l,
        Err(_) => return Ok(None), // Skip unsupported files
    };

    // Read and analyze current working copy
    let source = match fs::read_to_string(&file_path) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    let summary = parse_and_extract(&file_path, &source, lang, cli)?;

    // Determine risk level (behavioral_risk is a RiskLevel enum)
    let risk_str = summary.behavioral_risk.as_str();
    match risk_str {
        "high" => stats.high_risk += 1,
        "medium" => stats.medium_risk += 1,
        _ => stats.low_risk += 1,
    }

    if cli.summary_only {
        return Ok(Some((String::new(), stats)));
    }

    // Format output
    let change_marker = match changed_file.change_type {
        semfora_engine::git::ChangeType::Added => "[+]",
        semfora_engine::git::ChangeType::Modified => "[M]",
        semfora_engine::git::ChangeType::Deleted => "[-]",
        _ => "[?]",
    };

    let output = match cli.format {
        OutputFormat::Toon => {
            format!(
                "{} {} ({})\n{}\n\n",
                change_marker,
                changed_file.path,
                risk_str,
                encode_toon(&summary)
            )
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&summary).unwrap_or_default();
            format!(
                "{} {} ({})\n{}\n\n",
                change_marker, changed_file.path, risk_str, json
            )
        }
    };

    Ok(Some((output, stats)))
}

/// Run single commit analysis
fn run_single_commit(cli: &Cli, sha: &str) -> semfora_engine::Result<String> {
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root = get_repo_root(None)?;
    let changed_files = get_commit_changed_files(sha, None)?;

    if changed_files.is_empty() {
        return Ok(format!("No files changed in commit {}.\n", sha));
    }

    let parent = semfora_engine::git::get_parent_commit(sha, None)?;

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  COMMIT: {} ({} files)\n",
        &sha[..7.min(sha.len())],
        changed_files.len()
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    for changed_file in &changed_files {
        if let Some(ext) = Path::new(&changed_file.path).extension() {
            if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                continue;
            }
        }

        let file_output = process_changed_file(cli, &repo_root, changed_file, &parent)?;
        if let Some((summary_output, _)) = file_output {
            output.push_str(&summary_output);
        }
    }

    Ok(output)
}

/// Run all commits analysis since base
fn run_all_commits(cli: &Cli, base_ref: &str) -> semfora_engine::Result<String> {
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let merge_base = get_merge_base(base_ref, "HEAD", None).unwrap_or_else(|_| base_ref.to_string());
    let commits = get_commits_since(&merge_base, None)?;

    if commits.is_empty() {
        return Ok(format!("No commits since {}.\n", base_ref));
    }

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  {} COMMITS since {}\n",
        commits.len(),
        base_ref
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    for commit in commits {
        output.push_str(&format!(
            "───────────────────────────────────────────────────────\n"
        ));
        output.push_str(&format!(
            "{} {}\n",
            commit.short_sha, commit.subject
        ));
        output.push_str(&format!("by {} on {}\n", commit.author, commit.date));
        output.push_str(&format!(
            "───────────────────────────────────────────────────────\n"
        ));

        // Get files changed in this commit
        let changed_files = get_commit_changed_files(&commit.sha, None)?;
        let parent = semfora_engine::git::get_parent_commit(&commit.sha, None)
            .unwrap_or_else(|_| commit.sha.clone());
        let repo_root = get_repo_root(None)?;

        for changed_file in &changed_files {
            if let Some(ext) = Path::new(&changed_file.path).extension() {
                if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                    continue;
                }
            }

            if let Some((summary_output, _)) =
                process_changed_file(cli, &repo_root, changed_file, &parent)?
            {
                output.push_str(&summary_output);
            }
        }
        output.push('\n');
    }

    Ok(output)
}

/// Process a single changed file and return its summary
fn process_changed_file(
    cli: &Cli,
    repo_root: &str,
    changed_file: &ChangedFile,
    base_ref: &str,
) -> semfora_engine::Result<Option<(String, DiffStats)>> {
    let full_path = PathBuf::from(repo_root).join(&changed_file.path);
    let mut stats = DiffStats::default();

    // Update stats
    match changed_file.change_type {
        ChangeType::Added => stats.added += 1,
        ChangeType::Modified => stats.modified += 1,
        ChangeType::Deleted => stats.deleted += 1,
        ChangeType::Renamed => stats.modified += 1,
        _ => {}
    }

    let mut output = String::new();

    // Header with change type
    output.push_str(&format!(
        "━━━ {} [{}] ━━━\n",
        changed_file.path,
        changed_file.change_type.as_str()
    ));

    // Skip deleted files (can't analyze)
    if changed_file.change_type == ChangeType::Deleted {
        output.push_str("(file deleted)\n\n");
        return Ok(Some((output, stats)));
    }

    // Try to detect language
    let lang = match Lang::from_path(&full_path) {
        Ok(l) => l,
        Err(_) => {
            if cli.verbose {
                output.push_str("(unsupported language, skipping)\n\n");
            }
            return Ok(Some((output, stats)));
        }
    };

    // Read current file content
    let current_source = match fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            output.push_str(&format!("(could not read: {})\n\n", e));
            return Ok(Some((output, stats)));
        }
    };

    // Parse and extract current version
    let current_summary = match parse_and_extract(&full_path, &current_source, lang, cli) {
        Ok(s) => s,
        Err(e) => {
            output.push_str(&format!("(extraction failed: {})\n\n", e));
            return Ok(Some((output, stats)));
        }
    };

    // Update risk stats
    match current_summary.behavioral_risk {
        semfora_engine::RiskLevel::High => stats.high_risk += 1,
        semfora_engine::RiskLevel::Medium => stats.medium_risk += 1,
        semfora_engine::RiskLevel::Low => stats.low_risk += 1,
    }

    // For added files, just show the current state
    if changed_file.change_type == ChangeType::Added {
        let toon = encode_toon(&current_summary);
        output.push_str(&toon);
        output.push_str("\n\n");
        return Ok(Some((output, stats)));
    }

    // For modified files, try to get the base version for comparison
    let base_source = get_file_at_ref(&changed_file.path, base_ref, None)?;

    if let Some(base_src) = base_source {
        // Parse base version
        if let Ok(base_summary) = parse_and_extract_string(&full_path, &base_src, lang) {
            // Show diff summary
            output.push_str(&format_diff_summary(&base_summary, &current_summary));
        } else {
            // Just show current if base fails to parse
            let toon = encode_toon(&current_summary);
            output.push_str(&toon);
        }
    } else {
        // No base version, show current
        let toon = encode_toon(&current_summary);
        output.push_str(&toon);
    }

    output.push_str("\n\n");
    Ok(Some((output, stats)))
}

/// Parse source and extract semantic summary
fn parse_and_extract(
    file_path: &Path,
    source: &str,
    lang: Lang,
    cli: &Cli,
) -> semfora_engine::Result<SemanticSummary> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language: {:?}", e),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: "Failed to parse file".to_string(),
        })?;

    if cli.verbose {
        eprintln!("Parsed AST with {} nodes", count_nodes(&tree.root_node()));
    }

    if cli.print_ast {
        eprintln!("\n=== AST ===");
        print_ast(&tree.root_node(), source, 0);
        eprintln!("=== END AST ===\n");
    }

    extract(file_path, source, &tree, lang)
}

/// Parse and extract without CLI context (for base version comparison)
fn parse_and_extract_string(
    file_path: &Path,
    source: &str,
    lang: Lang,
) -> semfora_engine::Result<SemanticSummary> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language: {:?}", e),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: "Failed to parse file".to_string(),
        })?;

    extract(file_path, source, &tree, lang)
}

/// Format a diff summary comparing base and current versions
fn format_diff_summary(base: &SemanticSummary, current: &SemanticSummary) -> String {
    let mut output = String::new();

    // Basic info
    output.push_str(&format!("symbol: {}\n", current.symbol.as_deref().unwrap_or("_")));
    output.push_str(&format!(
        "symbol_kind: {}\n",
        current
            .symbol_kind
            .map(|k| k.as_str())
            .unwrap_or("_")
    ));

    // Check for symbol changes
    if base.symbol != current.symbol {
        output.push_str(&format!(
            "CHANGED: symbol {} → {}\n",
            base.symbol.as_deref().unwrap_or("_"),
            current.symbol.as_deref().unwrap_or("_")
        ));
    }

    // Dependencies diff
    let added_deps: Vec<_> = current
        .added_dependencies
        .iter()
        .filter(|d| !base.added_dependencies.contains(d))
        .collect();
    let removed_deps: Vec<_> = base
        .added_dependencies
        .iter()
        .filter(|d| !current.added_dependencies.contains(d))
        .collect();

    if !added_deps.is_empty() {
        output.push_str(&format!("deps_added[{}]: {}\n", added_deps.len(), added_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }
    if !removed_deps.is_empty() {
        output.push_str(&format!("deps_removed[{}]: {}\n", removed_deps.len(), removed_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }

    // State changes diff
    let base_state_names: Vec<_> = base.state_changes.iter().map(|s| &s.name).collect();
    let current_state_names: Vec<_> = current.state_changes.iter().map(|s| &s.name).collect();

    let added_states: Vec<_> = current_state_names
        .iter()
        .filter(|n| !base_state_names.contains(n))
        .collect();
    let removed_states: Vec<_> = base_state_names
        .iter()
        .filter(|n| !current_state_names.contains(n))
        .collect();

    if !added_states.is_empty() {
        output.push_str(&format!("state_added[{}]: {}\n", added_states.len(), added_states.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }
    if !removed_states.is_empty() {
        output.push_str(&format!("state_removed[{}]: {}\n", removed_states.len(), removed_states.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }

    // Control flow diff
    let cf_base = base.control_flow_changes.len();
    let cf_current = current.control_flow_changes.len();
    if cf_base != cf_current {
        output.push_str(&format!(
            "control_flow: {} → {} ({:+})\n",
            cf_base,
            cf_current,
            cf_current as i64 - cf_base as i64
        ));
    }

    // Risk level
    output.push_str(&format!("behavioral_risk: {}\n", current.behavioral_risk.as_str()));
    if base.behavioral_risk != current.behavioral_risk {
        output.push_str(&format!(
            "CHANGED: risk {} → {}\n",
            base.behavioral_risk.as_str(),
            current.behavioral_risk.as_str()
        ));
    }

    output
}

/// Statistics for diff output
#[derive(Default)]
struct DiffStats {
    added: usize,
    modified: usize,
    deleted: usize,
    high_risk: usize,
    medium_risk: usize,
    low_risk: usize,
}

impl DiffStats {
    fn merge(&mut self, other: &DiffStats) {
        self.added += other.added;
        self.modified += other.modified;
        self.deleted += other.deleted;
        self.high_risk += other.high_risk;
        self.medium_risk += other.medium_risk;
        self.low_risk += other.low_risk;
    }
}

/// Count total nodes in the AST
fn count_nodes(node: &tree_sitter::Node) -> usize {
    let mut count = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_nodes(&child);
    }
    count
}

/// Print AST for debugging
fn print_ast(node: &tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = node
        .utf8_text(source.as_bytes())
        .unwrap_or("<invalid utf8>");
    let text_preview: String = text.chars().take(50).collect();
    let text_preview = text_preview.replace('\n', "\\n");

    eprintln!(
        "{}{}:{} [{}-{}] \"{}\"{}",
        indent,
        node.kind(),
        if node.is_named() { "" } else { " (anonymous)" },
        node.start_position().row,
        node.end_position().row,
        text_preview,
        if text.len() > 50 { "..." } else { "" }
    );

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_ast(&child, source, depth + 1);
    }
}

/// Run static code analysis on the cached index
fn run_static_analysis(cli: &Cli) -> semfora_engine::Result<String> {
    use semfora_engine::analysis::{analyze_module, analyze_repo, format_analysis_report as format_report};

    let cwd = cli.dir.clone().unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Check if we have a cached index
    let cache = semfora_engine::CacheDir::for_repo(&cwd)?;
    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No cached index found. Run with --shard first to generate the index.".to_string(),
        });
    }

    // Check if analyzing a specific module
    if let Some(ref module_name) = cli.analyze_module {
        let metrics = analyze_module(&cwd, module_name)?;
        let mut output = String::new();
        output.push_str(&format!("Module: {}\n", metrics.name));
        output.push_str(&format!("  Files: {}\n", metrics.files));
        output.push_str(&format!("  Symbols: {}\n", metrics.symbols));
        output.push_str(&format!("  Avg Complexity: {:.1}\n", metrics.avg_complexity));
        output.push_str(&format!("  Max Complexity: {}\n", metrics.max_complexity));
        if let Some(ref most_complex) = metrics.most_complex_symbol {
            output.push_str(&format!("  Most Complex: {}\n", most_complex));
        }
        output.push_str(&format!("  Total LoC: {}\n", metrics.total_loc));
        output.push_str(&format!("  High Risk Count: {}\n", metrics.high_risk_count));
        output.push_str(&format!("  Instability: {:.2}\n", metrics.instability()));
        return Ok(output);
    }

    // Full repo analysis
    eprintln!("Running static analysis on cached index...");
    let analysis = analyze_repo(&cwd)?;
    let report = format_report(&analysis);

    Ok(report)
}

/// Run duplicate detection on the cached index
fn run_find_duplicates(cli: &Cli) -> semfora_engine::Result<String> {
    use semfora_engine::{CacheDir, DuplicateDetector, FunctionSignature};
    use std::io::{BufRead, BufReader};

    let cwd = cli.dir.clone().unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));
    let cache = CacheDir::for_repo(&cwd)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No cached index found. Run with --shard first to generate the index.".to_string(),
        });
    }

    // Load signatures from signature index
    let sig_path = cache.signature_index_path();
    if !sig_path.exists() {
        return Err(McpDiffError::GitError {
            message: "Signature index not found. Regenerate index with --shard.".to_string(),
        });
    }

    eprintln!("Loading function signatures...");
    let file = std::fs::File::open(&sig_path)?;
    let reader = BufReader::new(file);

    let mut signatures: Vec<FunctionSignature> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(sig) = serde_json::from_str::<FunctionSignature>(&line) {
            signatures.push(sig);
        }
    }

    if signatures.is_empty() {
        return Ok("No function signatures found in index.".to_string());
    }

    eprintln!("Analyzing {} signatures for duplicates...", signatures.len());

    // Configure detector
    let exclude_boilerplate = !cli.include_boilerplate;
    let detector = DuplicateDetector::new(cli.duplicate_threshold)
        .with_boilerplate_exclusion(exclude_boilerplate);

    // Find all clusters
    let clusters = detector.find_all_clusters(&signatures);

    // Format output
    let mut output = String::new();
    output.push_str(&format!("Duplicate Detection Results\n"));
    output.push_str(&format!("===========================\n\n"));
    output.push_str(&format!("Threshold: {:.0}%\n", cli.duplicate_threshold * 100.0));
    output.push_str(&format!("Boilerplate excluded: {}\n", exclude_boilerplate));
    output.push_str(&format!("Total signatures analyzed: {}\n", signatures.len()));
    output.push_str(&format!("Duplicate clusters found: {}\n\n", clusters.len()));

    if clusters.is_empty() {
        output.push_str("No duplicate clusters found above threshold.\n");
        return Ok(output);
    }

    // Count total duplicates
    let total_duplicates: usize = clusters.iter().map(|c| c.duplicates.len()).sum();
    output.push_str(&format!("Total duplicate functions: {}\n\n", total_duplicates));

    for (i, cluster) in clusters.iter().enumerate() {
        output.push_str(&format!("--- Cluster {} ---\n", i + 1));
        output.push_str(&format!("Primary: {} ({})\n", cluster.primary.name, cluster.primary.file));
        output.push_str(&format!("  Hash: {}\n", cluster.primary.hash));
        if cluster.primary.start_line > 0 {
            output.push_str(&format!("  Lines: {}-{}\n", cluster.primary.start_line, cluster.primary.end_line));
        }
        output.push_str(&format!("Duplicates ({}):\n", cluster.duplicates.len()));

        for dup in &cluster.duplicates {
            let kind_str = match dup.kind {
                semfora_engine::DuplicateKind::Exact => "EXACT",
                semfora_engine::DuplicateKind::Near => "NEAR",
                semfora_engine::DuplicateKind::Divergent => "DIVERGENT",
            };
            output.push_str(&format!(
                "  - {} ({}) [{} {:.0}%]\n",
                dup.symbol.name, dup.symbol.file, kind_str, dup.similarity * 100.0
            ));
            if dup.symbol.start_line > 0 {
                output.push_str(&format!("    Lines: {}-{}\n", dup.symbol.start_line, dup.symbol.end_line));
            }

            // Show differences for near/divergent matches
            if !dup.differences.is_empty() && dup.differences.len() <= 5 {
                for diff in &dup.differences {
                    output.push_str(&format!("    {}\n", diff));
                }
            }
        }
        output.push_str("\n");
    }

    Ok(output)
}

/// Check duplicates for a specific symbol
fn run_check_duplicates(cli: &Cli) -> semfora_engine::Result<String> {
    use semfora_engine::{CacheDir, DuplicateDetector, FunctionSignature};
    use std::io::{BufRead, BufReader};

    let symbol_hash = cli.check_duplicates.as_ref().unwrap();
    let cwd = cli.dir.clone().unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));
    let cache = CacheDir::for_repo(&cwd)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No cached index found. Run with --shard first to generate the index.".to_string(),
        });
    }

    // Load signatures
    let sig_path = cache.signature_index_path();
    if !sig_path.exists() {
        return Err(McpDiffError::GitError {
            message: "Signature index not found. Regenerate index with --shard.".to_string(),
        });
    }

    let file = std::fs::File::open(&sig_path)?;
    let reader = BufReader::new(file);

    let mut signatures: Vec<FunctionSignature> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(sig) = serde_json::from_str::<FunctionSignature>(&line) {
            signatures.push(sig);
        }
    }

    // Find target signature
    let target = signatures.iter()
        .find(|s| s.symbol_hash == *symbol_hash)
        .ok_or_else(|| McpDiffError::GitError {
            message: format!("Symbol {} not found in signature index.", symbol_hash),
        })?;

    // Configure detector
    let detector = DuplicateDetector::new(cli.duplicate_threshold);

    // Find duplicates
    let matches = detector.find_duplicates(target, &signatures);

    // Format output
    let mut output = String::new();
    output.push_str(&format!("Duplicate Check for: {}\n", target.name));
    output.push_str(&format!("File: {}\n", target.file));
    output.push_str(&format!("Threshold: {:.0}%\n", cli.duplicate_threshold * 100.0));
    output.push_str(&format!("Matches found: {}\n\n", matches.len()));

    if matches.is_empty() {
        output.push_str("No duplicates found for this symbol.\n");
        return Ok(output);
    }

    for m in &matches {
        let kind_str = match m.kind {
            semfora_engine::DuplicateKind::Exact => "EXACT",
            semfora_engine::DuplicateKind::Near => "NEAR",
            semfora_engine::DuplicateKind::Divergent => "DIVERGENT",
        };
        output.push_str(&format!("- {} ({})\n", m.symbol.name, m.symbol.file));
        output.push_str(&format!("  Similarity: {:.0}% [{}]\n", m.similarity * 100.0, kind_str));
        if m.symbol.start_line > 0 {
            output.push_str(&format!("  Lines: {}-{}\n", m.symbol.start_line, m.symbol.end_line));
        }
        output.push_str(&format!("  Hash: {}\n", m.symbol.hash));
        if !m.differences.is_empty() {
            for diff in &m.differences {
                output.push_str(&format!("  {}\n", diff));
            }
        }
        output.push_str("\n");
    }

    Ok(output)
}

// ============================================================================
// New Query Commands (CLI parity with MCP tools)
// ============================================================================

/// Get source code for a file with optional line range
fn run_get_source(cli: &Cli, file_path: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let full_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        repo_dir.join(file_path)
    };

    if !full_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: full_path.display().to_string(),
        });
    }

    let source = fs::read_to_string(&full_path)?;
    let lines: Vec<&str> = source.lines().collect();
    let total_lines = lines.len();

    let start = cli.start_line.unwrap_or(1).saturating_sub(1);
    let end = cli.end_line.unwrap_or(total_lines).min(total_lines);
    let context = cli.context;

    // Apply context
    let actual_start = start.saturating_sub(context);
    let actual_end = (end + context).min(total_lines);

    let mut output = String::new();
    output.push_str(&format!("// {} (lines {}-{}, showing {}-{})\n",
        full_path.display(), start + 1, end, actual_start + 1, actual_end));

    for (i, line) in lines.iter().enumerate().skip(actual_start).take(actual_end - actual_start) {
        let line_num = i + 1;
        let marker = if line_num > start && line_num <= end { ">" } else { " " };
        output.push_str(&format!("{:>5} |{} {}\n", line_num, marker, line));
    }

    Ok(output)
}

/// Direct ripgrep search (regex pattern)
fn run_raw_search(cli: &Cli, pattern: &str) -> semfora_engine::Result<String> {
    use semfora_engine::ripgrep::{RipgrepSearcher, SearchOptions};

    let repo_dir = get_repo_dir(cli)?;
    let limit = cli.limit;
    let merge_threshold = cli.merge_threshold;

    let mut options = SearchOptions::new(pattern)
        .with_limit(limit)
        .with_merge_threshold(merge_threshold);

    if !cli.case_sensitive {
        options = options.case_insensitive();
    }

    if let Some(ref types) = cli.file_types {
        let file_types: Vec<String> = types.split(',').map(|s| s.trim().to_string()).collect();
        options = options.with_file_types(file_types);
    }

    let searcher = RipgrepSearcher::new();

    let mut output = String::new();

    if merge_threshold > 0 {
        match searcher.search_merged(&repo_dir, &options) {
            Ok(blocks) => {
                output.push_str(&format!("pattern: \"{}\"\n", pattern));
                output.push_str(&format!("blocks[{}]:\n", blocks.len()));
                for block in &blocks {
                    let relative_file = block.file.strip_prefix(&repo_dir)
                        .unwrap_or(&block.file)
                        .to_string_lossy();
                    output.push_str(&format!("\n--- {}:{}-{} ---\n", relative_file, block.start_line, block.end_line));
                    for line in &block.lines {
                        let prefix = if line.is_match { ">" } else { " " };
                        output.push_str(&format!("{} {:>4} | {}\n", prefix, line.line, line.content));
                    }
                }
            }
            Err(e) => return Err(McpDiffError::GitError {
                message: format!("Search failed: {}", e),
            }),
        }
    } else {
        match searcher.search(&repo_dir, &options) {
            Ok(matches) => {
                output.push_str(&format!("pattern: \"{}\"\n", pattern));
                output.push_str(&format!("matches[{}]:\n", matches.len()));
                for m in &matches {
                    let relative_file = m.file.strip_prefix(&repo_dir)
                        .unwrap_or(&m.file)
                        .to_string_lossy();
                    output.push_str(&format!("  {}:{}:{}: {}\n",
                        relative_file, m.line, m.column, m.content.trim()));
                }
            }
            Err(e) => return Err(McpDiffError::GitError {
                message: format!("Search failed: {}", e),
            }),
        }
    }

    Ok(output)
}

/// Semantic search using BM25 (natural language query)
fn run_semantic_search(cli: &Cli, query: &str) -> semfora_engine::Result<String> {
    use semfora_engine::bm25::Bm25Index;

    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.has_bm25_index() {
        return Err(McpDiffError::FileNotFound {
            path: "BM25 index not found. Run with --shard first to generate index.".to_string(),
        });
    }

    let bm25_path = cache.bm25_index_path();
    let index = Bm25Index::load(&bm25_path).map_err(|e| McpDiffError::GitError {
        message: format!("Failed to load BM25 index: {}", e),
    })?;

    let limit = cli.limit;
    let include_source = cli.include_source;

    let mut results = index.search(query, limit * 2);

    // Apply filters
    if let Some(ref kind_filter) = cli.kind {
        let kind_lower = kind_filter.to_lowercase();
        results.retain(|r| r.kind.to_lowercase() == kind_lower);
    }
    if let Some(ref module_filter) = cli.module {
        let module_lower = module_filter.to_lowercase();
        results.retain(|r| r.module.to_lowercase() == module_lower);
    }

    results.truncate(limit);

    let mut output = String::new();

    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "query": query,
                "results": results.iter().map(|r| serde_json::json!({
                    "symbol": r.symbol,
                    "kind": r.kind,
                    "hash": r.hash,
                    "file": r.file,
                    "lines": r.lines,
                    "module": r.module,
                    "risk": r.risk,
                    "score": r.score,
                    "matched_terms": r.matched_terms
                })).collect::<Vec<_>>(),
                "count": results.len()
            });
            output = serde_json::to_string_pretty(&json).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&format!("query: \"{}\"\n", query));
            output.push_str(&format!("results[{}]:\n", results.len()));

            for result in &results {
                output.push_str(&format!("\n## {} ({})\n", result.symbol, result.kind));
                output.push_str(&format!("hash: {}\n", result.hash));
                output.push_str(&format!("file: {}\n", result.file));
                output.push_str(&format!("lines: {}\n", result.lines));
                output.push_str(&format!("module: {}\n", result.module));
                output.push_str(&format!("risk: {}\n", result.risk));
                output.push_str(&format!("score: {:.3}\n", result.score));
                output.push_str(&format!("matched_terms: {}\n", result.matched_terms.join(", ")));

                if include_source {
                    if let Some(source) = get_source_snippet(&cache, &result.file, &result.lines, 2) {
                        output.push_str("__source__:\n");
                        output.push_str(&source);
                    }
                }
            }

            let suggestions = index.suggest_related_terms(query, 5);
            if !suggestions.is_empty() {
                output.push_str(&format!("\n---\nrelated_terms: {}\n", suggestions.join(", ")));
            }
        }
    }

    Ok(output)
}

/// Get all symbols in a specific file
fn run_file_symbols(cli: &Cli, file_path: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "No index found. Run with --shard first to generate index.".to_string(),
        });
    }

    let include_source = cli.include_source;
    let context = cli.context;

    // Normalize the target file path to absolute
    let target_path = Path::new(file_path);
    let target_file = if target_path.is_absolute() {
        file_path.to_string()
    } else {
        // Convert relative path to absolute using repo_dir
        repo_dir.join(file_path).to_string_lossy().to_string()
    };

    let symbols: Vec<_> = match cache.load_all_symbol_entries() {
        Ok(all) => {
            all.into_iter()
                .filter(|e| {
                    // Compare normalized paths
                    e.file == target_file ||
                    e.file.ends_with(&format!("/{}", file_path.trim_start_matches("./"))) ||
                    Path::new(&e.file) == Path::new(&target_file)
                })
                .filter(|e| {
                    cli.kind.as_ref().map_or(true, |k| e.kind == *k)
                })
                .collect()
        }
        Err(e) => return Err(McpDiffError::GitError {
            message: format!("Failed to load symbol index: {}", e),
        }),
    };

    let mut output = String::new();

    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "file": file_path,
                "symbols": symbols,
                "count": symbols.len()
            });
            output = serde_json::to_string_pretty(&json).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&format!("file: \"{}\"\n", file_path));
            output.push_str(&format!("symbols[{}]:\n", symbols.len()));

            for entry in &symbols {
                output.push_str(&format!(
                    "  {} ({}) [{}] - {}:{}\n",
                    entry.symbol, entry.kind, entry.risk, entry.file, entry.lines
                ));
            }

            if include_source && !symbols.is_empty() {
                output.push_str("\n__sources__:\n");
                for entry in &symbols {
                    if let Some(source) = get_source_snippet(&cache, &entry.file, &entry.lines, context) {
                        output.push_str(&format!("\n--- {} ({}) ---\n", entry.symbol, entry.lines));
                        output.push_str(&source);
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Get callers of a symbol (reverse call graph)
fn run_get_callers(cli: &Cli, symbol_hash: &str) -> semfora_engine::Result<String> {
    let repo_dir = get_repo_dir(cli)?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "No index found. Run with --shard first to generate index.".to_string(),
        });
    }

    let depth = cli.depth.min(3);
    let limit = cli.limit;
    let include_source = cli.include_source;

    let call_graph_path = cache.call_graph_path();
    if !call_graph_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "Call graph not found. Run generate_index to create it.".to_string(),
        });
    }

    let content = fs::read_to_string(&call_graph_path)?;

    // Build reverse call graph (callee -> callers)
    let mut reverse_graph: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut symbol_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for line in content.lines() {
        if line.starts_with("_type:") || line.starts_with("schema_version:") || line.starts_with("edges:") {
            continue;
        }
        if let Some(colon_pos) = line.find(':') {
            let caller = line[..colon_pos].trim().to_string();
            let rest = line[colon_pos + 1..].trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                let inner = &rest[1..rest.len()-1];
                for callee in inner.split(',').filter(|s| !s.is_empty()) {
                    let callee = callee.trim().trim_matches('"').to_string();
                    if !callee.starts_with("ext:") {
                        reverse_graph.entry(callee.clone()).or_default().push(caller.clone());
                    }
                }
            }
        }
    }

    // Load symbol index for name resolution
    if let Ok(entries) = cache.load_all_symbol_entries() {
        for entry in entries {
            symbol_names.insert(entry.hash.clone(), entry.symbol.clone());
        }
    }

    // Find callers at each depth level
    let target_name = symbol_names.get(symbol_hash).cloned().unwrap_or_else(|| symbol_hash.to_string());

    let mut all_callers: Vec<(String, String, usize)> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut current_level: Vec<String> = vec![symbol_hash.to_string()];

    for current_depth in 1..=depth {
        let mut next_level: Vec<String> = Vec::new();

        for hash in &current_level {
            if let Some(callers) = reverse_graph.get(hash) {
                for caller_hash in callers {
                    if !visited.contains(caller_hash) && all_callers.len() < limit {
                        visited.insert(caller_hash.clone());
                        let caller_name = symbol_names.get(caller_hash)
                            .cloned()
                            .unwrap_or_else(|| caller_hash.clone());
                        all_callers.push((caller_hash.clone(), caller_name, current_depth));
                        next_level.push(caller_hash.clone());
                    }
                }
            }
        }

        current_level = next_level;
        if current_level.is_empty() {
            break;
        }
    }

    let mut output = String::new();

    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "target": target_name,
                "target_hash": symbol_hash,
                "depth": depth,
                "callers": all_callers.iter().map(|(hash, name, d)| serde_json::json!({
                    "hash": hash,
                    "name": name,
                    "depth": d
                })).collect::<Vec<_>>(),
                "count": all_callers.len()
            });
            output = serde_json::to_string_pretty(&json).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&format!("target: {} ({})\n", target_name, symbol_hash));
            output.push_str(&format!("depth: {}\n", depth));
            output.push_str(&format!("total_callers: {}\n", all_callers.len()));

            if all_callers.is_empty() {
                output.push_str("callers: (none - this may be an entry point or unused)\n");
            } else {
                output.push_str(&format!("callers[{}]:\n", all_callers.len()));
                for (hash, name, d) in &all_callers {
                    output.push_str(&format!("  {} ({}) depth={}\n", name, hash, d));
                }

                if include_source {
                    output.push_str("\n__caller_sources__:\n");
                    for (hash, name, _) in all_callers.iter().take(5) {
                        let symbol_path = cache.symbol_path(hash);
                        if symbol_path.exists() {
                            if let Ok(content) = fs::read_to_string(&symbol_path) {
                                let mut file: Option<String> = None;
                                let mut lines: Option<String> = None;
                                for line in content.lines() {
                                    let trimmed = line.trim();
                                    if trimmed.starts_with("file:") {
                                        file = Some(trimmed.trim_start_matches("file:").trim().trim_matches('"').to_string());
                                    } else if trimmed.starts_with("lines:") {
                                        lines = Some(trimmed.trim_start_matches("lines:").trim().trim_matches('"').to_string());
                                    }
                                }
                                if let (Some(f), Some(l)) = (file, lines) {
                                    if let Some(source) = get_source_snippet(&cache, &f, &l, 2) {
                                        output.push_str(&format!("\n--- {} ({}) ---\n", name, l));
                                        output.push_str(&source);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Helper: Get source snippet for a symbol given file and line range
fn get_source_snippet(cache: &CacheDir, file: &str, lines: &str, context: usize) -> Option<String> {
    let (start_line, end_line) = if let Some((s, e)) = lines.split_once('-') {
        (s.parse::<usize>().ok()?, e.parse::<usize>().ok()?)
    } else {
        let line = lines.parse::<usize>().ok()?;
        (line, line)
    };

    let full_path = cache.repo_root.join(file);
    let source = fs::read_to_string(&full_path).ok()?;
    let all_lines: Vec<&str> = source.lines().collect();

    let actual_start = start_line.saturating_sub(1).saturating_sub(context);
    let actual_end = (end_line + context).min(all_lines.len());

    let mut output = String::new();
    for (i, line) in all_lines.iter().enumerate().skip(actual_start).take(actual_end - actual_start) {
        let line_num = i + 1;
        output.push_str(&format!("{:>5} | {}\n", line_num, line));
    }

    Some(output)
}

/// Prepare information for writing a commit message
/// Shows git context, staged/unstaged changes with semantic analysis
fn run_prep_commit(cli: &Cli) -> semfora_engine::Result<String> {
    use std::process::Command;
    use semfora_engine::normalize_kind;

    let repo_dir = get_repo_dir(cli)?;

    if !is_git_repo(Some(&repo_dir)) {
        return Err(McpDiffError::GitError {
            message: "Not a git repository".to_string(),
        });
    }

    // Extract options from CLI
    let include_complexity = cli.show_complexity;
    let include_all_metrics = cli.show_all_metrics;
    let staged_only = cli.staged_only;
    let auto_refresh = !cli.no_auto_refresh;
    let show_diff_stats = !cli.no_diff_stats;

    // Check index freshness if auto-refresh is enabled
    if auto_refresh {
        if let Ok(cache) = CacheDir::for_repo(&repo_dir) {
            if cache.exists() {
                // Check staleness
                let meta_path = cache.root.join("meta.json");
                if let Ok(meta_content) = fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                        if let Some(indexed_at) = meta.get("indexed_at").and_then(|v| v.as_str()) {
                            if let Ok(indexed_time) = chrono::DateTime::parse_from_rfc3339(indexed_at) {
                                let age = chrono::Utc::now().signed_duration_since(indexed_time);
                                if age > chrono::Duration::hours(1) {
                                    eprintln!("Note: Semantic index is stale. Run with --shard to refresh.");
                                }
                            }
                        }
                    }
                }
            } else {
                eprintln!("Note: No semantic index found. Run with --shard to generate one for richer analysis.");
            }
        }
    }

    // Get git context
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&repo_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let remote = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&repo_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let commit_info = Command::new("git")
        .args(["log", "-1", "--format=%h|%s"])
        .current_dir(&repo_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let (last_commit_hash, last_commit_message) = if let Some(info) = commit_info {
        let parts: Vec<&str> = info.splitn(2, '|').collect();
        if parts.len() >= 2 {
            (Some(parts[0].to_string()), Some(parts[1].to_string()))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Get staged changes
    let staged_changes = get_staged_changes(Some(&repo_dir))?;

    // Get unstaged changes (unless staged_only)
    let unstaged_changes = if staged_only {
        Vec::new()
    } else {
        get_unstaged_changes(Some(&repo_dir))?
    };

    // If no changes at all, return early
    if staged_changes.is_empty() && unstaged_changes.is_empty() {
        return Ok("_type: prep_commit\n_note: No changes to commit.\n\nstaged_changes: (none)\nunstaged_changes: (none)\n".to_string());
    }

    // Helper struct for file analysis results
    struct AnalyzedFile {
        path: String,
        change_type: String,
        insertions: usize,
        deletions: usize,
        symbols: Vec<SymbolInfo>,
        error: Option<String>,
    }

    struct SymbolInfo {
        name: String,
        kind: String,
        lines: String,
        cognitive: Option<usize>,
        cyclomatic: Option<usize>,
        max_nesting: Option<usize>,
        fan_out: Option<usize>,
        loc: Option<usize>,
        state_mutations: Option<usize>,
        io_operations: Option<usize>,
    }

    // Helper to analyze changed files
    let analyze_files = |changes: &[ChangedFile]| -> Vec<AnalyzedFile> {
        changes.iter().map(|changed_file| {
            let file_path = repo_dir.join(&changed_file.path);
            let change_type_str = format!("{:?}", changed_file.change_type);

            // Get diff stats
            let (insertions, deletions) = if show_diff_stats {
                let stat_output = Command::new("git")
                    .args(["diff", "--numstat", "--cached", "--", &changed_file.path])
                    .current_dir(&repo_dir)
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

                let stat_output = stat_output.or_else(|| {
                    Command::new("git")
                        .args(["diff", "--numstat", "--", &changed_file.path])
                        .current_dir(&repo_dir)
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                });

                stat_output.map(|stat| {
                    let parts: Vec<&str> = stat.split_whitespace().collect();
                    if parts.len() >= 2 {
                        (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0))
                    } else {
                        (0, 0)
                    }
                }).unwrap_or((0, 0))
            } else {
                (0, 0)
            };

            // Skip deleted files
            if matches!(changed_file.change_type, ChangeType::Deleted) {
                return AnalyzedFile {
                    path: changed_file.path.clone(),
                    change_type: change_type_str,
                    insertions,
                    deletions,
                    symbols: Vec::new(),
                    error: Some("file deleted".to_string()),
                };
            }

            // Check if file exists
            if !file_path.exists() {
                return AnalyzedFile {
                    path: changed_file.path.clone(),
                    change_type: change_type_str,
                    insertions,
                    deletions,
                    symbols: Vec::new(),
                    error: Some("file not found".to_string()),
                };
            }

            // Check if it's a supported language
            let lang = match Lang::from_path(&file_path) {
                Ok(l) => l,
                Err(_) => {
                    return AnalyzedFile {
                        path: changed_file.path.clone(),
                        change_type: change_type_str,
                        insertions,
                        deletions,
                        symbols: Vec::new(),
                        error: Some("unsupported language".to_string()),
                    };
                }
            };

            // Parse and extract symbols
            let source = match fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    return AnalyzedFile {
                        path: changed_file.path.clone(),
                        change_type: change_type_str,
                        insertions,
                        deletions,
                        symbols: Vec::new(),
                        error: Some(format!("read error: {}", e)),
                    };
                }
            };

            let summary = match parse_and_extract(&file_path, &source, lang, cli) {
                Ok(s) => s,
                Err(e) => {
                    return AnalyzedFile {
                        path: changed_file.path.clone(),
                        change_type: change_type_str,
                        insertions,
                        deletions,
                        symbols: Vec::new(),
                        error: Some(format!("parse error: {}", e)),
                    };
                }
            };

            // Create symbol info from the semantic summary
            let lines = format!(
                "{}-{}",
                summary.start_line.unwrap_or(1),
                summary.end_line.unwrap_or(1)
            );

            let (cognitive, cyclomatic, max_nesting, fan_out, loc, state_mutations, io_operations) =
                if include_complexity || include_all_metrics {
                    let complexity = semfora_engine::analysis::symbol_complexity_from_summary(&summary, 0);
                    (
                        Some(complexity.cognitive as usize),
                        Some(complexity.cyclomatic as usize),
                        Some(complexity.max_nesting as usize),
                        if include_all_metrics { Some(complexity.fan_out as usize) } else { None },
                        if include_all_metrics { Some(complexity.loc as usize) } else { None },
                        if include_all_metrics { Some(complexity.state_mutations as usize) } else { None },
                        if include_all_metrics { Some(complexity.io_operations as usize) } else { None },
                    )
                } else {
                    (None, None, None, None, None, None, None)
                };

            // Get symbol name and kind from the summary
            let symbol_name = summary.symbol.clone().unwrap_or_else(|| {
                // Use file stem as fallback name
                file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });
            let symbol_kind = summary
                .symbol_kind
                .map(|k| format!("{:?}", k).to_lowercase())
                .unwrap_or_else(|| "file".to_string());

            let symbols = vec![SymbolInfo {
                name: symbol_name,
                kind: symbol_kind,
                lines,
                cognitive,
                cyclomatic,
                max_nesting,
                fan_out,
                loc,
                state_mutations,
                io_operations,
            }];

            AnalyzedFile {
                path: changed_file.path.clone(),
                change_type: change_type_str,
                insertions,
                deletions,
                symbols,
                error: None,
            }
        }).collect()
    };

    // Analyze files
    let staged_files = analyze_files(&staged_changes);
    let unstaged_files = analyze_files(&unstaged_changes);

    // Format output as TOON
    let mut output = String::new();
    output.push_str("_type: prep_commit\n");
    output.push_str("_note: Information for commit message. This tool DOES NOT commit.\n\n");

    // Git context
    output.push_str("git_context:\n");
    output.push_str(&format!("  branch: \"{}\"\n", branch));
    if let Some(ref r) = remote {
        output.push_str(&format!("  remote: \"{}\"\n", r));
    }
    if let Some(ref h) = last_commit_hash {
        output.push_str(&format!("  last_commit: \"{}\"\n", h));
    }
    if let Some(ref m) = last_commit_message {
        let truncated = if m.len() > 60 {
            format!("{}...", &m[..57])
        } else {
            m.clone()
        };
        output.push_str(&format!("  last_message: \"{}\"\n", truncated));
    }
    output.push('\n');

    // Summary
    let staged_symbol_count: usize = staged_files.iter().map(|f| f.symbols.len()).sum();
    let unstaged_symbol_count: usize = unstaged_files.iter().map(|f| f.symbols.len()).sum();

    output.push_str("summary:\n");
    output.push_str(&format!("  staged_files: {}\n", staged_files.len()));
    output.push_str(&format!("  staged_symbols: {}\n", staged_symbol_count));
    output.push_str(&format!("  unstaged_files: {}\n", unstaged_files.len()));
    output.push_str(&format!("  unstaged_symbols: {}\n", unstaged_symbol_count));

    if show_diff_stats {
        let staged_insertions: usize = staged_files.iter().map(|f| f.insertions).sum();
        let staged_deletions: usize = staged_files.iter().map(|f| f.deletions).sum();
        output.push_str(&format!("  staged_changes: +{} -{}\n", staged_insertions, staged_deletions));
    }
    output.push('\n');

    // Helper to format file list
    let format_files = |files: &[AnalyzedFile], output: &mut String| {
        for file in files {
            let mut header = format!("  {} [{}]", file.path, file.change_type);
            if show_diff_stats && (file.insertions > 0 || file.deletions > 0) {
                header.push_str(&format!(" (+{} -{})", file.insertions, file.deletions));
            }
            output.push_str(&header);
            output.push('\n');

            if let Some(ref err) = file.error {
                output.push_str(&format!("    ({})\n", err));
                continue;
            }

            if file.symbols.is_empty() {
                output.push_str("    symbols: (none detected)\n");
                continue;
            }

            output.push_str(&format!("    symbols[{}]:\n", file.symbols.len()));
            for sym in &file.symbols {
                output.push_str(&format!("      - {} ({}) L{}\n", sym.name, sym.kind, sym.lines));

                if include_complexity || include_all_metrics {
                    let mut metrics = Vec::new();
                    if let Some(cog) = sym.cognitive {
                        metrics.push(format!("cognitive={}", cog));
                    }
                    if let Some(cyc) = sym.cyclomatic {
                        metrics.push(format!("cyclomatic={}", cyc));
                    }
                    if let Some(nest) = sym.max_nesting {
                        metrics.push(format!("nesting={}", nest));
                    }
                    if !metrics.is_empty() {
                        output.push_str(&format!("        complexity: {}\n", metrics.join(", ")));
                    }
                }

                if include_all_metrics {
                    let mut metrics = Vec::new();
                    if let Some(fo) = sym.fan_out {
                        metrics.push(format!("fan_out={}", fo));
                    }
                    if let Some(loc) = sym.loc {
                        metrics.push(format!("loc={}", loc));
                    }
                    if let Some(sm) = sym.state_mutations {
                        if sm > 0 {
                            metrics.push(format!("mutations={}", sm));
                        }
                    }
                    if let Some(io) = sym.io_operations {
                        if io > 0 {
                            metrics.push(format!("io_ops={}", io));
                        }
                    }
                    if !metrics.is_empty() {
                        output.push_str(&format!("        metrics: {}\n", metrics.join(", ")));
                    }
                }
            }
        }
    };

    // Staged changes
    if !staged_files.is_empty() {
        output.push_str(&format!("staged_changes[{}]:\n", staged_files.len()));
        format_files(&staged_files, &mut output);
        output.push('\n');
    } else {
        output.push_str("staged_changes: (none)\n\n");
    }

    // Unstaged changes
    if !unstaged_files.is_empty() {
        output.push_str(&format!("unstaged_changes[{}]:\n", unstaged_files.len()));
        format_files(&unstaged_files, &mut output);
    } else {
        output.push_str("unstaged_changes: (none)\n");
    }

    Ok(output)
}
