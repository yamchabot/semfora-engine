//! MCP Server for semfora-engine
//!
//! This module provides an MCP (Model Context Protocol) server that exposes
//! the semantic code analysis capabilities of semfora-engine as tools that can be
//! called by AI assistants like Claude.

pub mod formatting;
pub mod helpers;
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
        AnalyzeArgs, CommitArgs, IndexArgs, IndexOperation, OutputFormat, SearchArgs, SecurityArgs,
        SecurityOperation, TestArgs, ValidateArgs,
    },
    commands::{run_analyze, run_commit, run_duplicates, run_file_symbols, run_get_callgraph, run_get_callers, run_get_source, run_get_symbol, run_index, run_overview, run_search, run_security, run_test, run_validate, CommandContext},
    server::ServerState,
    test_runner::{self},
    utils::truncate_to_char_boundary,
    CacheDir,
};

// Re-export types for external use
use formatting::{format_module_symbols, get_supported_languages};
use helpers::{
    check_cache_staleness_detailed,
    ensure_fresh_index,
    format_freshness_note,
    generate_index_internal,
    FreshnessResult,
};
pub use types::*;
// Match this to the active module above:
use instructions_fast::MCP_INSTRUCTIONS;

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
        use crate::git::{get_current_branch, get_last_commit, get_remote_url};

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

        // Git branch (DEDUP-104: uses shared git module)
        let branch = get_current_branch(Some(&repo_path)).unwrap_or_else(|_| "unknown".to_string());
        output.push_str(&format!("branch: \"{}\"\n", branch));

        // Git remote (DEDUP-104: uses shared git module)
        let remote = get_remote_url(None, Some(&repo_path)).unwrap_or_else(|| "none".to_string());
        output.push_str(&format!("remote: \"{}\"\n", remote));

        // Last commit info (DEDUP-104: uses shared git module)
        if let Some(commit) = get_last_commit(Some(&repo_path)) {
            output.push_str("last_commit:\n");
            output.push_str(&format!("  hash: \"{}\"\n", commit.short_sha));
            // Truncate message to 60 chars
            let msg = if commit.subject.len() > 60 {
                format!("{}...", &commit.subject[..57])
            } else {
                commit.subject.clone()
            };
            output.push_str(&format!("  message: \"{}\"\n", msg));
            output.push_str(&format!("  author: \"{}\"\n", commit.author));
            // Simplify date to just the date part
            let date = commit.date.split('T').next().unwrap_or(&commit.date);
            output.push_str(&format!("  date: \"{}\"\n", date));
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

        // Path-based analysis - delegate to CLI handler (DEDUP-301)
        let path = match &request.path {
            Some(p) => p,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Error: Must provide either 'path' or 'module' parameter.",
                )]))
            }
        };

        let resolved_path = self.resolve_path(path).await;

        // Build CLI args from MCP request
        let args = AnalyzeArgs {
            path: Some(resolved_path),
            diff: None,
            uncommitted: false,
            commit: None,
            all_commits: false,
            base: None,
            max_depth: request.max_depth.unwrap_or(10),
            extensions: request.extensions.clone().unwrap_or_default(),
            allow_tests: false,
            summary_only: request.summary_only.unwrap_or(false),
            start_line: request.start_line,
            end_line: request.end_line,
            output_mode: request.output_mode.clone().unwrap_or_else(|| "full".to_string()),
            target_ref: None,
            limit: None,
            offset: None,
            shard: false,
            incremental: false,
            analyze_tokens: None,
            compare_compact: false,
            print_ast: false,
        };

        // Select output format based on MCP request
        let format = match request.format.as_deref() {
            Some("json") => OutputFormat::Json,
            _ => OutputFormat::Toon,
        };

        let ctx = CommandContext {
            format,
            verbose: false,
            progress: false,
        };

        // Call CLI handler
        match run_analyze(&ctx, &args) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Analysis failed: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "**Use for code reviews** - analyzes changes between git branches or commits semantically. Shows new/modified symbols, changed dependencies, and risk assessment for each file. Use target_ref='WORKING' to review uncommitted changes before committing. Supports pagination (limit/offset) for large diffs and summary_only mode for quick overview."
    )]
    async fn analyze_diff(
        &self,
        Parameters(request): Parameters<AnalyzeDiffRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Resolve working directory
        let working_dir = match &request.working_dir {
            Some(wd) => self.resolve_path(wd).await,
            None => self.get_working_dir().await,
        };

        // Validate git repo before delegating
        if !crate::git::is_git_repo(Some(&working_dir)) {
            return Ok(CallToolResult::error(vec![Content::text(
                "Not a git repository",
            )]));
        }

        // Build CLI args from MCP request (DEDUP-302)
        let args = AnalyzeArgs {
            path: Some(working_dir),
            diff: Some(request.base_ref.clone()),
            uncommitted: false,
            commit: None,
            all_commits: false,
            base: None,
            max_depth: 10,
            extensions: vec![],
            allow_tests: false,
            summary_only: request.summary_only.unwrap_or(false),
            start_line: None,
            end_line: None,
            output_mode: "full".to_string(),
            target_ref: request.target_ref.clone(),
            limit: request.limit,
            offset: request.offset,
            shard: false,
            incremental: false,
            analyze_tokens: None,
            compare_compact: false,
            print_ast: false,
        };

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        // Delegate to CLI handler
        match run_analyze(&ctx, &args) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Diff analysis failed: {}",
                e
            ))])),
        }
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

        // Build output with optional freshness notice
        let mut output = String::new();
        if let Some(note) = format_freshness_note(&freshness) {
            output.push_str(&note);
            output.push_str("\n\n");
        }

        // Delegate to CLI handler (DEDUP-303)
        // include_modules=true when max_modules > 0
        let include_modules = max_modules > 0;
        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_overview(Some(&repo_path), include_modules, max_modules, exclude_test_dirs, include_git_context, &ctx) {
            Ok(overview_output) => {
                output.push_str(&overview_output);
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get overview: {}",
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
        // DEDUP-306: Delegate to CLI run_get_symbol handler

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Convert batch hashes array to comma-separated string (CLI format)
        let hash_str: Option<String> = if let Some(ref hashes) = request.hashes {
            if !hashes.is_empty() {
                Some(hashes.iter().take(20).cloned().collect::<Vec<_>>().join(","))
            } else {
                request.symbol_hash.clone()
            }
        } else {
            request.symbol_hash.clone()
        };

        let include_source = request.include_source.unwrap_or(false);
        let context = request.context.unwrap_or(3);

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_get_symbol(
            Some(&repo_path),
            hash_str.as_deref(),
            request.file.as_deref(),
            request.line,
            include_source,
            context,
            &ctx,
        ) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get symbol: {}",
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
        // DEDUP-306: Delegate to CLI run_get_callgraph handler

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // For SQLite export, use output_path if provided
        let export_arg: Option<String> = if let Some(export_format) = &request.export {
            if export_format == "sqlite" {
                // If output_path provided, pass it; otherwise CLI uses default
                request.output_path.clone().or(Some("sqlite".to_string()))
            } else {
                Some(export_format.clone())
            }
        } else {
            None
        };

        let limit = request.limit.unwrap_or(500).min(2000) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let stats_only = request.summary_only.unwrap_or(false);
        let include_escape_refs = request.include_escape_refs.unwrap_or(false);

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_get_callgraph(
            Some(&repo_path),
            request.module.as_deref(),
            request.symbol.as_deref(),
            export_arg.as_deref(),
            stats_only,
            limit,
            offset,
            include_escape_refs,
            &ctx,
        ) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get call graph: {}",
                e
            ))])),
        }
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
        // DEDUP-306: Delegate to CLI run_get_source handler

        let repo_path = self.get_working_dir().await;

        // Convert batch hashes array to comma-separated string (CLI format)
        let hash_str: Option<String> = if let Some(ref hashes) = request.hashes {
            if !hashes.is_empty() {
                Some(hashes.iter().take(20).cloned().collect::<Vec<_>>().join(","))
            } else {
                request.symbol_hash.clone()
            }
        } else {
            request.symbol_hash.clone()
        };

        // For file mode, use file path as string
        let file_str: Option<String> = request.file_path.clone();

        let context = request.context.unwrap_or(5);

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_get_source(
            Some(&repo_path),
            file_str.as_deref(),
            request.start_line,
            request.end_line,
            hash_str.as_deref(),
            context,
            &ctx,
        ) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get source: {}",
                e
            ))])),
        }
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
            include_escape_refs: request.include_escape_refs.unwrap_or(false),
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
        // Resolve working directory
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // Build CLI args from MCP request (DEDUP-304)
        let args = ValidateArgs {
            target: None,
            path: Some(repo_path),
            symbol_hash: request.symbol_hash.clone(),
            file_path: request.file_path.clone(),
            line: request.line,
            module: request.module.clone(),
            include_source: request.include_source.unwrap_or(false),
            duplicates: false,
            threshold: request.duplicate_threshold.unwrap_or(0.85),
            include_boilerplate: false,
            kind: request.kind.clone(),
            limit: request.limit.unwrap_or(100),
            offset: 0,
            min_lines: 3,
            sort_by: "similarity".to_string(),
        };

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        // Delegate to CLI handler
        match run_validate(&args, &ctx) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Validation failed: {}",
                e
            ))])),
        }
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
        // DEDUP-307: Delegate to CLI run_duplicates handler

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let threshold = request.threshold.unwrap_or(0.90);
        let exclude_boilerplate = request.exclude_boilerplate.unwrap_or(true);
        let min_lines = request.min_lines.unwrap_or(3) as usize;
        let limit = request.limit.unwrap_or(50).min(200) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let sort_by = request.sort_by.as_deref().unwrap_or("similarity");

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_duplicates(
            Some(&repo_path),
            request.symbol_hash.as_deref(),
            threshold,
            request.module.as_deref(),
            exclude_boilerplate,
            min_lines,
            sort_by,
            limit,
            offset,
            &ctx,
        ) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to find duplicates: {}",
                e
            ))])),
        }
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
        // DEDUP-305: Delegate to CLI run_commit handler

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        // MCP-specific: Auto-refresh index if requested (async operation)
        let auto_refresh = request.auto_refresh_index.unwrap_or(true);
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

        // Build CLI args from MCP request
        let args = CommitArgs {
            path: Some(repo_path),
            staged: request.staged_only.unwrap_or(false),
            metrics: request.include_complexity.unwrap_or(false),
            all_metrics: request.include_all_metrics.unwrap_or(false),
            no_auto_refresh: true, // Already handled above
            no_diff_stats: !request.show_diff_stats.unwrap_or(true),
        };

        // Create command context (MCP uses TOON format)
        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        // Delegate to CLI handler
        match run_commit(&args, &ctx) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Commit preparation failed: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Get symbols from a file or module (mutually exclusive). Use file_path for file-centric view, or module for module-centric view. Returns lightweight index entries with optional source snippets."
    )]
    async fn get_file(
        &self,
        Parameters(request): Parameters<GetFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        // DEDUP-306: Delegate file mode to CLI run_file_symbols handler
        // Module mode uses MCP-specific formatting helpers

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

        // Module mode: list symbols in a module (uses MCP-specific formatting)
        if let Some(module) = &request.module {
            let freshness = match self.ensure_index(&repo_path).await {
                Ok(r) => r,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };
            let cache = freshness.cache;

            let limit = request.limit.unwrap_or(50).min(200);
            let include_escape_refs = request.include_escape_refs.unwrap_or(false);

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

            let results: Vec<_> = results
                .into_iter()
                .filter(|entry| include_escape_refs || !entry.is_escape_local)
                .collect();
            let output = format_module_symbols(module, &results, &cache);
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // File mode: delegate to CLI run_file_symbols handler
        let file_path = request.file_path.as_ref().unwrap();
        let include_source = request.include_source.unwrap_or(false);
        let context = request.context.unwrap_or(2);
        let include_escape_refs = request.include_escape_refs.unwrap_or(false);

        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        match run_file_symbols(
            Some(&repo_path),
            file_path,
            include_source,
            request.kind.as_deref(),
            request.risk.as_deref(),
            context,
            include_escape_refs,
            &ctx,
        ) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get file symbols: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "**Use before modifying existing code** to understand impact radius. Answers 'what functions call this symbol?' Shows what will break if you change this function. Returns direct callers and optionally transitive callers (up to depth 3)."
    )]
    async fn get_callers(
        &self,
        Parameters(request): Parameters<GetCallersRequest>,
    ) -> Result<CallToolResult, McpError> {
        // DEDUP-306: Delegate to CLI run_get_callers handler

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let depth = request.depth.unwrap_or(1).min(3);
        let limit = request.limit.unwrap_or(20).min(50);
        let include_source = request.include_source.unwrap_or(false);

        // Create command context (MCP uses TOON format)
        let ctx = CommandContext {
            format: OutputFormat::Toon,
            verbose: false,
            progress: false,
        };

        // Delegate to CLI handler
        match run_get_callers(Some(&repo_path), &request.symbol_hash, depth, include_source, limit, &ctx) {
            Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to get callers: {}",
                e
            ))])),
        }
    }
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
