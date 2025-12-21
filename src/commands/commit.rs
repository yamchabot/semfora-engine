//! Commit command handler - Prepare information for writing commit messages

use std::fs;
use std::process::Command;

use crate::cache::CacheDir;
use crate::cli::{CommitArgs, OutputFormat};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::git::{
    get_current_branch, get_last_commit, get_remote_url, get_staged_changes, get_unstaged_changes,
    is_git_repo, ChangeType, ChangedFile,
};
use crate::parsing::parse_and_extract;
use crate::Lang;

/// Run the commit command - prepare information for commit message
pub fn run_commit(args: &CommitArgs, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    if !is_git_repo(Some(&repo_dir)) {
        return Err(McpDiffError::GitError {
            message: "Not a git repository".to_string(),
        });
    }

    // Check index freshness if auto-refresh is enabled
    if !args.no_auto_refresh {
        if let Ok(cache) = CacheDir::for_repo(&repo_dir) {
            if cache.exists() {
                let meta_path = cache.root.join("meta.json");
                if let Ok(meta_content) = fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                        if let Some(indexed_at) = meta.get("indexed_at").and_then(|v| v.as_str()) {
                            if let Ok(indexed_time) =
                                chrono::DateTime::parse_from_rfc3339(indexed_at)
                            {
                                let age = chrono::Utc::now().signed_duration_since(indexed_time);
                                if age > chrono::Duration::hours(1) {
                                    eprintln!("Note: Semantic index is stale. Run `semfora index generate` to refresh.");
                                }
                            }
                        }
                    }
                }
            } else {
                eprintln!("Note: No semantic index found. Run `semfora index generate` for richer analysis.");
            }
        }
    }

    // Get git context (DEDUP-104: uses shared git module)
    let branch = get_current_branch(Some(&repo_dir)).unwrap_or_else(|_| "unknown".to_string());
    let remote = get_remote_url(None, Some(&repo_dir));
    let (last_commit_hash, last_commit_message) = get_last_commit(Some(&repo_dir))
        .map(|c| (Some(c.short_sha), Some(c.subject)))
        .unwrap_or((None, None));

    // Get staged changes
    let staged_changes = get_staged_changes(Some(&repo_dir))?;

    // Get unstaged changes (unless staged_only)
    let unstaged_changes = if args.staged {
        Vec::new()
    } else {
        get_unstaged_changes(Some(&repo_dir))?
    };

    // If no changes at all, return early
    if staged_changes.is_empty() && unstaged_changes.is_empty() {
        return Ok("_type: prep_commit\n_note: No changes to commit.\n\nstaged_changes: (none)\nunstaged_changes: (none)\n".to_string());
    }

    // Analyze files
    let show_diff_stats = !args.no_diff_stats;
    let include_complexity = args.metrics;
    let include_all_metrics = args.all_metrics;

    let staged_files = analyze_changed_files(
        &staged_changes,
        &repo_dir,
        show_diff_stats,
        include_complexity,
        include_all_metrics,
    );
    let unstaged_files = analyze_changed_files(
        &unstaged_changes,
        &repo_dir,
        show_diff_stats,
        include_complexity,
        include_all_metrics,
    );

    // Format output
    let mut output = String::new();

    // Build JSON value for all formats
    let json_value = serde_json::json!({
        "_type": "prep_commit",
        "_note": "Information for commit message. This tool DOES NOT commit.",
        "git_context": {
            "branch": branch,
            "remote": remote,
            "last_commit": last_commit_hash,
            "last_message": last_commit_message
        },
        "summary": {
            "staged_files": staged_files.len(),
            "unstaged_files": unstaged_files.len()
        },
        "staged_changes": staged_files,
        "unstaged_changes": unstaged_files
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("_type: prep_commit\n");
            output
                .push_str("_note: Information for commit message. This tool DOES NOT commit.\n\n");

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
                output.push_str(&format!(
                    "  staged_changes: +{} -{}\n",
                    staged_insertions, staged_deletions
                ));
            }
            output.push('\n');

            // Staged changes
            if !staged_files.is_empty() {
                output.push_str(&format!("staged_changes[{}]:\n", staged_files.len()));
                format_file_list(
                    &staged_files,
                    &mut output,
                    show_diff_stats,
                    include_complexity,
                    include_all_metrics,
                );
                output.push('\n');
            } else {
                output.push_str("staged_changes: (none)\n\n");
            }

            // Unstaged changes
            if !unstaged_files.is_empty() {
                output.push_str(&format!("unstaged_changes[{}]:\n", unstaged_files.len()));
                format_file_list(
                    &unstaged_files,
                    &mut output,
                    show_diff_stats,
                    include_complexity,
                    include_all_metrics,
                );
            } else {
                output.push_str("unstaged_changes: (none)\n");
            }
        }
    }

    Ok(output)
}

// ============================================
// Helper Types
// ============================================

#[derive(Debug, Clone, serde::Serialize)]
struct AnalyzedFile {
    path: String,
    change_type: String,
    insertions: usize,
    deletions: usize,
    symbols: Vec<SymbolInfo>,
    error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct SymbolInfo {
    name: String,
    kind: String,
    lines: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cognitive: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cyclomatic: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_nesting: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fan_out: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loc: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_mutations: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    io_operations: Option<usize>,
}

// ============================================
// Helper Functions
// ============================================

fn analyze_changed_files(
    changes: &[ChangedFile],
    repo_dir: &std::path::Path,
    show_diff_stats: bool,
    include_complexity: bool,
    include_all_metrics: bool,
) -> Vec<AnalyzedFile> {
    changes
        .iter()
        .map(|changed_file| {
            let file_path = repo_dir.join(&changed_file.path);
            let change_type_str = format!("{:?}", changed_file.change_type);

            // Get diff stats
            let (insertions, deletions) = if show_diff_stats {
                get_diff_stats(repo_dir, &changed_file.path)
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

            let summary = match parse_and_extract(&file_path, &source, lang) {
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
                    let complexity = crate::analysis::symbol_complexity_from_summary(&summary, 0);
                    (
                        Some(complexity.cognitive as usize),
                        Some(complexity.cyclomatic as usize),
                        Some(complexity.max_nesting as usize),
                        if include_all_metrics {
                            Some(complexity.fan_out as usize)
                        } else {
                            None
                        },
                        if include_all_metrics {
                            Some(complexity.loc as usize)
                        } else {
                            None
                        },
                        if include_all_metrics {
                            Some(complexity.state_mutations as usize)
                        } else {
                            None
                        },
                        if include_all_metrics {
                            Some(complexity.io_operations as usize)
                        } else {
                            None
                        },
                    )
                } else {
                    (None, None, None, None, None, None, None)
                };

            // Get symbol name and kind from the summary
            let symbol_name = summary.symbol.clone().unwrap_or_else(|| {
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
        })
        .collect()
}

fn get_diff_stats(repo_dir: &std::path::Path, file_path: &str) -> (usize, usize) {
    // Try staged first
    let stat_output = Command::new("git")
        .args(["diff", "--numstat", "--cached", "--", file_path])
        .current_dir(repo_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Fall back to unstaged
    let stat_output = stat_output.or_else(|| {
        Command::new("git")
            .args(["diff", "--numstat", "--", file_path])
            .current_dir(repo_dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });

    stat_output
        .map(|stat| {
            let parts: Vec<&str> = stat.split_whitespace().collect();
            if parts.len() >= 2 {
                (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0))
            } else {
                (0, 0)
            }
        })
        .unwrap_or((0, 0))
}

// parse_and_extract removed - now uses crate::parsing::parse_and_extract (DEDUP-103)

fn format_file_list(
    files: &[AnalyzedFile],
    output: &mut String,
    show_diff_stats: bool,
    include_complexity: bool,
    include_all_metrics: bool,
) {
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
            output.push_str(&format!(
                "      - {} ({}) L{}\n",
                sym.name, sym.kind, sym.lines
            ));

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
}
