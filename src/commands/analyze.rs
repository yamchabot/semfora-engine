//! Analyze command implementation
//!
//! Handles file, directory, and git diff analysis.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::cli::{AnalyzeArgs, OutputFormat, TokenAnalysisMode};
use crate::error::{McpDiffError, Result};
use crate::git::{
    detect_base_branch, get_changed_files, get_commit_changed_files, get_commits_since,
    get_file_at_ref, get_merge_base, get_repo_root, get_staged_changes, get_uncommitted_changes,
    get_unstaged_changes, ChangeType, ChangedFile,
};
use crate::mcp_server::formatting::{format_diff_output_paginated, format_diff_summary};
use crate::parsing::{parse_and_extract, parse_and_extract_with_options};
use crate::tokens::{format_analysis_compact, format_analysis_report, TokenAnalyzer};
use crate::{
    encode_toon, encode_toon_directory, fs_utils, generate_repo_overview, is_test_file, CacheDir,
    Lang, SemanticSummary, ShardWriter,
};

use super::CommandContext;

/// Run the analyze command
pub fn run_analyze(ctx: &CommandContext, args: &AnalyzeArgs) -> Result<String> {
    // Determine what kind of analysis to perform
    if args.uncommitted {
        let base_ref = args.base.clone().unwrap_or_else(|| "HEAD".to_string());
        return run_uncommitted(ctx, args, &base_ref);
    }

    if let Some(ref diff_ref) = args.diff {
        let base_ref = resolve_base_ref(args, diff_ref)?;
        if args.all_commits {
            return run_all_commits(ctx, args, &base_ref);
        }
        return run_diff_branch(ctx, args, &base_ref);
    }

    if let Some(ref sha) = args.commit {
        return run_single_commit(ctx, args, sha);
    }

    if args.all_commits {
        let base_ref = resolve_base_ref(args, &"auto".to_string())?;
        return run_all_commits(ctx, args, &base_ref);
    }

    // File or directory analysis
    let path = args.path.clone().unwrap_or_else(|| PathBuf::from("."));

    if path.is_file() {
        run_single_file(ctx, args, &path)
    } else if path.is_dir() {
        if args.shard {
            run_shard(ctx, args, &path)
        } else {
            run_directory(ctx, args, &path)
        }
    } else if !path.exists() {
        Err(McpDiffError::FileNotFound {
            path: path.display().to_string(),
        })
    } else {
        Err(McpDiffError::GitError {
            message: format!("Invalid path: {}", path.display()),
        })
    }
}

/// Resolve the base ref for diff operations
fn resolve_base_ref(args: &AnalyzeArgs, diff_ref: &str) -> Result<String> {
    if let Some(ref base) = args.base {
        return Ok(base.clone());
    }

    if diff_ref != "auto" {
        return Ok(diff_ref.to_string());
    }

    detect_base_branch(None)
}

/// Large file thresholds (matching MCP constants)
const VERY_LARGE_FILE_BYTES: u64 = 500_000;
const LARGE_FILE_LINES: usize = 3000;
const LARGE_SYMBOL_COUNT: usize = 50;

/// Analyze a single file
///
/// Supports focus mode (start_line/end_line) and output modes (symbols_only, summary, full).
/// Handles large files with navigation hints when focus mode is not used.
fn run_single_file(ctx: &CommandContext, args: &AnalyzeArgs, file_path: &Path) -> Result<String> {
    if !file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: file_path.display().to_string(),
        });
    }

    let lang = Lang::from_path(file_path)?;

    if ctx.verbose {
        eprintln!(
            "Detected language: {} ({})",
            lang.name(),
            lang.family().name()
        );
    }

    let source = fs::read_to_string(file_path)?;

    if ctx.verbose {
        eprintln!("Read {} bytes from {}", source.len(), file_path.display());
    }

    // Large file detection
    let file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    let line_count = source.lines().count();
    let has_focus = args.start_line.is_some() && args.end_line.is_some();

    // For very large files without focus, return metadata with navigation hints
    if (file_size > VERY_LARGE_FILE_BYTES || line_count > LARGE_FILE_LINES) && !has_focus {
        // Do a quick parse to get symbol count for the hint
        let symbol_hint = if let Ok(summary) = parse_and_extract(file_path, &source, lang) {
            format!(
                "\nsymbols_found: {}\nhigh_risk_count: {}\n",
                summary.symbols.len(),
                summary
                    .symbols
                    .iter()
                    .filter(|s| s.behavioral_risk == crate::RiskLevel::High)
                    .count()
            )
        } else {
            String::new()
        };

        return Ok(format!(
            "_type: large_file_notice\n\
             file: {}\n\
             size_bytes: {}\n\
             line_count: {}\n\
             language: {}\n\
             {}\n\
             This file is very large ({:.1}KB, {} lines).\n\
             Use focus mode to analyze a specific section:\n\
               semfora-engine analyze {} --start-line N --end-line M\n\n\
             Or use get-file to see symbol index with line ranges.\n",
            file_path.display(),
            file_size,
            line_count,
            lang.name(),
            symbol_hint,
            file_size as f64 / 1024.0,
            line_count,
            file_path.display()
        ));
    }

    // Focus mode: extract only specified line range
    let source_to_analyze = if let (Some(start), Some(end)) = (args.start_line, args.end_line) {
        let lines: Vec<&str> = source.lines().collect();
        let start_idx = start.saturating_sub(1);
        let end_idx = end.min(lines.len());

        if start_idx >= lines.len() {
            return Err(McpDiffError::ExtractionFailure {
                message: format!(
                    "start_line {} exceeds file length {} lines",
                    start,
                    lines.len()
                ),
            });
        }

        lines[start_idx..end_idx].join("\n")
    } else {
        source.clone()
    };

    let summary =
        parse_and_extract_with_options(file_path, &source_to_analyze, lang, args.print_ast)?;

    // Handle output mode
    let output = match args.output_mode.as_str() {
        "symbols_only" => {
            // Just list symbols with line ranges
            let mut out = format!(
                "_type: symbols_only\nfile: {}\nsymbol_count: {}\n\n",
                file_path.display(),
                summary.symbols.len()
            );
            for sym in &summary.symbols {
                out.push_str(&format!(
                    "- {} ({}) L{}-{}\n",
                    sym.name,
                    sym.kind.as_str(),
                    sym.start_line,
                    sym.end_line
                ));
            }
            out
        }
        "summary" => {
            // Brief overview only
            format!(
                "_type: analysis_summary\n\
                 file: {}\n\
                 language: {}\n\
                 symbols: {}\n\
                 calls: {}\n\
                 high_risk: {}\n",
                file_path.display(),
                summary.language,
                summary.symbols.len(),
                summary.calls.len(),
                summary
                    .symbols
                    .iter()
                    .filter(|s| s.behavioral_risk == crate::RiskLevel::High)
                    .count()
            )
        }
        _ => {
            // Full output (default)
            let toon_output = encode_toon(&summary);
            let json_pretty = serde_json::to_string_pretty(&summary).map_err(|e| {
                McpDiffError::ExtractionFailure {
                    message: format!("JSON serialization failed: {}", e),
                }
            })?;
            let json_compact =
                serde_json::to_string(&summary).map_err(|e| McpDiffError::ExtractionFailure {
                    message: format!("JSON serialization failed: {}", e),
                })?;

            // Token analysis if requested
            if let Some(mode) = args.analyze_tokens {
                let analyzer = TokenAnalyzer::new();
                let analysis = analyzer.analyze(&source, &json_pretty, &json_compact, &toon_output);

                let report = match mode {
                    TokenAnalysisMode::Full => {
                        format_analysis_report(&analysis, args.compare_compact)
                    }
                    TokenAnalysisMode::Compact => {
                        format_analysis_compact(&analysis, args.compare_compact)
                    }
                };
                eprintln!("{}", report);
            }

            let mut output = match ctx.format {
                OutputFormat::Text => {
                    // Human-readable text format
                    let mut text = String::new();
                    text.push_str("═══════════════════════════════════════════\n");
                    text.push_str("  SEMANTIC ANALYSIS\n");
                    text.push_str("═══════════════════════════════════════════\n\n");
                    text.push_str(&format!("file: {}\n", file_path.display()));
                    text.push_str(&format!(
                        "language: {} ({})\n",
                        lang.name(),
                        lang.family().name()
                    ));
                    if let Some(ref sym) = summary.symbol {
                        text.push_str(&format!("symbol: {}\n", sym));
                    }
                    if let Some(ref kind) = summary.symbol_kind {
                        text.push_str(&format!("kind: {:?}\n", kind));
                    }
                    if let Some(start) = summary.start_line {
                        if let Some(end) = summary.end_line {
                            text.push_str(&format!("lines: {}-{}\n", start, end));
                        }
                    }
                    text.push('\n');
                    text.push_str(&toon_output);
                    text
                }
                OutputFormat::Toon => toon_output,
                OutputFormat::Json => json_pretty,
            };

            // Add focus context if applicable
            if has_focus {
                output = format!(
                    "_type: focused_analysis\n\
                     file: {}\n\
                     focus_range: L{}-{}\n\
                     ---\n{}",
                    file_path.display(),
                    args.start_line.unwrap_or(1),
                    args.end_line.unwrap_or(0),
                    output
                );
            }

            // Symbol count warning for large files (only for non-JSON formats)
            if summary.symbols.len() > LARGE_SYMBOL_COUNT && ctx.format != OutputFormat::Json {
                output = format!(
                    "# Note: {} symbols found. Consider using --output-mode=symbols_only for overview first.\n\n{}",
                    summary.symbols.len(),
                    output
                );
            }

            output
        }
    };

    Ok(format!("{}\n", output))
}

/// Analyze a directory
fn run_directory(ctx: &CommandContext, args: &AnalyzeArgs, dir_path: &Path) -> Result<String> {
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

    let files = collect_files(dir_path, args.max_depth, args);

    if files.is_empty() {
        return Ok(format!(
            "directory: {}\nfiles_found: 0\n",
            dir_path.display()
        ));
    }

    if ctx.verbose {
        eprintln!("Found {} files to analyze", files.len());
    }

    let all_source_len_atomic = AtomicUsize::new(0);
    let total_lines_atomic = AtomicUsize::new(0);
    let verbose = ctx.verbose;

    let summaries: Vec<SemanticSummary> = files
        .par_iter()
        .filter_map(|file_path| {
            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

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

    let dir_str = dir_path.display().to_string();
    let overview = generate_repo_overview(&summaries, &dir_str);

    let output = if args.summary_only {
        encode_toon_directory(&overview, &[])
    } else {
        encode_toon_directory(&overview, &summaries)
    };

    if let Some(mode) = args.analyze_tokens {
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
                all_source_len,
                toon_len,
                compression_ratio,
                summaries.len(),
                total_lines
            ),
            TokenAnalysisMode::Compact => format!(
                "compression: {:.1}% ({} source → {} toon, {} files)",
                compression_ratio,
                all_source_len,
                toon_len,
                summaries.len()
            ),
        };
        eprintln!("{}", report);
    }

    Ok(output)
}

/// Generate sharded index for large repositories
fn run_shard(ctx: &CommandContext, args: &AnalyzeArgs, dir_path: &Path) -> Result<String> {
    if !dir_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: dir_path.display().to_string(),
        });
    }

    // Get canonical path for consistent cache lookup (normalized for Windows)
    let canonical_path = fs_utils::normalize_path(
        &dir_path
            .canonicalize()
            .unwrap_or_else(|_| dir_path.to_path_buf()),
    );
    let cache = CacheDir::for_repo(&canonical_path)?;

    if ctx.verbose {
        eprintln!("Cache directory: {}", cache.root.display());
    }

    // Incremental mode: check if cache exists and skip full reindex
    // For now, incremental just means "only if cache doesn't exist"
    if args.incremental && cache.exists() {
        return Ok(format!(
            "index_status: fresh\n\
             cache: {}\n\
             hint: Use --force to regenerate\n",
            cache.root.display()
        ));
    }

    // Full reindex
    let files = collect_files(dir_path, args.max_depth, args);

    if files.is_empty() {
        return Ok(format!(
            "directory: {}\nfiles_found: 0\n",
            dir_path.display()
        ));
    }

    if ctx.verbose {
        eprintln!("Found {} files to index", files.len());
    }

    let total_files = files.len();
    let processed = AtomicUsize::new(0);
    let show_progress = ctx.progress;
    let verbose = ctx.verbose;

    let summaries: Vec<SemanticSummary> = files
        .par_iter()
        .filter_map(|file_path| {
            let current = processed.fetch_add(1, Ordering::Relaxed);
            if show_progress && current % 100 == 0 {
                let pct = (current as f64 / total_files as f64) * 100.0;
                eprintln!("Progress: {:.1}% ({}/{})", pct, current, total_files);
            }

            let lang = match Lang::from_path(file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    if verbose {
                        eprintln!("Skipping {}: {}", file_path.display(), e);
                    }
                    return None;
                }
            };

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

    if ctx.verbose {
        eprintln!("Analyzed {} files successfully", summaries.len());
    }

    // Write sharded output
    let mut writer = ShardWriter::new(&canonical_path)?;
    writer.add_summaries(summaries.clone());
    let stats = writer.write_all(&canonical_path.display().to_string())?;

    Ok(format!(
        "index_generated: true\n\
         files: {}\n\
         modules: {}\n\
         symbols: {}\n\
         cache: {}\n",
        summaries.len(),
        stats.modules_written,
        stats.symbols_written,
        cache.root.display()
    ))
}

/// Analyze uncommitted changes
fn run_uncommitted(ctx: &CommandContext, _args: &AnalyzeArgs, _base_ref: &str) -> Result<String> {
    let repo_root = PathBuf::from(get_repo_root(None)?);

    let staged = get_staged_changes(None)?;
    let unstaged = get_unstaged_changes(None)?;

    let mut all_changes: Vec<ChangedFile> = Vec::new();
    all_changes.extend(staged.into_iter());
    all_changes.extend(unstaged.into_iter());

    // Deduplicate by path
    all_changes.sort_by(|a, b| a.path.cmp(&b.path));
    all_changes.dedup_by(|a, b| a.path == b.path);

    if all_changes.is_empty() {
        return Ok("uncommitted_changes: 0\n".to_string());
    }

    let verbose = ctx.verbose;
    let summaries: Vec<SemanticSummary> = all_changes
        .par_iter()
        .filter_map(|change| {
            let file_path = repo_root.join(&change.path);

            if !file_path.exists() {
                return None;
            }

            let lang = match Lang::from_path(&file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            let source = match fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    if verbose {
                        eprintln!("Skipping {}: {}", change.path, e);
                    }
                    return None;
                }
            };

            parse_and_extract_string(&file_path, &source, lang).ok()
        })
        .collect();

    let overview = generate_repo_overview(&summaries, "uncommitted");
    let output = encode_toon_directory(&overview, &summaries);

    Ok(output)
}

/// Analyze diff against a base branch
///
/// Supports pagination (limit/offset) and summary_only mode.
/// Uses the same formatting as MCP analyze_diff for consistent output.
/// If args.path is set, uses it as the repo root (for MCP working_dir support).
fn run_diff_branch(ctx: &CommandContext, args: &AnalyzeArgs, base_ref: &str) -> Result<String> {
    // Use args.path as repo root if provided, otherwise auto-detect
    let repo_root = match &args.path {
        Some(p) if p.is_dir() => p.clone(),
        _ => PathBuf::from(get_repo_root(None)?),
    };
    let target_ref = args.target_ref.as_deref().unwrap_or("HEAD");

    // Handle special case for uncommitted changes (WORKING target)
    let (changed_files, display_target) = if target_ref.eq_ignore_ascii_case("WORKING") {
        let files = get_uncommitted_changes(base_ref, Some(&repo_root))?;
        (files, "WORKING (uncommitted)")
    } else {
        // Normal comparison between refs
        let merge_base = get_merge_base(base_ref, target_ref, Some(&repo_root))
            .unwrap_or_else(|_| base_ref.to_string());
        let files = get_changed_files(&merge_base, target_ref, Some(&repo_root))?;
        (files, target_ref)
    };

    if changed_files.is_empty() {
        return Ok(format!(
            "_type: analyze_diff\nbase: \"{}\"\ntarget: \"{}\"\ntotal_files: 0\n_note: No files changed.\n",
            base_ref, display_target
        ));
    }

    // Extract pagination options with defaults
    let limit = args.limit.unwrap_or(20).min(100); // Default 20, max 100
    let offset = args.offset.unwrap_or(0);

    // Choose output format based on options
    let output = if args.summary_only {
        format_diff_summary(&repo_root, base_ref, display_target, &changed_files)
    } else {
        format_diff_output_paginated(
            &repo_root,
            base_ref,
            display_target,
            &changed_files,
            offset,
            limit,
        )
    };

    if ctx.verbose {
        eprintln!(
            "Analyzed diff: {} -> {} ({} files)",
            base_ref,
            display_target,
            changed_files.len()
        );
    }

    Ok(output)
}

/// Analyze a single commit
fn run_single_commit(ctx: &CommandContext, _args: &AnalyzeArgs, sha: &str) -> Result<String> {
    let changed_files = get_commit_changed_files(sha, None)?;

    if changed_files.is_empty() {
        return Ok(format!("commit: {}\nchanged_files: 0\n", sha));
    }

    let repo_root = PathBuf::from(get_repo_root(None)?);
    let verbose = ctx.verbose;

    let summaries: Vec<SemanticSummary> = changed_files
        .par_iter()
        .filter_map(|change| {
            if change.change_type == ChangeType::Deleted {
                return None;
            }

            // Get file content at the commit
            let source = match get_file_at_ref(&change.path, sha, None) {
                Ok(Some(s)) => s,
                Ok(None) => {
                    if verbose {
                        eprintln!("Skipping {}: file not found at ref", change.path);
                    }
                    return None;
                }
                Err(e) => {
                    if verbose {
                        eprintln!("Skipping {}: {}", change.path, e);
                    }
                    return None;
                }
            };

            let file_path = repo_root.join(&change.path);
            let lang = match Lang::from_path(&file_path) {
                Ok(l) => l,
                Err(_) => return None,
            };

            parse_and_extract_string(&file_path, &source, lang).ok()
        })
        .collect();

    let overview = generate_repo_overview(&summaries, &format!("commit:{}", sha));
    let output = encode_toon_directory(&overview, &summaries);

    Ok(output)
}

/// Analyze all commits since base
fn run_all_commits(_ctx: &CommandContext, _args: &AnalyzeArgs, base_ref: &str) -> Result<String> {
    let commits = get_commits_since(base_ref, None)?;

    if commits.is_empty() {
        return Ok(format!("base: {}\ncommits: 0\n", base_ref));
    }

    let mut output = String::new();
    output.push_str(&format!("base: {}\n", base_ref));
    output.push_str(&format!("commits[{}]:\n", commits.len()));

    for commit in &commits {
        output.push_str(&format!("\n## {} - {}\n", &commit.sha[..8], commit.subject));

        let changed_files = get_commit_changed_files(&commit.sha, None)?;
        output.push_str(&format!("files_changed: {}\n", changed_files.len()));
    }

    Ok(output)
}

// ============================================
// Helper Functions
// ============================================

/// Parse and extract with string source (for parallel processing).
///
/// Uses the shared parsing module (DEDUP-103).
#[inline]
fn parse_and_extract_string(file_path: &Path, source: &str, lang: Lang) -> Result<SemanticSummary> {
    parse_and_extract(file_path, source, lang)
}

/// Collect files for analysis
fn collect_files(dir_path: &Path, max_depth: usize, args: &AnalyzeArgs) -> Vec<PathBuf> {
    collect_files_recursive(dir_path, max_depth, 0, args)
}

/// Recursively collect files
fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    args: &AnalyzeArgs,
) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if current_depth > max_depth {
        return files;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden directories and common non-source directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "build"
                || name == "dist"
                || name == "__pycache__"
                || name == ".git"
                || name == "vendor"
            {
                continue;
            }
        }

        if path.is_dir() {
            files.extend(collect_files_recursive(
                &path,
                max_depth,
                current_depth + 1,
                args,
            ));
        } else if path.is_file() {
            // Check extension filter
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if !args.should_process_extension(ext) {
                    continue;
                }
            }

            // Skip test files unless allowed
            if !args.allow_tests && is_test_file(&path.display().to_string()) {
                continue;
            }

            // Only include files we can analyze
            if Lang::from_path(&path).is_ok() {
                files.push(path);
            }
        }
    }

    files
}
