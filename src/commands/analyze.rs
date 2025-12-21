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
    get_file_at_ref, get_repo_root, get_staged_changes, get_unstaged_changes, ChangeType,
    ChangedFile,
};
use crate::parsing::{parse_and_extract, parse_and_extract_with_options};
use crate::tokens::{format_analysis_compact, format_analysis_report, TokenAnalyzer};
use crate::{
    encode_toon, encode_toon_directory, fs_utils, generate_repo_overview, is_test_file,
    CacheDir, Lang, SemanticSummary, ShardWriter,
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

/// Analyze a single file
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

    let summary = parse_and_extract_with_options(file_path, &source, lang, args.print_ast)?;

    let toon_output = encode_toon(&summary);
    let json_pretty =
        serde_json::to_string_pretty(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;
    let json_compact =
        serde_json::to_string(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;

    if let Some(mode) = args.analyze_tokens {
        let analyzer = TokenAnalyzer::new();
        let analysis = analyzer.analyze(&source, &json_pretty, &json_compact, &toon_output);

        let report = match mode {
            TokenAnalysisMode::Full => format_analysis_report(&analysis, args.compare_compact),
            TokenAnalysisMode::Compact => format_analysis_compact(&analysis, args.compare_compact),
        };
        eprintln!("{}", report);
    }

    let output = match ctx.format {
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
            // Add the toon output as well for full detail
            text.push_str(&toon_output);
            text
        }
        OutputFormat::Toon => toon_output,
        OutputFormat::Json => json_pretty,
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
fn run_diff_branch(ctx: &CommandContext, _args: &AnalyzeArgs, base_ref: &str) -> Result<String> {
    let changed_files = get_changed_files(base_ref, "HEAD", None)?;

    if changed_files.is_empty() {
        return Ok(format!("diff_against: {}\nchanged_files: 0\n", base_ref));
    }

    let repo_root = PathBuf::from(get_repo_root(None)?);
    let verbose = ctx.verbose;

    let summaries: Vec<SemanticSummary> = changed_files
        .par_iter()
        .filter_map(|change| {
            if change.change_type == ChangeType::Deleted {
                return None;
            }

            let file_path = repo_root.join(&change.path);

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

    let overview = generate_repo_overview(&summaries, &format!("diff:{}", base_ref));
    let output = encode_toon_directory(&overview, &summaries);

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
