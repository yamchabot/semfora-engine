//! Semfora-MCP CLI entry point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use semfora_mcp::cli::{OperationMode, TokenAnalysisMode};
use semfora_mcp::git::{
    get_changed_files, get_commit_changed_files, get_commits_since, get_file_at_ref,
    get_merge_base, get_repo_root, is_git_repo, ChangedFile, ChangeType,
};
use semfora_mcp::{
    encode_toon, encode_toon_directory, extract, format_analysis_compact, format_analysis_report,
    generate_repo_overview, Cli, Lang, McpDiffError, OutputFormat, SemanticSummary, TokenAnalyzer,
    CacheDir, ShardWriter, get_cache_base_dir, list_cached_repos, prune_old_caches,
    analyze_repo_tokens, is_test_file,
};

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

fn run() -> semfora_mcp::Result<String> {
    let cli = Cli::parse();

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

/// Run single file analysis (Phase 1 mode)
fn run_single_file(cli: &Cli, file_path: &Path) -> semfora_mcp::Result<String> {
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
fn run_directory(cli: &Cli, dir_path: &Path, max_depth: usize) -> semfora_mcp::Result<String> {
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

    // First pass: collect all summaries
    let mut summaries: Vec<SemanticSummary> = Vec::new();
    let mut all_source_len = 0usize;
    let mut _failed_count = 0usize;
    let mut total_lines = 0usize;

    for file_path in &files {
        let relative_path = file_path
            .strip_prefix(dir_path)
            .unwrap_or(file_path)
            .display()
            .to_string();

        // Try to detect language
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Read and analyze file
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Skipping {}: {}", relative_path, e);
                }
                continue;
            }
        };

        all_source_len += source.len();
        total_lines += source.lines().count();

        let summary = match parse_and_extract_string(file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Failed to analyze {}: {}", relative_path, e);
                }
                _failed_count += 1;
                continue;
            }
        };

        summaries.push(summary);
    }

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
fn run_shard(cli: &Cli, dir_path: &Path, max_depth: usize) -> semfora_mcp::Result<String> {
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

    eprintln!("Cache location: {}", shard_writer.cache_path().display());

    // Collect all supported files
    let files = collect_files(dir_path, max_depth, cli);

    if files.is_empty() {
        return Ok(format!(
            "directory: {}\nfiles_found: 0\nNo files to shard.\n",
            dir_path.display()
        ));
    }

    eprintln!("Found {} files to analyze", files.len());

    // First pass: collect all summaries
    let mut summaries: Vec<SemanticSummary> = Vec::new();
    let mut total_source_bytes = 0usize;
    let mut processed = 0usize;
    let total = files.len();

    for file_path in &files {
        processed += 1;
        if processed % 100 == 0 || processed == total {
            eprintln!("Processing: {}/{} ({:.1}%)", processed, total, (processed as f64 / total as f64) * 100.0);
        }

        // Try to detect language
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Read and analyze file
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Skipping {}: {}", file_path.display(), e);
                }
                continue;
            }
        };

        total_source_bytes += source.len();

        let summary = match parse_and_extract_string(file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Failed to analyze {}: {}", file_path.display(), e);
                }
                continue;
            }
        };

        summaries.push(summary);
    }

    eprintln!("Analyzed {} files ({} bytes source)", summaries.len(), total_source_bytes);

    // Add summaries to shard writer
    shard_writer.add_summaries(summaries);

    // Write all shards
    let dir_str = dir_path.display().to_string();
    let stats = shard_writer.write_all(&dir_str)?;

    // Format output
    let (cache_size, _module_count) = shard_writer.cache_stats();
    let compression = if total_source_bytes > 0 {
        ((total_source_bytes as f64 - stats.total_bytes() as f64) / total_source_bytes as f64) * 100.0
    } else {
        0.0
    };

    let mut output = String::new();
    output.push_str(&format!("═══════════════════════════════════════════════════════\n"));
    output.push_str(&format!("  SHARDED INDEX CREATED\n"));
    output.push_str(&format!("═══════════════════════════════════════════════════════\n\n"));

    output.push_str(&format!("directory: {}\n", dir_path.display()));
    output.push_str(&format!("cache: {}\n", shard_writer.cache_path().display()));
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
fn run_cache_info() -> semfora_mcp::Result<String> {
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
fn run_cache_clear() -> semfora_mcp::Result<String> {
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
fn run_cache_prune(days: u32) -> semfora_mcp::Result<String> {
    let count = prune_old_caches(days)?;

    let mut output = String::new();
    output.push_str(&format!("Pruned {} cache(s) older than {} days.\n", count, days));

    Ok(output)
}

/// Run token efficiency benchmark
fn run_benchmark(dir_path: &Path) -> semfora_mcp::Result<String> {
    let metrics = analyze_repo_tokens(dir_path)?;
    Ok(metrics.report())
}

// ============================================================================
// Shard Query Commands
// ============================================================================

/// List all modules in the cached index
fn run_list_modules(cli: &Cli) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

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
fn run_get_module(cli: &Cli, module_name: &str) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

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

/// Search for symbols by name in the cached index
fn run_search_symbols(cli: &Cli, query: &str) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

    if !cache.has_symbol_index() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No symbol index found. Run with --shard first to generate index."),
        });
    }

    let results = cache.search_symbols(
        query,
        None, // module filter - could add later
        cli.kind.as_deref(),
        cli.risk.as_deref(),
        cli.limit,
    )?;

    let mut output = String::new();

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

    Ok(output)
}

/// List all symbols in a module from the cached index
fn run_list_module_symbols(cli: &Cli, module_name: &str) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

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
fn run_get_symbol(cli: &Cli, symbol_hash: &str) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

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
fn run_get_overview(cli: &Cli) -> semfora_mcp::Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

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
fn run_diff_branch(cli: &Cli, base_ref: &str) -> semfora_mcp::Result<String> {
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
fn run_uncommitted(cli: &Cli, base_ref: &str) -> semfora_mcp::Result<String> {
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
    let changed_files = semfora_mcp::git::get_uncommitted_changes(base_ref, None)?;

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
    changed_file: &semfora_mcp::git::ChangedFile,
    _base_ref: &str,
) -> semfora_mcp::Result<Option<(String, DiffStats)>> {
    let file_path = repo_root.join(&changed_file.path);
    let mut stats = DiffStats::default();

    // Update stats based on change type
    match changed_file.change_type {
        semfora_mcp::git::ChangeType::Added => stats.added += 1,
        semfora_mcp::git::ChangeType::Modified => stats.modified += 1,
        semfora_mcp::git::ChangeType::Deleted => stats.deleted += 1,
        _ => stats.modified += 1,
    }

    // Skip deleted files (can't analyze what doesn't exist)
    if changed_file.change_type == semfora_mcp::git::ChangeType::Deleted {
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
        semfora_mcp::git::ChangeType::Added => "[+]",
        semfora_mcp::git::ChangeType::Modified => "[M]",
        semfora_mcp::git::ChangeType::Deleted => "[-]",
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
fn run_single_commit(cli: &Cli, sha: &str) -> semfora_mcp::Result<String> {
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root = get_repo_root(None)?;
    let changed_files = get_commit_changed_files(sha, None)?;

    if changed_files.is_empty() {
        return Ok(format!("No files changed in commit {}.\n", sha));
    }

    let parent = semfora_mcp::git::get_parent_commit(sha, None)?;

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
fn run_all_commits(cli: &Cli, base_ref: &str) -> semfora_mcp::Result<String> {
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
        let parent = semfora_mcp::git::get_parent_commit(&commit.sha, None)
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
) -> semfora_mcp::Result<Option<(String, DiffStats)>> {
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
        semfora_mcp::RiskLevel::High => stats.high_risk += 1,
        semfora_mcp::RiskLevel::Medium => stats.medium_risk += 1,
        semfora_mcp::RiskLevel::Low => stats.low_risk += 1,
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
) -> semfora_mcp::Result<SemanticSummary> {
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
) -> semfora_mcp::Result<SemanticSummary> {
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
