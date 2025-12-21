//! MCP Server for semfora-engine
//!
//! This module provides an MCP (Model Context Protocol) server that exposes
//! the semantic code analysis capabilities of semfora-engine as tools that can be
//! called by AI assistants like Claude.

mod formatting;
mod helpers;
mod types;

// Instruction variants for A/B testing - change import to switch:
// mod instructions_compact;   // Token efficiency (~500 tokens)
// mod instructions_complete;  // Full documentation (~4000 tokens)
mod instructions_fast; // Decision tree focused (~2000 tokens) - DEFAULT

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use tokio::sync::Mutex;

use crate::{
    // CLI types for MCP->CLI handler consolidation
    cli::{
        IndexArgs, IndexOperation, OutputFormat, SearchArgs, SecurityArgs, SecurityOperation,
        TestArgs,
    },
    commands::{run_index, run_search, run_security, run_test, CommandContext},
    duplicate::DuplicateDetector,
    encode_toon,
    encode_toon_directory,
    generate_repo_overview,
    normalize_kind,
    server::ServerState,
    test_runner::{self},
    utils::truncate_to_char_boundary,
    CacheDir,
    Lang,
};

// Re-export types for external use
use formatting::{
    analyze_files,
    extract_source_for_symbol,
    format_call_graph_paginated,
    format_call_graph_summary,
    // Diff formatting with pagination
    format_diff_output_paginated,
    format_diff_summary,
    format_duplicate_clusters_paginated,
    format_duplicate_matches,
    format_module_symbols,
    // Prep-commit formatting
    format_prep_commit,
    format_source_snippet,
    get_supported_languages,
    load_signatures,
    AnalyzedFile,
    FileDiffStats,
    GitContext,
    SymbolMetrics,
};
use helpers::{
    check_cache_staleness_detailed,
    collect_files,
    ensure_fresh_index,
    filter_repo_overview,
    // Validation helpers
    find_symbol_by_hash,
    find_symbol_by_location,
    format_batch_validation_results,
    format_freshness_note,
    format_validation_result,
    generate_index_internal,
    // String similarity
    levenshtein_distance,
    parse_and_extract,
    validate_single_symbol,
    validate_symbols_batch,
    FreshnessResult,
};
pub use types::*;
// Match this to the active module above:
use instructions_fast::MCP_INSTRUCTIONS;

// ============================================================================
// Large File Handling Constants
// ============================================================================

/// File size threshold for suggesting summary mode (100KB)
const LARGE_FILE_BYTES: u64 = 100_000;

/// File size threshold for requiring focus mode (500KB)
const VERY_LARGE_FILE_BYTES: u64 = 500_000;

/// Line count threshold for large file handling
const LARGE_FILE_LINES: usize = 3000;

/// Symbol count threshold for automatic summarization
const LARGE_SYMBOL_COUNT: usize = 50;

// ============================================================================
// MCP Server Implementation
// ============================================================================

/// MCP Server for semantic code analysis
#[derive(Clone)]
pub struct McpDiffServer {
    /// Working directory for operations
    working_dir: Arc<Mutex<PathBuf>>,
    /// Tool router for MCP
    tool_router: ToolRouter<McpDiffServer>,
    /// Optional persistent server state for live layer updates
    server_state: Option<Arc<ServerState>>,
}

impl Default for McpDiffServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl McpDiffServer {
    /// Create a new MCP server instance
    pub fn new() -> Self {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        Self {
            working_dir: Arc::new(Mutex::new(working_dir)),
            tool_router: Self::tool_router(),
            server_state: None,
        }
    }

    /// Create a new MCP server with a specific working directory
    pub fn with_working_dir(working_dir: PathBuf) -> Self {
        Self {
            working_dir: Arc::new(Mutex::new(working_dir)),
            tool_router: Self::tool_router(),
            server_state: None,
        }
    }

    /// Create a new MCP server with persistent server state
    ///
    /// This enables live layer updates via FileWatcher and GitPoller.
    pub fn with_server_state(working_dir: PathBuf, server_state: Arc<ServerState>) -> Self {
        Self {
            working_dir: Arc::new(Mutex::new(working_dir)),
            tool_router: Self::tool_router(),
            server_state: Some(server_state),
        }
    }

    /// Check if persistent server state is enabled
    pub fn has_server_state(&self) -> bool {
        self.server_state.is_some()
    }

    /// Get the server state if available
    pub fn server_state(&self) -> Option<&Arc<ServerState>> {
        self.server_state.as_ref()
    }

    /// Resolve a path relative to the working directory
    async fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            let wd = self.working_dir.lock().await;
            wd.join(path)
        }
    }

    /// Get the current working directory
    async fn get_working_dir(&self) -> PathBuf {
        self.working_dir.lock().await.clone()
    }

    /// Ensure a sharded index exists and is fresh for the repository.
    ///
    /// This transparently handles:
    /// - Missing index: generates a new one
    /// - Stale index with few changes: partial reindex
    /// - Stale index with many changes: full regeneration
    ///
    /// Returns FreshnessResult containing the cache and refresh status.
    async fn ensure_index(&self, repo_path: &Path) -> Result<FreshnessResult, String> {
        ensure_fresh_index(repo_path, None)
    }

    // ========================================================================
    // Quick Context Tool
    // ========================================================================

    #[tool(
        description = "Get quick git and project context in ~200 tokens. **Use this FIRST** when starting work on a repository to understand: current branch, last commit, index status, and project type. Much faster and smaller than get_repo_overview."
    )]
    async fn get_context(
        &self,
        Parameters(request): Parameters<GetContextRequest>,
    ) -> Result<CallToolResult, McpError> {
        use std::process::Command;

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let mut output = String::new();
        output.push_str("_type: context\n");

        // Get repo name from directory
        let repo_name = repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        output.push_str(&format!("repo_name: \"{}\"\n", repo_name));

        // Git branch
        let branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        output.push_str(&format!("branch: \"{}\"\n", branch));

        // Git remote
        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&repo_path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "none".to_string());
        output.push_str(&format!("remote: \"{}\"\n", remote));

        // Last commit info (hash, message, author, date)
        let commit_info = Command::new("git")
            .args(["log", "-1", "--format=%h|%s|%an|%ci"])
            .current_dir(&repo_path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        if let Some(info) = commit_info {
            let parts: Vec<&str> = info.splitn(4, '|').collect();
            if parts.len() >= 4 {
                output.push_str("last_commit:\n");
                output.push_str(&format!("  hash: \"{}\"\n", parts[0]));
                // Truncate message to 60 chars
                let msg = if parts[1].len() > 60 {
                    format!("{}...", &parts[1][..57])
                } else {
                    parts[1].to_string()
                };
                output.push_str(&format!("  message: \"{}\"\n", msg));
                output.push_str(&format!("  author: \"{}\"\n", parts[2]));
                // Simplify date to just the date part
                let date = parts[3].split(' ').next().unwrap_or(parts[3]);
                output.push_str(&format!("  date: \"{}\"\n", date));
            }
        }

        // Check index status
        let cache_result = CacheDir::for_repo(&repo_path);
        match cache_result {
            Ok(cache) if cache.exists() => {
                let staleness = check_cache_staleness_detailed(&cache, 3600);
                if staleness.is_stale {
                    output.push_str("index_status: \"stale\"\n");
                    output.push_str(&format!(
                        "stale_files: {}\n",
                        staleness.modified_files.len()
                    ));
                } else {
                    output.push_str("index_status: \"fresh\"\n");
                }

                // Try to read project type and entry points from overview
                let overview_path = cache.repo_overview_path();
                if let Ok(content) = fs::read_to_string(&overview_path) {
                    // Extract framework line
                    for line in content.lines() {
                        if line.starts_with("framework:") {
                            let framework = line
                                .trim_start_matches("framework:")
                                .trim()
                                .trim_matches('"');
                            output.push_str(&format!("project_type: \"{}\"\n", framework));
                            break;
                        }
                    }
                    // Extract entry points
                    for line in content.lines() {
                        if line.starts_with("entry_points") {
                            output.push_str(&format!("{}\n", line));
                            break;
                        }
                    }
                }
            }
            Ok(_) => {
                output.push_str("index_status: \"missing\"\n");
                output.push_str("hint: \"Run generate_index to create semantic index\"\n");
            }
            Err(_) => {
                output.push_str("index_status: \"error\"\n");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Analysis Tools
    // ========================================================================

    #[tool(
        description = "Unified analysis: auto-detects file, directory, or module. For files: extracts semantic info. For directories: returns overview with module grouping. For modules: returns detailed semantic info from index."
    )]
    async fn analyze(
        &self,
        Parameters(request): Parameters<AnalyzeRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Module mode: get module from index
        if let Some(module_name) = &request.module {
            let repo_path = match &request.path {
                Some(p) => self.resolve_path(p).await,
                None => self.get_working_dir().await,
            };

            let freshness = match self.ensure_index(&repo_path).await {
                Ok(r) => r,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };
            let cache = freshness.cache;

            let module_path = cache.module_path(module_name);
            if !module_path.exists() {
                let available = cache.list_modules();
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Module '{}' not found. Available modules: {}",
                    module_name,
                    available.join(", ")
                ))]));
            }

            return match fs::read_to_string(&module_path) {
                Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
                Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read module: {}",
                    e
                ))])),
            };
        }

        // Path-based analysis
        let path = match &request.path {
            Some(p) => p,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Error: Must provide either 'path' or 'module' parameter.",
                )]))
            }
        };

        let resolved_path = self.resolve_path(path).await;

        if !resolved_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Path not found: {}",
                resolved_path.display()
            ))]));
        }

        // Directory analysis
        if resolved_path.is_dir() {
            let max_depth = request.max_depth.unwrap_or(10);
            let summary_only = request.summary_only.unwrap_or(false);
            let extensions = request.extensions.clone().unwrap_or_default();

            let files = collect_files(&resolved_path, max_depth, &extensions);

            if files.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "directory: {}\nfiles_found: 0\n",
                    resolved_path.display()
                ))]));
            }

            let summaries = analyze_files(&files);
            let dir_str = resolved_path.display().to_string();
            let overview = generate_repo_overview(&summaries, &dir_str);

            let output = if summary_only {
                encode_toon_directory(&overview, &[])
            } else {
                encode_toon_directory(&overview, &summaries)
            };

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // File analysis
        let lang = match Lang::from_path(&resolved_path) {
            Ok(l) => l,
            Err(_) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unsupported file type: {}",
                    resolved_path.display()
                ))]))
            }
        };

        let source = match fs::read_to_string(&resolved_path) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file: {}",
                    e
                ))]))
            }
        };

        // Large file detection
        let file_size = fs::metadata(&resolved_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let line_count = source.lines().count();
        let has_focus = request.start_line.is_some() && request.end_line.is_some();

        // For very large files without focus, return metadata with navigation hints
        if (file_size > VERY_LARGE_FILE_BYTES || line_count > LARGE_FILE_LINES) && !has_focus {
            // Do a quick parse to get symbol count for the hint
            let symbol_hint = if let Ok(summary) = parse_and_extract(&resolved_path, &source, lang)
            {
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

            return Ok(CallToolResult::success(vec![Content::text(format!(
                "_type: large_file_notice\n\
                 file: {}\n\
                 size_bytes: {}\n\
                 line_count: {}\n\
                 language: {}\n\
                 {}\n\
                 This file is very large ({:.1}KB, {} lines).\n\
                 Use focus mode to analyze a specific section:\n\
                   analyze(path=\"{}\", start_line=N, end_line=M)\n\n\
                 Or use get_file to see symbol index with line ranges.\n",
                resolved_path.display(),
                file_size,
                line_count,
                lang.name(),
                symbol_hint,
                file_size as f64 / 1024.0,
                line_count,
                resolved_path.display()
            ))]));
        }

        // Focus mode: extract only specified line range
        let source_to_analyze = if let (Some(start), Some(end)) =
            (request.start_line, request.end_line)
        {
            let lines: Vec<&str> = source.lines().collect();
            let start_idx = start.saturating_sub(1);
            let end_idx = end.min(lines.len());

            if start_idx >= lines.len() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "start_line {} exceeds file length {} lines",
                    start, lines.len()
                ))]));
            }

            lines[start_idx..end_idx].join("\n")
        } else {
            source
        };

        let summary = match parse_and_extract(&resolved_path, &source_to_analyze, lang) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Analysis failed: {}",
                    e
                ))]))
            }
        };

        // Output mode support
        let output = match request.output_mode.as_deref() {
            Some("symbols_only") => {
                // Just list symbols with line ranges
                let mut out = format!(
                    "_type: symbols_only\nfile: {}\nsymbol_count: {}\n\n",
                    resolved_path.display(),
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
            Some("summary") => {
                // Brief overview only
                format!(
                    "_type: analysis_summary\n\
                     file: {}\n\
                     language: {}\n\
                     symbols: {}\n\
                     calls: {}\n\
                     high_risk: {}\n",
                    resolved_path.display(),
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
                let mut output = match request.format.as_deref() {
                    Some("json") => {
                        serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".to_string())
                    }
                    _ => encode_toon(&summary),
                };

                // Add focus context if applicable
                if has_focus {
                    output = format!(
                        "_type: focused_analysis\n\
                         file: {}\n\
                         focus_range: L{}-{}\n\
                         ---\n{}",
                        resolved_path.display(),
                        request.start_line.unwrap_or(1),
                        request.end_line.unwrap_or(0),
                        output
                    );
                }

                // Symbol count warning for large files
                if summary.symbols.len() > LARGE_SYMBOL_COUNT {
                    output = format!(
                        "# Note: {} symbols found. Consider using output_mode='symbols_only' for overview first.\n\n{}",
                        summary.symbols.len(),
                        output
                    );
                }

                output
            }
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "**Use for code reviews** - analyzes changes between git branches or commits semantically. Shows new/modified symbols, changed dependencies, and risk assessment for each file. Use target_ref='WORKING' to review uncommitted changes before committing. Supports pagination (limit/offset) for large diffs and summary_only mode for quick overview."
    )]
    async fn analyze_diff(
        &self,
        Parameters(request): Parameters<AnalyzeDiffRequest>,
    ) -> Result<CallToolResult, McpError> {
        let working_dir = match &request.working_dir {
            Some(wd) => self.resolve_path(wd).await,
            None => self.get_working_dir().await,
        };

        if !crate::git::is_git_repo(Some(&working_dir)) {
            return Ok(CallToolResult::error(vec![Content::text(
                "Not a git repository",
            )]));
        }

        let base_ref = &request.base_ref;
        let target_ref = request.target_ref.as_deref().unwrap_or("HEAD");

        // Extract pagination options with defaults
        let limit = request.limit.unwrap_or(20).min(100); // Default 20, max 100
        let offset = request.offset.unwrap_or(0);
        let summary_only = request.summary_only.unwrap_or(false);

        // Handle special case for uncommitted changes
        let (changed_files, display_target) = if target_ref.eq_ignore_ascii_case("WORKING") {
            // Compare base_ref against working tree (uncommitted changes)
            let files = match crate::git::get_uncommitted_changes(base_ref, Some(&working_dir)) {
                Ok(files) => files,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to get uncommitted changes: {}",
                        e
                    ))]))
                }
            };
            (files, "WORKING (uncommitted)")
        } else {
            // Normal comparison between refs
            let merge_base = crate::git::get_merge_base(base_ref, target_ref, Some(&working_dir))
                .unwrap_or_else(|_| base_ref.to_string());

            let files =
                match crate::git::get_changed_files(&merge_base, target_ref, Some(&working_dir)) {
                    Ok(files) => files,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to get changed files: {}",
                            e
                        ))]))
                    }
                };
            (files, target_ref)
        };

        if changed_files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "_type: analyze_diff\nbase: \"{}\"\ntarget: \"{}\"\ntotal_files: 0\n_note: No files changed.\n",
                base_ref,
                display_target
            ))]));
        }

        // Choose output format based on options
        let output = if summary_only {
            format_diff_summary(&working_dir, base_ref, display_target, &changed_files)
        } else {
            format_diff_output_paginated(
                &working_dir,
                base_ref,
                display_target,
                &changed_files,
                offset,
                limit,
            )
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Get all programming languages supported by semfora-engine for semantic analysis"
    )]
    fn get_languages(
        &self,
        Parameters(_request): Parameters<GetLanguagesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = get_supported_languages();
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Sharded Index Tools
    // ========================================================================

    #[tool(
        description = "Get the repository overview from a pre-built sharded index. Returns a compact summary with framework detection, module list, risk breakdown, and entry points. Use this to understand a codebase before diving into specific modules. Use max_modules param to control module listing (default 30, set 0 to exclude, high number for all)."
    )]
    async fn get_overview(
        &self,
        Parameters(request): Parameters<GetOverviewRequest>,
    ) -> Result<CallToolResult, McpError> {
        use std::process::Command;

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Parse options with defaults
        let max_modules = request.max_modules.unwrap_or(30);
        let exclude_test_dirs = request.exclude_test_dirs.unwrap_or(true);
        let include_git_context = request.include_git_context.unwrap_or(true);

        // Ensure index exists and is fresh (auto-generates or refreshes if needed)
        let freshness = match self.ensure_index(&repo_path).await {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let cache = freshness.cache.clone();

        let overview_path = cache.repo_overview_path();

        match fs::read_to_string(&overview_path) {
            Ok(content) => {
                // Build output with optional freshness notice
                let mut output = String::new();

                // Add freshness note if index was refreshed
                if let Some(note) = format_freshness_note(&freshness) {
                    output.push_str(&note);
                    output.push_str("\n\n");
                }

                // Add git context if requested
                if include_git_context {
                    let branch = Command::new("git")
                        .args(["rev-parse", "--abbrev-ref", "HEAD"])
                        .current_dir(&repo_path)
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

                    let commit = Command::new("git")
                        .args(["log", "-1", "--format=%h %s"])
                        .current_dir(&repo_path)
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

                    if branch.is_some() || commit.is_some() {
                        output.push_str("git_context:\n");
                        if let Some(b) = branch {
                            output.push_str(&format!("  branch: \"{}\"\n", b));
                        }
                        if let Some(c) = commit {
                            // Truncate commit message
                            let c = if c.len() > 60 {
                                format!("{}...", &c[..57])
                            } else {
                                c
                            };
                            output.push_str(&format!("  last_commit: \"{}\"\n", c));
                        }
                        output.push('\n');
                    }
                }

                // Filter and limit modules in the content
                let filtered_content =
                    filter_repo_overview(&content, max_modules, exclude_test_dirs);

                output.push_str(&filtered_content);
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read overview: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Get detailed semantic information for symbol(s). Three modes: (1) Single hash: use symbol_hash. (2) Batch: use hashes array (max 20). (3) Location: use file+line to find symbol at that position. Returns complete semantic summaries including calls, state changes, and control flow."
    )]
    async fn get_symbol(
        &self,
        Parameters(request): Parameters<GetSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Ensure index exists and is fresh
        let freshness = match self.ensure_index(&repo_path).await {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let cache = freshness.cache;

        // Check for batch mode (hashes takes precedence)
        if let Some(ref hashes) = request.hashes {
            if !hashes.is_empty() {
                // Batch mode
                let hashes: Vec<&str> = hashes.iter().take(20).map(|s| s.as_str()).collect();

                let include_source = request.include_source.unwrap_or(false);
                let context = request.context.unwrap_or(3);

                let mut output = String::new();
                output.push_str("_type: batch_symbols\n");
                output.push_str(&format!("requested: {}\n", hashes.len()));

                let mut found = 0;
                let mut not_found: Vec<&str> = Vec::new();

                for hash in &hashes {
                    let symbol_path = cache.symbol_path(hash);
                    if symbol_path.exists() {
                        match fs::read_to_string(&symbol_path) {
                            Ok(content) => {
                                output.push_str(&format!("\n--- {} ---\n", hash));
                                output.push_str(&content);

                                // Optionally include source code
                                if include_source {
                                    if let Some(source_snippet) =
                                        extract_source_for_symbol(&cache, &content, context)
                                    {
                                        output.push_str("\n__source__:\n");
                                        output.push_str(&source_snippet);
                                    }
                                }
                                found += 1;
                            }
                            Err(_) => not_found.push(hash),
                        }
                    } else {
                        not_found.push(hash);
                    }
                }

                output.push_str(&format!("\n_summary:\n  found: {}\n", found));
                if !not_found.is_empty() {
                    output.push_str(&format!("  not_found: {}\n", not_found.join(",")));
                }

                return Ok(CallToolResult::success(vec![Content::text(output)]));
            }
        }

        // File+line mode: find symbol at specific location
        if let (Some(file), Some(line)) = (&request.file, request.line) {
            let entry = match find_symbol_by_location(&cache, file, line) {
                Ok(e) => e,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };
            let symbol_path = cache.symbol_path(&entry.hash);
            return match fs::read_to_string(&symbol_path) {
                Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
                Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read symbol: {}",
                    e
                ))])),
            };
        }

        // Single symbol mode
        let symbol_hash = match &request.symbol_hash {
            Some(h) => h,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Either symbol_hash, hashes, or file+line must be provided",
                )]))
            }
        };

        let symbol_path = cache.symbol_path(symbol_hash);
        if !symbol_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Symbol '{}' not found in index",
                symbol_hash
            ))]));
        }

        match fs::read_to_string(&symbol_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read symbol: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Understand code flow and dependencies between functions. **Use with filters** (module, symbol) for targeted analysis - unfiltered output can be very large. Returns a mapping of symbol -> [called symbols]. Set export='sqlite' to export to SQLite database (expensive operation)."
    )]
    async fn get_callgraph(
        &self,
        Parameters(request): Parameters<GetCallgraphRequest>,
    ) -> Result<CallToolResult, McpError> {
        use std::io::{BufRead, BufReader};

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Ensure index exists and is fresh
        let freshness = match self.ensure_index(&repo_path).await {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let cache = freshness.cache.clone();

        // Handle export mode
        if let Some(export_format) = &request.export {
            if export_format == "sqlite" {
                use crate::sqlite_export::{default_export_path, SqliteExporter};
                use std::path::Path;

                let output_path = match &request.output_path {
                    Some(p) => {
                        let path = Path::new(p);
                        if path.is_absolute() {
                            path.to_path_buf()
                        } else {
                            repo_path.join(p)
                        }
                    }
                    None => default_export_path(&cache),
                };

                let batch_size = request.batch_size.unwrap_or(5000).clamp(100, 50000);
                let exporter = SqliteExporter::new().with_batch_size(batch_size);

                return match exporter.export(&cache, &output_path, None) {
                    Ok(stats) => {
                        let mut output = String::new();
                        output.push_str("_type: sqlite_export_result\n");
                        output.push_str("status: \"success\"\n");
                        output.push_str(&format!("output_path: \"{}\"\n", stats.output_path));
                        output.push_str(&format!(
                            "file_size_mb: {:.2}\n",
                            stats.file_size_bytes as f64 / 1024.0 / 1024.0
                        ));
                        output.push_str(&format!("nodes_exported: {}\n", stats.nodes_inserted));
                        output.push_str(&format!("edges_exported: {}\n", stats.edges_inserted));
                        output
                            .push_str(&format!("module_edges: {}\n", stats.module_edges_inserted));
                        output.push_str(&format!("duration_ms: {}\n", stats.duration_ms));
                        output.push_str("\nhint: \"Open with semfora-graph explorer or any SQLite client (DB Browser, DBeaver, sqlite3 CLI)\"\n");
                        Ok(CallToolResult::success(vec![Content::text(output)]))
                    }
                    Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                        "Export failed: {}",
                        e
                    ))])),
                };
            } else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown export format: '{}'. Supported: 'sqlite'",
                    export_format
                ))]));
            }
        }

        let call_graph_path = cache.call_graph_path();
        if !call_graph_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Call graph not found in index. The index may need to be regenerated.",
            )]));
        }

        // Parse parameters
        let limit = request.limit.unwrap_or(500).min(2000) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let summary_only = request.summary_only.unwrap_or(false);

        // Build hash-to-name mapping for symbol resolution and display
        let hash_to_name: std::collections::HashMap<String, String> = cache
            .load_all_symbol_entries()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.hash, e.symbol))
            .collect();

        // Resolve symbol filter to matching hashes
        // This enables searching by name (e.g., "get_symbol") instead of just hash
        let resolved_symbol_hashes: Option<std::collections::HashSet<String>> =
            if let Some(symbol) = &request.symbol {
                let symbol_lower = symbol.to_lowercase();

                // Check if it looks like a hash (contains : with hex-like chars)
                let is_hash_like = symbol.contains(':')
                    && symbol.chars().all(|c| c.is_ascii_hexdigit() || c == ':');

                if is_hash_like {
                    // Direct hash lookup - include if it exists in our mapping
                    if hash_to_name.contains_key(symbol) {
                        Some(std::iter::once(symbol.clone()).collect())
                    } else {
                        // Hash not found, but still use it for filtering (might be external)
                        Some(std::iter::once(symbol.clone()).collect())
                    }
                } else {
                    // Search by name - first try exact match, then partial
                    let exact_matches: std::collections::HashSet<String> = hash_to_name
                        .iter()
                        .filter(|(_, name)| name.to_lowercase() == symbol_lower)
                        .map(|(hash, _)| hash.clone())
                        .collect();

                    if !exact_matches.is_empty() {
                        Some(exact_matches)
                    } else {
                        // Fallback to partial/fuzzy match (BM25-style)
                        let partial_matches: std::collections::HashSet<String> = hash_to_name
                            .iter()
                            .filter(|(_, name)| name.to_lowercase().contains(&symbol_lower))
                            .map(|(hash, _)| hash.clone())
                            .collect();

                        if partial_matches.is_empty() {
                            // No matches at all - we'll return helpful message later
                            Some(std::collections::HashSet::new())
                        } else {
                            Some(partial_matches)
                        }
                    }
                }
            } else {
                None
            };

        // If symbol filter was provided but resolved to empty set, suggest alternatives
        if let Some(ref hashes) = resolved_symbol_hashes {
            if hashes.is_empty() {
                if let Some(symbol) = &request.symbol {
                    // Find similar symbol names to suggest
                    let symbol_lower = symbol.to_lowercase();
                    let mut suggestions: Vec<(&String, usize)> = hash_to_name
                        .values()
                        .filter(|name| {
                            let name_lower = name.to_lowercase();
                            // Simple similarity: shared prefix or substring
                            name_lower
                                .starts_with(&symbol_lower[..symbol_lower.len().min(3).max(1)])
                                || symbol_lower
                                    .split('_')
                                    .any(|part| name_lower.contains(part))
                        })
                        .map(|name| {
                            (
                                name,
                                levenshtein_distance(&symbol_lower, &name.to_lowercase()),
                            )
                        })
                        .collect();
                    suggestions.sort_by_key(|(_, dist)| *dist);
                    suggestions.truncate(5);

                    let mut output = String::new();
                    output.push_str("_type: call_graph\n");
                    output.push_str(&format!("symbol_filter: \"{}\"\n", symbol));
                    output.push_str("status: \"no_matches\"\n");
                    output.push_str(&format!(
                        "message: \"No symbol found matching '{}'\"\n\n",
                        symbol
                    ));

                    if !suggestions.is_empty() {
                        output.push_str("did_you_mean:\n");
                        for (name, _) in suggestions {
                            output.push_str(&format!("  - {}\n", name));
                        }
                    }

                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }
            }
        }

        // Check file size - for large files (>10MB), require filter or default to summary
        let file_size = fs::metadata(&call_graph_path).map(|m| m.len()).unwrap_or(0);
        let is_large = file_size > 10 * 1024 * 1024; // 10MB threshold

        if is_large && request.module.is_none() && request.symbol.is_none() && !summary_only {
            // For large repos without filters, return summary with instructions
            let file = match fs::File::open(&call_graph_path) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to open call graph: {}",
                        e
                    ))]))
                }
            };

            let reader = BufReader::new(file);
            let mut total_edges = 0usize;
            let mut total_callees = 0usize;
            let mut top_callers: Vec<(String, usize)> = Vec::new();

            for line in reader.lines().filter_map(|l| l.ok()) {
                if line.starts_with("_type:")
                    || line.starts_with("schema_version:")
                    || line.starts_with("edges:")
                {
                    continue;
                }
                // Note: hash may contain colons (e.g., "locationHash:semanticHash"), so we find ": ["
                if let Some(bracket_pos) = line.find(": [") {
                    let caller = line[..bracket_pos].trim().to_string();
                    let rest = line[bracket_pos + 2..].trim();
                    if rest.starts_with('[') && rest.ends_with(']') {
                        let inner = &rest[1..rest.len() - 1];
                        let callee_count = if inner.is_empty() {
                            0
                        } else {
                            inner.matches(',').count() + 1
                        };
                        total_edges += 1;
                        total_callees += callee_count;

                        // Track top callers (by callee count)
                        if callee_count > 10 {
                            top_callers.push((caller, callee_count));
                        }
                    }
                }
            }

            // Sort and take top 20
            top_callers.sort_by(|a, b| b.1.cmp(&a.1));
            top_callers.truncate(20);

            let mut output = String::new();
            output.push_str("_type: call_graph_summary\n");
            output.push_str(&format!("file_size: {} MB\n", file_size / 1024 / 1024));
            output.push_str(&format!("total_callers: {}\n", total_edges));
            output.push_str(&format!("total_call_edges: {}\n", total_callees));
            output.push_str(&format!(
                "avg_callees_per_caller: {:.1}\n\n",
                total_callees as f64 / total_edges.max(1) as f64
            ));

            output.push_str("top_callers_by_fan_out:\n");
            for (caller, count) in &top_callers {
                output.push_str(&format!("  {} ({} callees)\n", caller, count));
            }

            output
                .push_str("\n⚠️ Large call graph detected. Use filters to query specific parts:\n");
            output.push_str("  - module: Filter by module name\n");
            output.push_str("  - symbol: Filter by symbol name\n");
            output.push_str("  - summary_only: true for statistics only\n");

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Stream through file with filtering (for filtered queries or small files)
        let file = match fs::File::open(&call_graph_path) {
            Ok(f) => f,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to open call graph: {}",
                    e
                ))]))
            }
        };

        let reader = BufReader::new(file);
        let mut edges: Vec<(String, Vec<String>)> = Vec::new();
        let mut total_edges = 0usize;
        let mut skipped = 0usize;

        for line in reader.lines().filter_map(|l| l.ok()) {
            // Skip header lines
            if line.starts_with("_type:")
                || line.starts_with("schema_version:")
                || line.starts_with("edges:")
            {
                continue;
            }

            // Parse edge
            // Note: hash may contain colons (e.g., "locationHash:semanticHash"), so we find ": ["
            if let Some(bracket_pos) = line.find(": [") {
                let caller = line[..bracket_pos].trim();
                let rest = line[bracket_pos + 2..].trim();

                if rest.starts_with('[') && rest.ends_with(']') {
                    total_edges += 1;

                    let inner = &rest[1..rest.len() - 1];
                    let callees: Vec<String> = inner
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .collect();

                    // Apply filters during streaming
                    let matches_filter = {
                        let mut matches = true;

                        if let Some(module) = &request.module {
                            let caller_matches = caller.contains(module.as_str());
                            let callee_matches =
                                callees.iter().any(|c| c.contains(module.as_str()));
                            if !caller_matches && !callee_matches {
                                matches = false;
                            }
                        }

                        // Use resolved symbol hashes for filtering (supports name-based lookup)
                        if matches {
                            if let Some(ref resolved_hashes) = resolved_symbol_hashes {
                                // Check if caller or any callee matches the resolved hashes
                                let caller_matches = resolved_hashes.contains(caller);
                                let callee_matches = callees.iter().any(|c| {
                                    // Handle both internal hashes and external calls
                                    resolved_hashes.contains(c.as_str())
                                });
                                if !caller_matches && !callee_matches {
                                    matches = false;
                                }
                            }
                        }

                        matches
                    };

                    if matches_filter {
                        // Handle offset
                        if skipped < offset {
                            skipped += 1;
                            continue;
                        }

                        // Collect edges (for both summary and non-summary mode)
                        if edges.len() < limit {
                            edges.push((caller.to_string(), callees));
                        }

                        // Early exit if we have enough for non-summary mode
                        // (summary mode will process all matching edges)
                    }
                }
            }
        }

        let filtered_count = skipped + edges.len();

        // Summary mode: return statistics only
        if summary_only {
            let output =
                format_call_graph_summary(&edges, total_edges, edges.len(), Some(&hash_to_name));
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Paginated output
        let output = format_call_graph_paginated(
            &edges,
            total_edges,
            filtered_count,
            offset,
            limit,
            Some(&hash_to_name),
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Query-Driven API Tools
    // ========================================================================

    #[tool(
        description = "Get source code for symbol(s) or line range. Three modes: (1) Batch: use hashes array (max 20, most efficient). (2) Single hash: use symbol_hash. (3) File+lines: use file_path with start_line/end_line. Returns code snippets with context lines."
    )]
    async fn get_source(
        &self,
        Parameters(request): Parameters<GetSourceRequest>,
    ) -> Result<CallToolResult, McpError> {
        let context = request.context.unwrap_or(5);

        // Batch mode: extract source for multiple symbols
        if let Some(ref hashes) = request.hashes {
            if !hashes.is_empty() {
                let repo_path = self.get_working_dir().await;
                let cache = match CacheDir::for_repo(&repo_path) {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to access cache: {}",
                            e
                        ))]))
                    }
                };

                let hashes: Vec<&str> = hashes.iter().take(20).map(|s| s.as_str()).collect();

                let mut output = String::new();
                output.push_str("_type: batch_source\n");
                output.push_str(&format!("requested: {}\n", hashes.len()));

                let mut found = 0;
                let mut not_found: Vec<&str> = Vec::new();

                for hash in &hashes {
                    let symbol_path = cache.symbol_path(hash);
                    if symbol_path.exists() {
                        if let Ok(symbol_content) = fs::read_to_string(&symbol_path) {
                            if let Some(source_snippet) =
                                extract_source_for_symbol(&cache, &symbol_content, context)
                            {
                                output.push_str(&format!("\n--- {} ---\n", hash));
                                output.push_str(&source_snippet);
                                found += 1;
                            } else {
                                not_found.push(hash);
                            }
                        } else {
                            not_found.push(hash);
                        }
                    } else {
                        not_found.push(hash);
                    }
                }

                output.push_str(&format!("\n_summary:\n  found: {}\n", found));
                if !not_found.is_empty() {
                    output.push_str(&format!("  not_found: {}\n", not_found.join(",")));
                }

                return Ok(CallToolResult::success(vec![Content::text(output)]));
            }
        }

        // Single hash mode
        if let Some(ref hash) = request.symbol_hash {
            let repo_path = self.get_working_dir().await;
            let cache = match CacheDir::for_repo(&repo_path) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to access cache: {}",
                        e
                    ))]))
                }
            };

            let symbol_path = cache.symbol_path(hash);
            if !symbol_path.exists() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol {} not found in index",
                    hash
                ))]));
            }

            let symbol_content = match fs::read_to_string(&symbol_path) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to read symbol: {}",
                        e
                    ))]))
                }
            };

            return match extract_source_for_symbol(&cache, &symbol_content, context) {
                Some(source) => Ok(CallToolResult::success(vec![Content::text(source)])),
                None => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Could not extract source for symbol {}",
                    hash
                ))])),
            };
        }

        // File+lines mode (requires file_path)
        let file_path = match &request.file_path {
            Some(p) => self.resolve_path(p).await,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Either file_path, symbol_hash, or hashes must be provided",
                )]))
            }
        };

        let (start_line, end_line) = match (request.start_line, request.end_line) {
            (Some(s), Some(e)) => (s, e),
            _ => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Both start_line and end_line are required for file+lines mode",
                )]))
            }
        };

        let source = match fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file {}: {}",
                    file_path.display(),
                    e
                ))]))
            }
        };

        let output = format_source_snippet(&file_path, &source, start_line, end_line, context);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Unified Search Handler (consolidates search_symbols, semantic_search, raw_search, search_and_get_symbols)
    // ========================================================================

    #[tool(
        description = "Unified search - runs BOTH symbol and semantic search by default (hybrid mode). Returns symbol matches AND conceptually related code in one call. Use mode='symbols' for exact name match only, mode='semantic' for BM25 conceptual search, or mode='raw' for regex patterns in comments/strings."
    )]
    async fn search(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Set working directory if path provided
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Change to repo directory for the CLI handler
        let original_dir = std::env::current_dir().ok();
        if let Err(e) = std::env::set_current_dir(&repo_path) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to change to directory {}: {}",
                repo_path.display(),
                e
            ))]));
        }

        // Ensure index exists for non-raw searches
        let mode = request.mode.as_deref().unwrap_or("");
        if mode != "raw" {
            if let Err(e) = self.ensure_index(&repo_path).await {
                // Restore directory before returning
                if let Some(ref dir) = original_dir {
                    let _ = std::env::set_current_dir(dir);
                }
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
        }

        // Build SearchArgs from the request
        let args = SearchArgs {
            query: request.query.clone(),
            symbols: mode == "symbols",
            related: mode == "semantic",
            raw: mode == "raw",
            kind: request.kind.clone(),
            module: request.module.clone(),
            risk: request.risk.clone(),
            include_source: request.include_source.unwrap_or(false),
            limit: request.limit.unwrap_or(20),
            file_types: request.file_types.as_ref().map(|v| v.join(",")),
            case_sensitive: !request.case_insensitive.unwrap_or(true),
            merge_threshold: request.merge_threshold.unwrap_or(3),
        };

        // Create command context (TOON format for MCP)
        let ctx = CommandContext::from_cli(OutputFormat::Toon, false, false);

        // Call the CLI handler
        let result = run_search(&args, &ctx);

        // Restore original directory
        if let Some(ref dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }

        match result {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {}",
                e
            ))])),
        }
    }

    /// Unified validate handler - auto-detects scope based on parameters.
    /// Scope priority: symbol_hash > file_path+line > file_path > module
    #[tool(
        description = "Unified quality audit - validates complexity, duplicates, and impact radius. Auto-detects scope: provide symbol_hash OR file_path+line for single symbol, file_path alone for all symbols in file, or module for module-level validation."
    )]
    async fn validate(
        &self,
        Parameters(request): Parameters<ValidateRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]))
            }
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No index found for {}. Run index tool first.",
                repo_path.display()
            ))]));
        }

        let threshold = request.duplicate_threshold.unwrap_or(0.85);

        // Scope detection (in order of priority):
        // 1. symbol_hash → single symbol validation
        // 2. file_path + line → single symbol at location
        // 3. file_path only → file-level validation
        // 4. module → module-level validation

        if let Some(ref hash) = request.symbol_hash {
            // Single symbol by hash
            let symbol_entry = match find_symbol_by_hash(&cache, hash) {
                Ok(entry) => entry,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };

            let result = validate_single_symbol(&cache, &symbol_entry, threshold);
            let mut output = format_validation_result(&result);

            if request.include_source.unwrap_or(false) {
                if let Some(source) =
                    get_symbol_source_snippet(&cache, &symbol_entry.file, &symbol_entry.lines, 2)
                {
                    output.push_str("\n__source__:\n");
                    output.push_str(&source);
                }
            }

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        if let Some(ref file_path) = request.file_path {
            if let Some(line) = request.line {
                // Single symbol by file + line
                let symbol_entry = match find_symbol_by_location(&cache, file_path, line) {
                    Ok(entry) => entry,
                    Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
                };

                let result = validate_single_symbol(&cache, &symbol_entry, threshold);
                let mut output = format_validation_result(&result);

                if request.include_source.unwrap_or(false) {
                    if let Some(source) = get_symbol_source_snippet(
                        &cache,
                        &symbol_entry.file,
                        &symbol_entry.lines,
                        2,
                    ) {
                        output.push_str("\n__source__:\n");
                        output.push_str(&source);
                    }
                }

                return Ok(CallToolResult::success(vec![Content::text(output)]));
            }

            // File-level validation (all symbols in file)
            let all_entries = match cache.load_all_symbol_entries() {
                Ok(e) => e,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to load symbol index: {}",
                        e
                    ))]))
                }
            };

            let mut entries: Vec<_> = all_entries
                .into_iter()
                .filter(|e| e.file.ends_with(file_path) || file_path.ends_with(&e.file))
                .collect();

            if let Some(ref kind) = request.kind {
                let normalized = normalize_kind(kind);
                entries.retain(|e| e.kind.eq_ignore_ascii_case(normalized));
            }

            if entries.is_empty() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "No symbols found in file: {}",
                    file_path
                ))]));
            }

            let results = validate_symbols_batch(&cache, &entries, threshold);
            let output = format_batch_validation_results(&results, file_path);

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        if let Some(ref module_name) = request.module {
            // Module-level validation
            let all_entries = match cache.load_all_symbol_entries() {
                Ok(e) => e,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to load symbol index: {}",
                        e
                    ))]))
                }
            };

            let mut entries: Vec<_> = all_entries
                .into_iter()
                .filter(|e| {
                    e.module.eq_ignore_ascii_case(module_name) || e.module.ends_with(module_name)
                })
                .collect();

            if let Some(ref kind) = request.kind {
                let normalized = normalize_kind(kind);
                entries.retain(|e| e.kind.eq_ignore_ascii_case(normalized));
            }

            let limit = request.limit.unwrap_or(100).min(500);
            entries.truncate(limit);

            if entries.is_empty() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "No symbols found in module: {}",
                    module_name
                ))]));
            }

            let results = validate_symbols_batch(&cache, &entries, threshold);
            let output =
                format_batch_validation_results(&results, &format!("module:{}", module_name));

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // No valid scope provided
        Ok(CallToolResult::error(vec![Content::text(
            "Must provide one of: symbol_hash, file_path (with optional line), or module",
        )]))
    }

    /// Unified index handler - smart refresh by default.
    /// Checks freshness first, only regenerates if stale. Use force=true to always rebuild.
    #[tool(
        description = "Unified index management - smart refresh by default (checks freshness, rebuilds only if stale). Use force=true to always regenerate. Returns index status and statistics."
    )]
    async fn index(
        &self,
        Parameters(request): Parameters<IndexRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Save original dir and change to repo path
        let original_dir = std::env::current_dir().ok();
        if let Err(e) = std::env::set_current_dir(&repo_path) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to change to directory {}: {}",
                repo_path.display(),
                e
            ))]));
        }

        let force = request.force.unwrap_or(false);
        let max_age = request.max_age.unwrap_or(3600);

        // Build IndexArgs for CLI handler
        let args = if force {
            // Force regeneration
            IndexArgs {
                operation: IndexOperation::Generate {
                    path: Some(repo_path.clone()),
                    force: true,
                    incremental: false,
                    max_depth: request.max_depth.unwrap_or(10),
                    extensions: request.extensions.clone().unwrap_or_default(),
                },
            }
        } else {
            // Smart refresh: check first, auto-refresh if stale
            IndexArgs {
                operation: IndexOperation::Check {
                    auto_refresh: true,
                    max_age,
                },
            }
        };

        let ctx = CommandContext::from_cli(OutputFormat::Toon, false, false);
        let result = run_index(&args, &ctx);

        // Restore original directory
        if let Some(ref dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }

        match result {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Index operation failed: {}",
                e
            ))])),
        }
    }

    /// Unified test handler - runs tests by default, use detect_only=true to only detect frameworks.
    #[tool(
        description = "Unified test runner - runs tests by default (auto-detects framework). Use detect_only=true to only detect available test frameworks without running."
    )]
    async fn test(
        &self,
        Parameters(request): Parameters<TestRequest>,
    ) -> Result<CallToolResult, McpError> {
        let project_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Save original dir and change to project path
        let original_dir = std::env::current_dir().ok();
        if let Err(e) = std::env::set_current_dir(&project_path) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to change to directory {}: {}",
                project_path.display(),
                e
            ))]));
        }

        // Build TestArgs for CLI handler
        let args = TestArgs {
            path: Some(project_path.clone()),
            detect: request.detect_only.unwrap_or(false),
            framework: request.framework.clone(),
            filter: request.filter.clone(),
            test_verbose: request.verbose.unwrap_or(false),
            timeout: request.timeout.unwrap_or(300),
        };

        let ctx = CommandContext::from_cli(OutputFormat::Toon, false, false);
        let result = run_test(&args, &ctx);

        // Restore original directory
        if let Some(ref dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }

        match result {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Test operation failed: {}",
                e
            ))])),
        }
    }

    // ========================================================================
    // Unified Security Handler (consolidates cve_scan, update_security_patterns, get_security_pattern_stats)
    // ========================================================================

    #[tool(
        description = "Unified security tool - scans for CVE vulnerability patterns by default. Use stats_only=true to get pattern statistics, update=true to update patterns from remote source. Matches function signatures against pre-compiled fingerprints from NVD/GHSA data."
    )]
    async fn security(
        &self,
        Parameters(request): Parameters<SecurityRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Change to the working directory for the operation
        let original_dir = std::env::current_dir().ok();
        if let Err(e) = std::env::set_current_dir(&repo_path) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to change to directory {}: {}",
                repo_path.display(),
                e
            ))]));
        }

        // Determine operation based on request parameters
        let operation = if request.stats_only.unwrap_or(false) {
            SecurityOperation::Stats
        } else if request.update.unwrap_or(false) {
            SecurityOperation::Update {
                url: request.url.clone(),
                file: request.file_path.clone().map(std::path::PathBuf::from),
                force: request.force.unwrap_or(false),
            }
        } else {
            SecurityOperation::Scan {
                module: request.module.clone(),
                severity: request.severity_filter.clone(),
                cwe: request.cwe_filter.clone(),
                min_similarity: request.min_similarity.unwrap_or(0.75),
                limit: request.limit.unwrap_or(100),
            }
        };

        let args = SecurityArgs { operation };

        let ctx = CommandContext::from_cli(OutputFormat::Toon, false, false);
        let result = run_security(&args, &ctx);

        // Restore original directory
        if let Some(ref dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }

        match result {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Security operation failed: {}",
                e
            ))])),
        }
    }

    // ========================================================================
    // Layer Management Tools (SEM-98, SEM-99, SEM-101, SEM-102, SEM-104)
    // ========================================================================

    #[tool(
        description = "Get server status including mode, features, and optionally detailed layer status. Combines server mode info with layer details when include_layers is true."
    )]
    async fn server_status(
        &self,
        Parameters(request): Parameters<ServerStatusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let include_layers = request.include_layers.unwrap_or(false);
        let mut output = String::new();

        output.push_str("_type: server_status\n");
        output.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
        output.push_str(&format!(
            "persistent_mode: {}\n",
            self.server_state.is_some()
        ));

        if let Some(state) = &self.server_state {
            output.push_str("features:\n");
            output.push_str("  - file_watcher: enabled (Working layer auto-update)\n");
            output.push_str("  - git_poller: enabled (Base/Branch layer polling)\n");
            output.push_str("  - thread_safe: enabled (concurrent read access)\n");

            let stats = state.stats();
            output.push_str("\nindex_stats:\n");
            output.push_str(&format!("  base_symbols: {}\n", stats.base_symbols));
            output.push_str(&format!("  branch_symbols: {}\n", stats.branch_symbols));
            output.push_str(&format!("  working_symbols: {}\n", stats.working_symbols));
            output.push_str(&format!("  ai_symbols: {}\n", stats.ai_symbols));

            // Include detailed layer status if requested
            if include_layers {
                let status = state.status();
                output.push_str(&format!("\nrepo_root: {}\n", status.repo_root.display()));
                output.push_str(&format!("is_running: {}\n", status.is_running));
                output.push_str(&format!("uptime_secs: {}\n", status.uptime.as_secs()));
                output.push_str("\nlayers:\n");

                for layer_status in &status.layers {
                    output.push_str(&format!("  - kind: {:?}\n", layer_status.kind));
                    output.push_str(&format!("    is_stale: {}\n", layer_status.is_stale));
                    output.push_str(&format!(
                        "    symbol_count: {}\n",
                        layer_status.symbol_count
                    ));
                    output.push_str(&format!("    strategy: {:?}\n", layer_status.strategy));
                }
            }
        } else {
            output.push_str("features:\n");
            output.push_str("  - file_watcher: disabled\n");
            output.push_str("  - git_poller: disabled\n");
            output.push_str("  - thread_safe: n/a\n");
            output.push_str("\nhint: Start with --persistent to enable live layer updates\n");

            if include_layers {
                output.push_str("\nnote: Layer details not available (requires persistent mode)\n");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Duplicate Detection Tools
    // ========================================================================

    #[tool(
        description = "Unified duplicate detection. **Codebase scan** (default): Find all duplicate clusters for health audits. **Single symbol check**: Pass symbol_hash to check one symbol before writing new code or during refactoring.

Output is token-optimized: duplicates are grouped by module with counts and similarity ranges (e.g., 'Nop.Web.Factories: 12 (91-98%) [near]') plus top 3 individual matches per cluster. Use limit=10-20 for initial exploration, higher for comprehensive scans. Results are paginated - use offset to get more."
    )]
    async fn find_duplicates(
        &self,
        Parameters(request): Parameters<FindDuplicatesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]))
            }
        };

        // Load signatures from index
        let signatures = match load_signatures(&cache) {
            Ok(sigs) => sigs,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to load signature index: {}. Run generate_index first.",
                    e
                ))]))
            }
        };

        if signatures.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "_type: duplicate_results\nclusters: 0\nmessage: No function signatures found in index.\n"
            )]));
        }

        let threshold = request.threshold.unwrap_or(0.90);

        // Single symbol mode: check specific symbol for duplicates
        if let Some(symbol_hash) = &request.symbol_hash {
            let target = match signatures.iter().find(|s| &s.symbol_hash == symbol_hash) {
                Some(sig) => sig,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Symbol {} not found in signature index.",
                        symbol_hash
                    ))]))
                }
            };

            let detector = DuplicateDetector::new(threshold);
            let matches = detector.find_duplicates(target, &signatures);
            let output = format_duplicate_matches(&target.name, &target.file, &matches, threshold);
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Codebase scan mode: find all duplicate clusters
        let exclude_boilerplate = request.exclude_boilerplate.unwrap_or(true);
        let min_lines = request.min_lines.unwrap_or(3) as usize;
        let limit = request.limit.unwrap_or(50).min(200) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let sort_by = request.sort_by.as_deref().unwrap_or("similarity");

        let detector =
            DuplicateDetector::new(threshold).with_boilerplate_exclusion(exclude_boilerplate);

        // Filter by module and min_lines
        let filtered_sigs: Vec<_> = signatures
            .iter()
            .filter(|s| {
                // Apply module filter
                if let Some(module) = &request.module {
                    if !s.file.contains(module) {
                        return false;
                    }
                }
                // Apply min_lines filter
                s.line_count >= min_lines
            })
            .cloned()
            .collect();

        // Find all clusters
        let mut clusters = detector.find_all_clusters(&filtered_sigs);
        let total_clusters = clusters.len();

        // Sort clusters by specified criteria
        match sort_by {
            "size" => {
                // Sort by primary function size (lines), largest first
                clusters.sort_by(|a, b| {
                    let a_size = a.primary.end_line.saturating_sub(a.primary.start_line);
                    let b_size = b.primary.end_line.saturating_sub(b.primary.start_line);
                    b_size.cmp(&a_size)
                });
            }
            "count" => {
                // Sort by number of duplicates, most first
                clusters.sort_by(|a, b| b.duplicates.len().cmp(&a.duplicates.len()));
            }
            _ => {
                // Default: sort by highest similarity in cluster
                clusters.sort_by(|a, b| {
                    let a_max = a
                        .duplicates
                        .iter()
                        .map(|d| d.similarity)
                        .fold(0.0_f64, f64::max);
                    let b_max = b
                        .duplicates
                        .iter()
                        .map(|d| d.similarity)
                        .fold(0.0_f64, f64::max);
                    b_max
                        .partial_cmp(&a_max)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // Apply pagination
        let paginated: Vec<_> = clusters.into_iter().skip(offset).take(limit).collect();

        let output = format_duplicate_clusters_paginated(
            &paginated,
            threshold,
            total_clusters,
            offset,
            limit,
            sort_by,
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Commit Preparation Tools
    // ========================================================================

    #[tool(
        description = "Prepare information for writing a commit message. Gathers git context, analyzes staged and unstaged changes semantically, and returns a compact summary with optional complexity metrics. **Use before committing** to understand what you're about to commit. This tool NEVER commits - it only provides information."
    )]
    async fn prep_commit(
        &self,
        Parameters(request): Parameters<PrepCommitRequest>,
    ) -> Result<CallToolResult, McpError> {
        use std::process::Command;

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Verify it's a git repo
        if !crate::git::is_git_repo(Some(&repo_path)) {
            return Ok(CallToolResult::error(vec![Content::text(
                "Not a git repository",
            )]));
        }

        // Extract options with defaults
        let include_complexity = request.include_complexity.unwrap_or(false);
        let include_all_metrics = request.include_all_metrics.unwrap_or(false);
        let staged_only = request.staged_only.unwrap_or(false);
        let auto_refresh = request.auto_refresh_index.unwrap_or(true);
        let show_diff_stats = request.show_diff_stats.unwrap_or(true);

        // Auto-refresh index if requested
        if auto_refresh {
            if let Ok(cache) = CacheDir::for_repo(&repo_path) {
                if cache.exists() {
                    let staleness = check_cache_staleness_detailed(&cache, 3600);
                    if staleness.is_stale {
                        // Silently refresh the index
                        let _ = generate_index_internal(&repo_path, 10, &[]);
                    }
                }
            }
        }

        // Get git context
        let branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&repo_path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let commit_info = Command::new("git")
            .args(["log", "-1", "--format=%h|%s"])
            .current_dir(&repo_path)
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

        let git_context = GitContext {
            branch,
            remote,
            last_commit_hash,
            last_commit_message,
        };

        // Get staged changes
        let staged_changes = match crate::git::get_staged_changes(Some(&repo_path)) {
            Ok(files) => files,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to get staged changes: {}",
                    e
                ))]))
            }
        };

        // Get unstaged changes (unless staged_only)
        let unstaged_changes = if staged_only {
            Vec::new()
        } else {
            match crate::git::get_unstaged_changes(Some(&repo_path)) {
                Ok(files) => files,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to get unstaged changes: {}",
                        e
                    ))]))
                }
            }
        };

        // If no changes at all, return early
        if staged_changes.is_empty() && unstaged_changes.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "_type: prep_commit\n_note: No changes to commit.\n\nstaged_changes: (none)\nunstaged_changes: (none)\n"
            )]));
        }

        // Helper to analyze a list of changed files
        let analyze_files_list = |changes: &[crate::git::ChangedFile]| -> Vec<AnalyzedFile> {
            changes
                .iter()
                .map(|changed_file| {
                    let file_path = repo_path.join(&changed_file.path);
                    let change_type_str = format!("{:?}", changed_file.change_type);

                    // Get diff stats if requested
                    let diff_stats = if show_diff_stats {
                        // Get diff stats for this specific file
                        let stat_output = Command::new("git")
                            .args(["diff", "--numstat", "--cached", "--", &changed_file.path])
                            .current_dir(&repo_path)
                            .output()
                            .ok()
                            .filter(|o| o.status.success())
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

                        // If no cached stat, try unstaged
                        let stat_output = stat_output.or_else(|| {
                            Command::new("git")
                                .args(["diff", "--numstat", "--", &changed_file.path])
                                .current_dir(&repo_path)
                                .output()
                                .ok()
                                .filter(|o| o.status.success())
                                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        });

                        stat_output.and_then(|stat| {
                            let parts: Vec<&str> = stat.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let insertions = parts[0].parse().unwrap_or(0);
                                let deletions = parts[1].parse().unwrap_or(0);
                                Some(FileDiffStats {
                                    insertions,
                                    deletions,
                                })
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    };

                    // Skip deleted files - they have no symbols
                    if matches!(changed_file.change_type, crate::git::ChangeType::Deleted) {
                        return AnalyzedFile {
                            path: changed_file.path.clone(),
                            change_type: change_type_str,
                            diff_stats,
                            symbols: Vec::new(),
                            error: Some("file deleted".to_string()),
                        };
                    }

                    // Check if file exists
                    if !file_path.exists() {
                        return AnalyzedFile {
                            path: changed_file.path.clone(),
                            change_type: change_type_str,
                            diff_stats,
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
                                diff_stats,
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
                                diff_stats,
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
                                diff_stats,
                                symbols: Vec::new(),
                                error: Some(format!("parse error: {}", e)),
                            };
                        }
                    };

                    // Create a single symbol entry for the file-level summary
                    let lines = format!(
                        "{}-{}",
                        summary.start_line.unwrap_or(1),
                        summary.end_line.unwrap_or(1)
                    );

                    let (
                        cognitive,
                        cyclomatic,
                        max_nesting,
                        fan_out,
                        loc,
                        state_mutations,
                        io_operations,
                    ) = if include_complexity || include_all_metrics {
                        // Pass 0 for fan_in since we don't have call graph data
                        let complexity =
                            crate::analysis::symbol_complexity_from_summary(&summary, 0);
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
                        // Use file stem as fallback name
                        std::path::Path::new(&changed_file.path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string()
                    });
                    let symbol_kind = summary
                        .symbol_kind
                        .map(|k| format!("{:?}", k).to_lowercase())
                        .unwrap_or_else(|| "file".to_string());

                    let symbols = vec![SymbolMetrics {
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
                        diff_stats,
                        symbols,
                        error: None,
                    }
                })
                .collect()
        };

        // Analyze staged and unstaged files
        let staged_files = analyze_files_list(&staged_changes);
        let unstaged_files = analyze_files_list(&unstaged_changes);

        // Format output
        let output = format_prep_commit(
            &git_context,
            &staged_files,
            &unstaged_files,
            include_complexity,
            include_all_metrics,
            show_diff_stats,
        );

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Get symbols from a file or module (mutually exclusive). Use file_path for file-centric view, or module for module-centric view. Returns lightweight index entries with optional source snippets."
    )]
    async fn get_file(
        &self,
        Parameters(request): Parameters<GetFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Validate mutually exclusive parameters
        match (&request.file_path, &request.module) {
            (Some(_), Some(_)) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Error: file_path and module are mutually exclusive. Provide one or the other.",
                )]));
            }
            (None, None) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Error: Must provide either file_path or module parameter.",
                )]));
            }
            _ => {}
        }

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Module mode: list symbols in a module
        if let Some(module) = &request.module {
            let freshness = match self.ensure_index(&repo_path).await {
                Ok(r) => r,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };
            let cache = freshness.cache;

            let limit = request.limit.unwrap_or(50).min(200);

            let results = match cache.list_module_symbols(
                module,
                request.kind.as_deref(),
                request.risk.as_deref(),
                limit,
            ) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "List failed: {}",
                        e
                    ))]))
                }
            };

            let output = format_module_symbols(module, &results, &cache);
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // File mode: get symbols in a specific file
        let file_path = request.file_path.as_ref().unwrap();

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]))
            }
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.",
                repo_path.display()
            ))]));
        }

        let include_source = request.include_source.unwrap_or(false);
        let context = request.context.unwrap_or(2);

        // Normalize the file path for matching
        let target_file = file_path.trim_start_matches("./");

        // Search the symbol index for symbols in this file
        let symbols: Vec<_> = match cache.load_all_symbol_entries() {
            Ok(all) => all
                .into_iter()
                .filter(|e| {
                    let entry_file = e.file.trim_start_matches("./");
                    entry_file == target_file
                        || entry_file.ends_with(target_file)
                        || target_file.ends_with(entry_file)
                })
                .filter(|e| {
                    request
                        .kind
                        .as_ref()
                        .map_or(true, |k| e.kind == normalize_kind(k))
                })
                .collect(),
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to load symbol index: {}",
                    e
                ))]))
            }
        };

        if symbols.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "_type: file_symbols\nfile: \"{}\"\nshowing: 0\nsymbols: (none)\nhint: File may not be indexed or path doesn't match.\n",
                file_path
            ))]));
        }

        let mut output = String::new();
        output.push_str("_type: file_symbols\n");
        output.push_str(&format!("file: \"{}\"\n", file_path));
        output.push_str(&format!("showing: {}\n", symbols.len()));
        output.push_str(&format!(
            "symbols[{}]{{name,hash,kind,lines,risk}}:\n",
            symbols.len()
        ));

        for entry in &symbols {
            output.push_str(&format!(
                "  {},{},{},{},{}\n",
                entry.symbol, entry.hash, entry.kind, entry.lines, entry.risk
            ));
        }

        // Include source for each symbol if requested
        if include_source && !symbols.is_empty() {
            output.push_str("\n__sources__:\n");
            for entry in &symbols {
                if let Some(source) =
                    get_symbol_source_snippet(&cache, &entry.file, &entry.lines, context)
                {
                    output.push_str(&format!("\n--- {} ({}) ---\n", entry.symbol, entry.lines));
                    output.push_str(&source);
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "**Use before modifying existing code** to understand impact radius. Answers 'what functions call this symbol?' Shows what will break if you change this function. Returns direct callers and optionally transitive callers (up to depth 3)."
    )]
    async fn get_callers(
        &self,
        Parameters(request): Parameters<GetCallersRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]))
            }
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.",
                repo_path.display()
            ))]));
        }

        let depth = request.depth.unwrap_or(1).min(3);
        let limit = request.limit.unwrap_or(20).min(50);
        let include_source = request.include_source.unwrap_or(false);

        // Load call graph
        let call_graph_path = cache.call_graph_path();
        if !call_graph_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Call graph not found. Run generate_index to create it.",
            )]));
        }

        let content = match fs::read_to_string(&call_graph_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read call graph: {}",
                    e
                ))]))
            }
        };

        // Build reverse call graph (callee -> callers)
        let mut reverse_graph: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut symbol_names: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for line in content.lines() {
            if line.starts_with("_type:")
                || line.starts_with("schema_version:")
                || line.starts_with("edges:")
            {
                continue;
            }
            // Find ": [" pattern - the colon before the call list
            // Hash format is "file_hash:semantic_hash", so we can't use simple find(':')
            if let Some(bracket_pos) = line.find(": [") {
                let caller = line[..bracket_pos].trim().to_string();
                let rest = line[bracket_pos + 2..].trim();
                if rest.starts_with('[') && rest.ends_with(']') {
                    let inner = &rest[1..rest.len() - 1];
                    for callee in inner.split(',').filter(|s| !s.is_empty()) {
                        let callee = callee.trim().trim_matches('"').to_string();
                        // Skip external calls
                        if !callee.starts_with("ext:") {
                            reverse_graph
                                .entry(callee.clone())
                                .or_default()
                                .push(caller.clone());
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
        let target_hash = &request.symbol_hash;
        let target_name = symbol_names
            .get(target_hash)
            .cloned()
            .unwrap_or_else(|| target_hash.clone());

        let mut all_callers: Vec<(String, String, usize)> = Vec::new(); // (hash, name, depth)
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut current_level: Vec<String> = vec![target_hash.clone()];

        for current_depth in 1..=depth {
            let mut next_level: Vec<String> = Vec::new();

            for hash in &current_level {
                if let Some(callers) = reverse_graph.get(hash) {
                    for caller_hash in callers {
                        if !visited.contains(caller_hash) && all_callers.len() < limit {
                            visited.insert(caller_hash.clone());
                            let caller_name = symbol_names
                                .get(caller_hash)
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

        // Format output
        let mut output = String::new();
        output.push_str("_type: callers\n");
        output.push_str(&format!("target: {} ({})\n", target_name, target_hash));
        output.push_str(&format!("depth: {}\n", depth));
        output.push_str(&format!("total_callers: {}\n", all_callers.len()));

        if all_callers.is_empty() {
            output.push_str("callers: (none - this may be an entry point or unused)\n");
        } else {
            output.push_str(&format!(
                "callers[{}]{{name,hash,depth}}:\n",
                all_callers.len()
            ));
            for (hash, name, d) in &all_callers {
                output.push_str(&format!("  {},{},{}\n", name, hash, d));
            }

            // Include source snippets if requested
            if include_source {
                output.push_str("\n__caller_sources__:\n");
                for (hash, name, _) in all_callers.iter().take(5) {
                    // Get symbol info to find file/lines
                    let symbol_path = cache.symbol_path(hash);
                    if symbol_path.exists() {
                        if let Ok(content) = fs::read_to_string(&symbol_path) {
                            let mut file: Option<String> = None;
                            let mut lines: Option<String> = None;
                            for line in content.lines() {
                                let trimmed = line.trim();
                                if trimmed.starts_with("file:") {
                                    file = Some(
                                        trimmed
                                            .trim_start_matches("file:")
                                            .trim()
                                            .trim_matches('"')
                                            .to_string(),
                                    );
                                } else if trimmed.starts_with("lines:") {
                                    lines = Some(
                                        trimmed
                                            .trim_start_matches("lines:")
                                            .trim()
                                            .trim_matches('"')
                                            .to_string(),
                                    );
                                }
                            }
                            if let (Some(f), Some(l)) = (file, lines) {
                                if let Some(source) = get_symbol_source_snippet(&cache, &f, &l, 2) {
                                    output.push_str(&format!("\n--- {} ({}) ---\n", name, l));
                                    output.push_str(&source);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

/// Get source snippet for a symbol given file and line range
fn get_symbol_source_snippet(
    cache: &CacheDir,
    file: &str,
    lines: &str,
    context: usize,
) -> Option<String> {
    let (start_line, end_line) = if let Some((s, e)) = lines.split_once('-') {
        (s.parse::<usize>().ok()?, e.parse::<usize>().ok()?)
    } else {
        return None;
    };

    let full_path = cache.repo_root.join(file);
    let source = fs::read_to_string(&full_path).ok()?;

    Some(format_source_snippet(
        &full_path, &source, start_line, end_line, context,
    ))
}

/// Format test results as compact TOON output
#[allow(dead_code)]
fn format_test_results(results: &test_runner::TestResults) -> String {
    let mut output = String::new();
    output.push_str("_type: test_results\n");
    output.push_str(&format!("framework: {}\n", results.framework.as_str()));
    output.push_str(&format!("success: {}\n", results.success));
    output.push_str(&format!("passed: {}\n", results.passed));
    output.push_str(&format!("failed: {}\n", results.failed));
    output.push_str(&format!("skipped: {}\n", results.skipped));
    output.push_str(&format!("total: {}\n", results.total));
    output.push_str(&format!("duration_ms: {}\n", results.duration_ms));

    if let Some(code) = results.exit_code {
        output.push_str(&format!("exit_code: {}\n", code));
    }

    if !results.failures.is_empty() {
        output.push_str(&format!("\nfailures[{}]:\n", results.failures.len()));
        for failure in &results.failures {
            output.push_str(&format!("  - test: {}\n", failure.name));
            if let Some(ref file) = failure.file {
                output.push_str(&format!("    file: {}\n", file));
            }
            if let Some(line) = failure.line {
                output.push_str(&format!("    line: {}\n", line));
            }
            if !failure.message.is_empty() {
                // Truncate long messages
                let msg = if failure.message.len() > 200 {
                    format!("{}...", truncate_to_char_boundary(&failure.message, 200))
                } else {
                    failure.message.clone()
                };
                output.push_str(&format!("    message: {}\n", msg.replace('\n', "\\n")));
            }
        }
    }

    // Include truncated stdout/stderr for debugging
    if !results.stdout.is_empty() {
        let stdout = if results.stdout.len() > 500 {
            format!(
                "{}...(truncated)",
                truncate_to_char_boundary(&results.stdout, 500)
            )
        } else {
            results.stdout.clone()
        };
        output.push_str(&format!("\n__stdout__:\n{}\n", stdout));
    }

    if !results.stderr.is_empty() && results.stderr.len() < 500 {
        output.push_str(&format!("\n__stderr__:\n{}\n", results.stderr));
    }

    output
}

#[tool_handler]
impl ServerHandler for McpDiffServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "semfora-engine".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Semfora Engine".to_string()),
                website_url: None,
                icons: None,
            },
            instructions: Some(MCP_INSTRUCTIONS.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let server = McpDiffServer::new();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "semfora-engine");
    }
}
