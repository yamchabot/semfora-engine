//! MCP Server for semfora-engine
//!
//! This module provides an MCP (Model Context Protocol) server that exposes
//! the semantic code analysis capabilities of semfora-engine as tools that can be
//! called by AI assistants like Claude.

mod helpers;
mod types;

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
    encode_toon, encode_toon_directory, generate_repo_overview, Lang, MergedBlock,
    SemanticSummary, CacheDir, RipgrepSearchResult, ShardWriter, SymbolIndexEntry,
    test_runner::{self, TestFramework, TestRunOptions},
    server::ServerState,
    duplicate::{DuplicateDetector, FunctionSignature},
};
use crate::utils::truncate_to_char_boundary;

// Re-export types for external use
pub use types::*;
use helpers::{
    check_cache_staleness, check_cache_staleness_detailed, collect_files, parse_and_extract,
    generate_index_internal, IndexGenerationResult, analyze_files_with_stats, filter_repo_overview,
    ensure_fresh_index, FreshnessResult, RefreshType, format_freshness_note,
};

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

    #[tool(description = "Get quick git and project context in ~200 tokens. **Use this FIRST** when starting work on a repository to understand: current branch, last commit, index status, and project type. Much faster and smaller than get_repo_overview.")]
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
                    output.push_str(&format!("stale_files: {}\n", staleness.modified_files.len()));
                } else {
                    output.push_str("index_status: \"fresh\"\n");
                }

                // Try to read project type and entry points from overview
                let overview_path = cache.repo_overview_path();
                if let Ok(content) = fs::read_to_string(&overview_path) {
                    // Extract framework line
                    for line in content.lines() {
                        if line.starts_with("framework:") {
                            let framework = line.trim_start_matches("framework:").trim().trim_matches('"');
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

    #[tool(description = "Analyze a source file and extract semantic information (symbols, imports, state, control flow). Returns a compact TOON or JSON summary that is much smaller than the original source code.")]
    async fn analyze_file(
        &self,
        Parameters(request): Parameters<AnalyzeFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = self.resolve_path(&request.path).await;

        if !file_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "File not found: {}", file_path.display()
            ))]));
        }

        let lang = match Lang::from_path(&file_path) {
            Ok(l) => l,
            Err(_) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Unsupported file type: {}", file_path.display()
            ))])),
        };

        let source = match fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read file: {}", e
            ))])),
        };

        let summary = match parse_and_extract(&file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Analysis failed: {}", e
            ))])),
        };

        let output = match request.format.as_deref() {
            Some("json") => serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".to_string()),
            _ => encode_toon(&summary),
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Analyze all source files in a directory recursively. Returns a repository overview with framework detection, module grouping, and risk assessment, plus individual file summaries. The output is highly compressed compared to raw source code.")]
    async fn analyze_directory(
        &self,
        Parameters(request): Parameters<AnalyzeDirectoryRequest>,
    ) -> Result<CallToolResult, McpError> {
        let dir_path = self.resolve_path(&request.path).await;

        if !dir_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}", dir_path.display()
            ))]));
        }

        if !dir_path.is_dir() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Not a directory: {}", dir_path.display()
            ))]));
        }

        let max_depth = request.max_depth.unwrap_or(10);
        let summary_only = request.summary_only.unwrap_or(false);
        let extensions = request.extensions.unwrap_or_default();

        let files = collect_files(&dir_path, max_depth, &extensions);

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "directory: {}\nfiles_found: 0\n", dir_path.display()
            ))]));
        }

        let summaries = analyze_files(&files);
        let dir_str = dir_path.display().to_string();
        let overview = generate_repo_overview(&summaries, &dir_str);

        let output = if summary_only {
            encode_toon_directory(&overview, &[])
        } else {
            encode_toon_directory(&overview, &summaries)
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "**Use for code reviews** - analyzes changes between git branches or commits semantically. Shows new/modified symbols, changed dependencies, and risk assessment for each file. Use target_ref='WORKING' to review uncommitted changes before committing.")]
    async fn analyze_diff(
        &self,
        Parameters(request): Parameters<AnalyzeDiffRequest>,
    ) -> Result<CallToolResult, McpError> {
        let working_dir = match &request.working_dir {
            Some(wd) => self.resolve_path(wd).await,
            None => self.get_working_dir().await,
        };

        if !crate::git::is_git_repo(Some(&working_dir)) {
            return Ok(CallToolResult::error(vec![Content::text("Not a git repository")]));
        }

        let base_ref = &request.base_ref;
        let target_ref = request.target_ref.as_deref().unwrap_or("HEAD");

        // Handle special case for uncommitted changes
        let (changed_files, display_target) = if target_ref.eq_ignore_ascii_case("WORKING") {
            // Compare base_ref against working tree (uncommitted changes)
            let files = match crate::git::get_uncommitted_changes(base_ref, Some(&working_dir)) {
                Ok(files) => files,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to get uncommitted changes: {}", e
                ))])),
            };
            (files, "WORKING (uncommitted)")
        } else {
            // Normal comparison between refs
            let merge_base = crate::git::get_merge_base(base_ref, target_ref, Some(&working_dir))
                .unwrap_or_else(|_| base_ref.to_string());

            let files = match crate::git::get_changed_files(&merge_base, target_ref, Some(&working_dir)) {
                Ok(files) => files,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to get changed files: {}", e
                ))])),
            };
            (files, target_ref)
        };

        if changed_files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No files changed.\n")]));
        }

        let output = format_diff_output(&working_dir, base_ref, display_target, &changed_files);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "List all programming languages supported by semfora-engine for semantic analysis")]
    fn list_languages(
        &self,
        Parameters(_request): Parameters<ListLanguagesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = get_supported_languages();
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Sharded Index Tools
    // ========================================================================

    #[tool(description = "Get the repository overview from a pre-built sharded index. Returns a compact summary with framework detection, module list, risk breakdown, and entry points. Use this to understand a codebase before diving into specific modules. By default excludes test directories and limits to 30 modules for token efficiency.")]
    async fn get_repo_overview(
        &self,
        Parameters(request): Parameters<GetRepoOverviewRequest>,
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
                let filtered_content = filter_repo_overview(
                    &content,
                    max_modules,
                    exclude_test_dirs,
                );

                output.push_str(&filtered_content);
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read overview: {}", e
            ))])),
        }
    }

    #[tool(description = "List all modules available in a repository's sharded index. Returns module names that can be queried with get_module.")]
    async fn list_modules(
        &self,
        Parameters(request): Parameters<ListModulesRequest>,
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
        let cache = freshness.cache.clone();

        let modules = cache.list_modules();

        let mut output = String::new();

        // Add freshness note if index was refreshed
        if let Some(note) = format_freshness_note(&freshness) {
            output.push_str(&note);
            output.push_str("\n\n");
        }

        if modules.is_empty() {
            output.push_str("No modules found in index.");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        output.push_str(&format!("Available modules ({}):\n", modules.len()));
        for module in &modules {
            output.push_str(&format!("  - {}\n", module));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Get detailed semantic information for a specific module (e.g., 'api', 'components', 'lib'). Returns all symbols in that module with their risk levels, dependencies, and function calls. Use after get_repo_overview to drill down into specific areas.")]
    async fn get_module(
        &self,
        Parameters(request): Parameters<GetModuleRequest>,
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

        let module_path = cache.module_path(&request.module_name);
        if !module_path.exists() {
            let available = cache.list_modules();
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Module '{}' not found. Available modules: {}",
                request.module_name, available.join(", ")
            ))]));
        }

        match fs::read_to_string(&module_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read module: {}", e
            ))])),
        }
    }

    #[tool(description = "Get detailed semantic information for a specific symbol by its hash. Symbol hashes are found in the repo_overview or module shards. Returns the complete semantic summary including all calls, state changes, and control flow.")]
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

        let symbol_path = cache.symbol_path(&request.symbol_hash);
        if !symbol_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Symbol '{}' not found in index", request.symbol_hash
            ))]));
        }

        match fs::read_to_string(&symbol_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read symbol: {}", e
            ))])),
        }
    }

    #[tool(description = "Generate a sharded semantic index for a repository. This creates a queryable cache with repo_overview, module shards, symbol shards, and dependency graphs. Run this once for a repo, then use get_repo_overview/get_module/get_symbol for fast queries.")]
    async fn generate_index(
        &self,
        Parameters(request): Parameters<GenerateIndexRequest>,
    ) -> Result<CallToolResult, McpError> {
        let dir_path = self.resolve_path(&request.path).await;

        if !dir_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}", dir_path.display()
            ))]));
        }

        if !dir_path.is_dir() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Not a directory: {}", dir_path.display()
            ))]));
        }

        let max_depth = request.max_depth.unwrap_or(10);
        let extensions = request.extensions.unwrap_or_default();

        // Use the shared internal function
        let result = match generate_index_internal(&dir_path, max_depth, &extensions) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        if result.files_analyzed == 0 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No supported files found in {}", dir_path.display()
            ))]));
        }

        // Get cache path for display
        let cache = CacheDir::for_repo(&dir_path).ok();
        let cache_display = cache
            .as_ref()
            .map(|c| c.root.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let output = format!(
            "Sharded index created for: {}\n\
             Cache: {}\n\n\
             Files analyzed: {}\n\
             Modules: {}\n\
             Symbols: {}\n\
             Compression: {:.1}%\n\
             Duration: {}ms\n\n\
             Use get_repo_overview to see the high-level architecture.",
            dir_path.display(),
            cache_display,
            result.files_analyzed,
            result.modules_written,
            result.symbols_written,
            result.compression_pct,
            result.duration_ms
        );

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Understand code flow and dependencies between functions. **Use with filters** (module, symbol) for targeted analysis - unfiltered output can be very large. Returns a mapping of symbol -> [called symbols] useful for architectural understanding.")]
    async fn get_call_graph(
        &self,
        Parameters(request): Parameters<GetCallGraphRequest>,
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
        let cache = freshness.cache;

        let call_graph_path = cache.call_graph_path();
        if !call_graph_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Call graph not found in index. The index may need to be regenerated."
            )]));
        }

        // Parse parameters
        let limit = request.limit.unwrap_or(500).min(2000) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let summary_only = request.summary_only.unwrap_or(false);
        
        // Check file size - for large files (>10MB), require filter or default to summary
        let file_size = fs::metadata(&call_graph_path).map(|m| m.len()).unwrap_or(0);
        let is_large = file_size > 10 * 1024 * 1024; // 10MB threshold
        
        if is_large && request.module.is_none() && request.symbol.is_none() && !summary_only {
            // For large repos without filters, return summary with instructions
            let file = match fs::File::open(&call_graph_path) {
                Ok(f) => f,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to open call graph: {}", e
                ))])),
            };
            
            let reader = BufReader::new(file);
            let mut total_edges = 0usize;
            let mut total_callees = 0usize;
            let mut top_callers: Vec<(String, usize)> = Vec::new();
            
            for line in reader.lines().filter_map(|l| l.ok()) {
                if line.starts_with("_type:") || line.starts_with("schema_version:") || line.starts_with("edges:") {
                    continue;
                }
                if let Some(colon_pos) = line.find(':') {
                    let caller = line[..colon_pos].trim().to_string();
                    let rest = line[colon_pos + 1..].trim();
                    if rest.starts_with('[') && rest.ends_with(']') {
                        let inner = &rest[1..rest.len()-1];
                        let callee_count = if inner.is_empty() { 0 } else { inner.matches(',').count() + 1 };
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
            output.push_str(&format!("avg_callees_per_caller: {:.1}\n\n", total_callees as f64 / total_edges.max(1) as f64));
            
            output.push_str("top_callers_by_fan_out:\n");
            for (caller, count) in &top_callers {
                output.push_str(&format!("  {} ({} callees)\n", caller, count));
            }
            
            output.push_str("\n⚠️ Large call graph detected. Use filters to query specific parts:\n");
            output.push_str("  - module: Filter by module name\n");
            output.push_str("  - symbol: Filter by symbol name\n");
            output.push_str("  - summary_only: true for statistics only\n");
            
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Stream through file with filtering (for filtered queries or small files)
        let file = match fs::File::open(&call_graph_path) {
            Ok(f) => f,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to open call graph: {}", e
            ))])),
        };
        
        let reader = BufReader::new(file);
        let mut edges: Vec<(String, Vec<String>)> = Vec::new();
        let mut total_edges = 0usize;
        let mut skipped = 0usize;
        
        for line in reader.lines().filter_map(|l| l.ok()) {
            // Skip header lines
            if line.starts_with("_type:") || line.starts_with("schema_version:") || line.starts_with("edges:") {
                continue;
            }
            
            // Parse edge
            if let Some(colon_pos) = line.find(':') {
                let caller = line[..colon_pos].trim();
                let rest = line[colon_pos + 1..].trim();
                
                if rest.starts_with('[') && rest.ends_with(']') {
                    total_edges += 1;
                    
                    let inner = &rest[1..rest.len()-1];
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
                            let callee_matches = callees.iter().any(|c| c.contains(module.as_str()));
                            if !caller_matches && !callee_matches {
                                matches = false;
                            }
                        }
                        
                        if matches {
                            if let Some(symbol) = &request.symbol {
                                let symbol_lower = symbol.to_lowercase();
                                let caller_matches = caller.to_lowercase().contains(&symbol_lower);
                                let callee_matches = callees.iter().any(|c| c.to_lowercase().contains(&symbol_lower));
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
            let output = format_call_graph_summary(&edges, total_edges, edges.len());
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Paginated output
        let output = format_call_graph_paginated(
            &edges,
            total_edges,
            filtered_count,
            offset,
            limit,
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Query-Driven API Tools
    // ========================================================================

    #[tool(description = "Get source code for a specific symbol or line range. Use this instead of reading entire files - it returns only the relevant code snippet with optional context lines. Can specify lines directly or use a symbol_hash to auto-lookup the range.")]
    async fn get_symbol_source(
        &self,
        Parameters(request): Parameters<GetSymbolSourceRequest>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = self.resolve_path(&request.file_path).await;
        let context = request.context.unwrap_or(5);

        let (start_line, end_line) = match resolve_line_range(&file_path, &request).await {
            Ok(range) => range,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let source = match fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read file {}: {}", file_path.display(), e
            ))])),
        };

        let output = format_source_snippet(&file_path, &source, start_line, end_line, context);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "**Use for finding specific code** by symbol name. Returns lightweight entries (~400 tokens for 20 results) - much more efficient than browsing modules. Supports wildcards like '*Manager'. Use get_symbol(hash) for full details on specific matches. Falls back to ripgrep if no index exists.")]
    async fn search_symbols(
        &self,
        Parameters(request): Parameters<SearchSymbolsRequest>,
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
        let freshness_note = format_freshness_note(&freshness);
        let cache = freshness.cache;

        let limit = request.limit.unwrap_or(20).min(100);

        // If working_overlay is true, search only uncommitted files
        if request.working_overlay.unwrap_or(false) {
            // Map symbol kind to file extensions for filtering
            let file_types = match request.kind.as_deref() {
                Some("component") => Some(vec![
                    "tsx".to_string(), "jsx".to_string(), "vue".to_string(), "svelte".to_string()
                ]),
                Some("fn") | Some("function") | Some("method") => None, // All languages
                Some("struct") => Some(vec![
                    "rs".to_string(), "go".to_string(), "cs".to_string()
                ]),
                Some("trait") => Some(vec!["rs".to_string()]),
                Some("enum") => Some(vec![
                    "rs".to_string(), "ts".to_string(), "cs".to_string(), "java".to_string(), "kt".to_string()
                ]),
                Some("class") => Some(vec![
                    "py".to_string(), "ts".to_string(), "tsx".to_string(),
                    "java".to_string(), "kt".to_string(), "cs".to_string()
                ]),
                Some("interface") => Some(vec![
                    "ts".to_string(), "tsx".to_string(), "java".to_string(),
                    "kt".to_string(), "cs".to_string(), "go".to_string()
                ]),
                _ => None,
            };

            let results = match cache.search_working_overlay(&request.query, file_types, limit) {
                Ok(r) => r,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Working overlay search failed: {}", e
                ))])),
            };

            let output = format_working_overlay_results(&request.query, &results);
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Use fallback-aware search
        let result = match cache.search_symbols_with_fallback(
            &request.query,
            request.module.as_deref(),
            request.kind.as_deref(),
            request.risk.as_deref(),
            limit,
        ) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {}", e
            ))])),
        };

        let mut output = String::new();

        // Add freshness note if index was refreshed
        if let Some(note) = freshness_note {
            output.push_str(&note);
            output.push_str("\n\n");
        }

        let results_str = if result.fallback_used {
            format_ripgrep_results(&request.query, result.ripgrep_results.as_deref().unwrap_or(&[]))
        } else {
            format_search_results(&request.query, result.indexed_results.as_deref().unwrap_or(&[]))
        };
        output.push_str(&results_str);

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Direct ripgrep search that bypasses the semantic index. Use this for searching comments, strings, non-indexed content, or when you need raw text search. Supports regex patterns and file type filtering. For symbol-aware search, prefer search_symbols.")]
    async fn raw_search(
        &self,
        Parameters(request): Parameters<RawSearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::ripgrep::{RipgrepSearcher, SearchOptions};

        let search_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let limit = request.limit.unwrap_or(50).min(200);
        let merge_threshold = request.merge_threshold.unwrap_or(3);

        let mut options = SearchOptions::new(&request.pattern)
            .with_limit(limit)
            .with_merge_threshold(merge_threshold);

        if request.case_insensitive.unwrap_or(true) {
            options = options.case_insensitive();
        }

        if let Some(types) = &request.file_types {
            options = options.with_file_types(types.clone());
        }

        let searcher = RipgrepSearcher::new();

        // If merge_threshold > 0, return merged blocks; otherwise raw matches
        let output = if merge_threshold > 0 {
            match searcher.search_merged(&search_path, &options) {
                Ok(blocks) => format_merged_blocks(&request.pattern, &blocks, &search_path),
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Search failed: {}", e
                ))])),
            }
        } else {
            match searcher.search(&search_path, &options) {
                Ok(matches) => {
                    let results: Vec<RipgrepSearchResult> = matches
                        .into_iter()
                        .map(|m| RipgrepSearchResult {
                            file: m.file.strip_prefix(&search_path)
                                .unwrap_or(&m.file)
                                .to_string_lossy()
                                .to_string(),
                            line: m.line,
                            column: m.column,
                            content: m.content,
                        })
                        .collect();
                    format_ripgrep_results(&request.pattern, &results)
                }
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Search failed: {}", e
                ))])),
            }
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "List all symbols in a specific module. Returns lightweight index entries only (symbol, hash, kind, file, lines, risk). Much more efficient than get_module for browsing module contents. Use get_symbol(hash) to get full details for specific symbols.")]
    async fn list_symbols(
        &self,
        Parameters(request): Parameters<ListSymbolsRequest>,
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

        let limit = request.limit.unwrap_or(50).min(200);

        let results = match cache.list_module_symbols(
            &request.module,
            request.kind.as_deref(),
            request.risk.as_deref(),
            limit,
        ) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "List failed: {}", e
            ))])),
        };

        let output = format_module_symbols(&request.module, &results, &cache);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Batch Operation Tools
    // ========================================================================

    #[tool(description = "Get detailed semantic information for multiple symbols by their hashes. More efficient than multiple get_symbol calls. Returns up to 20 symbols per request. Use this for batch fetching after search_symbols.")]
    async fn get_symbols(
        &self,
        Parameters(request): Parameters<GetSymbolsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.", repo_path.display()
            ))]));
        }

        // Limit to 20 symbols per request for token efficiency
        let hashes: Vec<&str> = request.hashes.iter()
            .take(20)
            .map(|s| s.as_str())
            .collect();

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
                            if let Some(source_snippet) = extract_source_for_symbol(&cache, &content, context) {
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

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Check if the semantic index is fresh or stale. Returns staleness status, age, and modified files. Use auto_refresh=true to automatically regenerate a stale index.")]
    async fn check_index(
        &self,
        Parameters(request): Parameters<CheckIndexRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        // Check if index exists
        if !cache.exists() {
            return Ok(CallToolResult::success(vec![Content::text(
                "_type: index_status\nstatus: missing\nhint: Run generate_index to create the index."
            )]));
        }

        // Get detailed staleness info
        let max_age = request.max_age.unwrap_or(3600); // Default: 1 hour
        let staleness = check_cache_staleness_detailed(&cache, max_age);

        let mut output = String::new();
        output.push_str("_type: index_status\n");
        output.push_str(&format!("status: {}\n", if staleness.is_stale { "stale" } else { "fresh" }));
        output.push_str(&format!("age_seconds: {}\n", staleness.age_seconds));
        output.push_str(&format!("files_checked: {}\n", staleness.files_checked));
        output.push_str(&format!("modified_count: {}\n", staleness.modified_files.len()));

        if !staleness.modified_files.is_empty() {
            output.push_str("modified_files:\n");
            for file in staleness.modified_files.iter().take(10) {
                output.push_str(&format!("  - {}\n", file));
            }
            if staleness.modified_files.len() > 10 {
                output.push_str(&format!("  ... and {} more\n", staleness.modified_files.len() - 10));
            }
        }

        // Auto-refresh if requested and stale
        if staleness.is_stale && request.auto_refresh.unwrap_or(false) {
            output.push_str("\nauto_refresh: initiating\n");

            // Create shard writer and regenerate
            let mut shard_writer = match ShardWriter::new(&repo_path) {
                Ok(w) => w,
                Err(e) => {
                    output.push_str(&format!("refresh_error: {}\n", e));
                    return Ok(CallToolResult::success(vec![Content::text(output)]));
                }
            };

            let files = collect_files(&repo_path, 10, &[]);
            if files.is_empty() {
                output.push_str("refresh_error: No supported files found\n");
                return Ok(CallToolResult::success(vec![Content::text(output)]));
            }

            let (summaries, _) = analyze_files_with_stats(&files);
            shard_writer.add_summaries(summaries.clone());

            let dir_str = repo_path.display().to_string();
            match shard_writer.write_all(&dir_str) {
                Ok(stats) => {
                    output.push_str("refresh_status: completed\n");
                    output.push_str(&format!("refreshed_modules: {}\n", stats.modules_written));
                    output.push_str(&format!("refreshed_symbols: {}\n", stats.symbols_written));
                }
                Err(e) => {
                    output.push_str(&format!("refresh_error: {}\n", e));
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Test Runner Tools (North Star - VALIDATE phase)
    // ========================================================================

    #[tool(description = "Run tests in a project directory. Auto-detects the test framework (pytest, cargo test, npm test, go test) or use a specific framework. Returns structured results including pass/fail counts, failures, and duration.")]
    async fn run_tests(
        &self,
        Parameters(request): Parameters<RunTestsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let project_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        if !project_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}", project_path.display()
            ))]));
        }

        // Parse framework if specified
        let framework = match &request.framework {
            Some(f) => match f.to_lowercase().as_str() {
                "pytest" | "python" => Some(TestFramework::Pytest),
                "cargo" | "rust" => Some(TestFramework::Cargo),
                "npm" | "node" => Some(TestFramework::Npm),
                "vitest" => Some(TestFramework::Vitest),
                "jest" => Some(TestFramework::Jest),
                "go" | "golang" => Some(TestFramework::Go),
                _ => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown framework '{}'. Valid options: pytest, cargo, npm, vitest, jest, go", f
                ))])),
            },
            None => None,
        };

        // Build run options
        let options = TestRunOptions {
            filter: request.filter.clone(),
            timeout_secs: request.timeout,
            verbose: request.verbose.unwrap_or(false),
            extra_args: Vec::new(),
        };

        // Run tests
        let results = match framework {
            Some(fw) => test_runner::run_tests_with_framework(&project_path, fw, &options),
            None => test_runner::run_tests(&project_path, &options),
        };

        match results {
            Ok(results) => {
                let output = format_test_results(&results);
                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Test execution failed: {}", e
            ))])),
        }
    }

    #[tool(description = "Detect test frameworks in a project directory. Returns detected framework(s) and their locations. Useful for monorepo setups where multiple test runners may be present.")]
    async fn detect_tests(
        &self,
        Parameters(request): Parameters<DetectTestsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let project_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        if !project_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}", project_path.display()
            ))]));
        }

        let check_subdirs = request.check_subdirs.unwrap_or(false);

        let mut output = String::new();
        output.push_str("_type: test_frameworks\n");
        output.push_str(&format!("path: {}\n", project_path.display()));

        if check_subdirs {
            // Check for monorepo setup
            let frameworks = test_runner::detect_all_frameworks(&project_path);
            output.push_str(&format!("detected: {}\n", frameworks.len()));

            if frameworks.is_empty() {
                output.push_str("frameworks: (none detected)\n");
                output.push_str("hint: No Cargo.toml, package.json, pyproject.toml, or go.mod found.\n");
            } else {
                output.push_str("frameworks:\n");
                for (fw, path) in &frameworks {
                    let rel_path = path.strip_prefix(&project_path)
                        .unwrap_or(path)
                        .to_string_lossy();
                    let rel_path = if rel_path.is_empty() { "." } else { &rel_path };
                    output.push_str(&format!("  - {}: {}\n", fw.as_str(), rel_path));
                }
            }
        } else {
            // Single framework detection
            let framework = test_runner::detect_framework(&project_path);
            output.push_str(&format!("framework: {}\n", framework.as_str()));

            if framework == TestFramework::Unknown {
                output.push_str("hint: No test framework detected. Ensure project has Cargo.toml, package.json, pyproject.toml, or go.mod.\n");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Layer Management Tools (SEM-98, SEM-99, SEM-101, SEM-102, SEM-104)
    // ========================================================================

    #[tool(description = "Get the current status of all semantic layers (Base, Branch, Working, AI). Shows which layers are stale, their update strategies, and symbol counts. Requires persistent server mode.")]
    async fn get_layer_status(
        &self,
        Parameters(_request): Parameters<GetLayerStatusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let server_state = match &self.server_state {
            Some(state) => state,
            None => return Ok(CallToolResult::error(vec![Content::text(
                "Layer status not available. Server not running in persistent mode.\n\
                 Hint: Start the MCP server with --persistent flag to enable live layer updates."
            )])),
        };

        let status = server_state.status();
        let mut output = String::new();
        output.push_str("_type: layer_status\n");
        output.push_str(&format!("repo_root: {}\n", status.repo_root.display()));
        output.push_str(&format!("is_running: {}\n", status.is_running));
        output.push_str(&format!("uptime_secs: {}\n", status.uptime.as_secs()));
        output.push_str("\nlayers:\n");

        for layer_status in &status.layers {
            output.push_str(&format!("  - kind: {:?}\n", layer_status.kind));
            output.push_str(&format!("    is_stale: {}\n", layer_status.is_stale));
            output.push_str(&format!("    symbol_count: {}\n", layer_status.symbol_count));
            output.push_str(&format!("    strategy: {:?}\n", layer_status.strategy));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Check if persistent server mode is enabled. Returns information about the server state and available background services.")]
    async fn check_server_mode(
        &self,
        Parameters(_request): Parameters<CheckServerModeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut output = String::new();
        output.push_str("_type: server_mode\n");
        output.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
        output.push_str(&format!("persistent_mode: {}\n", self.server_state.is_some()));

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
        } else {
            output.push_str("features:\n");
            output.push_str("  - file_watcher: disabled\n");
            output.push_str("  - git_poller: disabled\n");
            output.push_str("  - thread_safe: n/a\n");
            output.push_str("\nhint: Start with --persistent to enable live layer updates\n");
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Duplicate Detection Tools
    // ========================================================================

    #[tool(description = "Find code duplication across the entire codebase. **Use for codebase health audits** or before major refactoring. Fast even on massive repos (O(n) fingerprinting). Returns groups of similar functions that may be candidates for consolidation.")]
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
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        // Load signatures from index
        let signatures = match load_signatures(&cache) {
            Ok(sigs) => sigs,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to load signature index: {}. Run generate_index first.", e
            ))])),
        };

        if signatures.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "_type: duplicate_results\nclusters: 0\nmessage: No function signatures found in index.\n"
            )]));
        }

        // Configure detector
        let threshold = request.threshold.unwrap_or(0.90);
        let exclude_boilerplate = request.exclude_boilerplate.unwrap_or(true);
        let min_lines = request.min_lines.unwrap_or(3) as usize;
        let limit = request.limit.unwrap_or(50).min(200) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let sort_by = request.sort_by.as_deref().unwrap_or("similarity");

        let detector = DuplicateDetector::new(threshold)
            .with_boilerplate_exclusion(exclude_boilerplate);

        // Filter by module and min_lines
        let filtered_sigs: Vec<_> = signatures.iter()
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
                    let a_max = a.duplicates.iter().map(|d| d.similarity).fold(0.0_f64, f64::max);
                    let b_max = b.duplicates.iter().map(|d| d.similarity).fold(0.0_f64, f64::max);
                    b_max.partial_cmp(&a_max).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // Apply pagination
        let paginated: Vec<_> = clusters.into_iter()
            .skip(offset)
            .take(limit)
            .collect();

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

    #[tool(description = "**Use before writing new functions** to avoid duplication. Returns similar existing functions that match the specified symbol hash. Also useful during refactoring to find consolidation candidates.")]
    async fn check_duplicates(
        &self,
        Parameters(request): Parameters<CheckDuplicatesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        // Load signatures from index
        let signatures = match load_signatures(&cache) {
            Ok(sigs) => sigs,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to load signature index: {}. Run generate_index first.", e
            ))])),
        };

        // Find the target signature
        let target = match signatures.iter().find(|s| s.symbol_hash == request.symbol_hash) {
            Some(sig) => sig,
            None => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Symbol {} not found in signature index.", request.symbol_hash
            ))])),
        };

        // Configure detector
        let threshold = request.threshold.unwrap_or(0.90);
        let detector = DuplicateDetector::new(threshold);

        // Find duplicates for this symbol
        let matches = detector.find_duplicates(target, &signatures);

        let output = format_duplicate_matches(&target.name, &target.file, &matches, threshold);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // AI-Optimized Combined Query Tools
    // ========================================================================

    #[tool(description = "Combined search + fetch: search symbols AND return full semantic details with source code in ONE call. Eliminates the search_symbols -> get_symbol -> get_symbol_source round-trip. Returns up to 20 symbols with their full TOON summaries and source snippets.")]
    async fn search_and_get_symbols(
        &self,
        Parameters(request): Parameters<SearchAndGetSymbolsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.", repo_path.display()
            ))]));
        }

        // Limit to 20 for token efficiency
        let limit = request.limit.unwrap_or(10).min(20);
        let include_source = request.include_source.unwrap_or(true);
        let context = request.context.unwrap_or(3);

        // Step 1: Search for matching symbols
        let search_result = match cache.search_symbols_with_fallback(
            &request.query,
            request.module.as_deref(),
            request.kind.as_deref(),
            request.risk.as_deref(),
            limit,
        ) {
            Ok(r) => r,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {}", e
            ))])),
        };

        // If fallback was used, we don't have hashes - return ripgrep results
        if search_result.fallback_used {
            let output = format_ripgrep_results(&request.query, search_result.ripgrep_results.as_deref().unwrap_or(&[]));
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let entries = search_result.indexed_results.unwrap_or_default();
        if entries.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "_type: search_and_get_results\nquery: \"{}\"\nshowing: 0\nresults: (none)\n",
                request.query
            ))]));
        }

        // Step 2: Fetch full details for each symbol
        let mut output = String::new();
        output.push_str("_type: search_and_get_results\n");
        output.push_str(&format!("query: \"{}\"\n", request.query));
        output.push_str(&format!("showing: {}\n", entries.len()));
        output.push_str(&format!("include_source: {}\n", include_source));

        for entry in &entries {
            output.push_str(&format!("\n=== {} ({}) ===\n", entry.symbol, entry.hash));
            output.push_str(&format!("file: {}\n", entry.file));
            output.push_str(&format!("lines: {}\n", entry.lines));
            output.push_str(&format!("kind: {}\n", entry.kind));
            output.push_str(&format!("risk: {}\n", entry.risk));

            // Load full symbol shard if available
            let symbol_path = cache.symbol_path(&entry.hash);
            if symbol_path.exists() {
                if let Ok(content) = fs::read_to_string(&symbol_path) {
                    // Extract key semantic info from shard
                    output.push_str("\n__semantic__:\n");
                    for line in content.lines() {
                        let trimmed = line.trim();
                        // Include meaningful semantic lines
                        if trimmed.starts_with("calls") ||
                           trimmed.starts_with("state_changes") ||
                           trimmed.starts_with("control_flow") ||
                           trimmed.starts_with("added_dependencies") ||
                           trimmed.starts_with("insertions") ||
                           trimmed.starts_with("  ") {
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                }
            }

            // Include source if requested
            if include_source {
                if let Some(source) = get_symbol_source_snippet(&cache, &entry.file, &entry.lines, context) {
                    output.push_str("\n__source__:\n");
                    output.push_str(&source);
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Get all symbols in a specific file without needing to know the module. File-centric view for when you know the file path but not how it maps to modules. Returns symbols with optional source snippets.")]
    async fn get_file_symbols(
        &self,
        Parameters(request): Parameters<GetFileSymbolsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.", repo_path.display()
            ))]));
        }

        let include_source = request.include_source.unwrap_or(false);
        let context = request.context.unwrap_or(2);

        // Normalize the file path for matching
        let target_file = request.file_path.trim_start_matches("./");

        // Search the symbol index for symbols in this file
        let symbols: Vec<_> = match cache.load_all_symbol_entries() {
            Ok(all) => {
                all.into_iter()
                    .filter(|e| {
                        let entry_file = e.file.trim_start_matches("./");
                        entry_file == target_file ||
                        entry_file.ends_with(target_file) ||
                        target_file.ends_with(entry_file)
                    })
                    .filter(|e| {
                        request.kind.as_ref().map_or(true, |k| e.kind == *k)
                    })
                    .collect()
            }
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to load symbol index: {}", e
            ))])),
        };

        if symbols.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "_type: file_symbols\nfile: \"{}\"\nshowing: 0\nsymbols: (none)\nhint: File may not be indexed or path doesn't match.\n",
                request.file_path
            ))]));
        }

        let mut output = String::new();
        output.push_str("_type: file_symbols\n");
        output.push_str(&format!("file: \"{}\"\n", request.file_path));
        output.push_str(&format!("showing: {}\n", symbols.len()));
        output.push_str(&format!("symbols[{}]{{name,hash,kind,lines,risk}}:\n", symbols.len()));

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
                if let Some(source) = get_symbol_source_snippet(&cache, &entry.file, &entry.lines, context) {
                    output.push_str(&format!("\n--- {} ({}) ---\n", entry.symbol, entry.lines));
                    output.push_str(&source);
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "**Use before modifying existing code** to understand impact radius. Answers 'what functions call this symbol?' Shows what will break if you change this function. Returns direct callers and optionally transitive callers (up to depth 3).")]
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
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.", repo_path.display()
            ))]));
        }

        let depth = request.depth.unwrap_or(1).min(3);
        let limit = request.limit.unwrap_or(20).min(50);
        let include_source = request.include_source.unwrap_or(false);

        // Load call graph
        let call_graph_path = cache.call_graph_path();
        if !call_graph_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Call graph not found. Run generate_index to create it."
            )]));
        }

        let content = match fs::read_to_string(&call_graph_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read call graph: {}", e
            ))])),
        };

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
                        // Skip external calls
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
        let target_hash = &request.symbol_hash;
        let target_name = symbol_names.get(target_hash).cloned().unwrap_or_else(|| target_hash.clone());

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

        // Format output
        let mut output = String::new();
        output.push_str("_type: callers\n");
        output.push_str(&format!("target: {} ({})\n", target_name, target_hash));
        output.push_str(&format!("depth: {}\n", depth));
        output.push_str(&format!("total_callers: {}\n", all_callers.len()));

        if all_callers.is_empty() {
            output.push_str("callers: (none - this may be an entry point or unused)\n");
        } else {
            output.push_str(&format!("callers[{}]{{name,hash,depth}}:\n", all_callers.len()));
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
                                    file = Some(trimmed.trim_start_matches("file:").trim().trim_matches('"').to_string());
                                } else if trimmed.starts_with("lines:") {
                                    lines = Some(trimmed.trim_start_matches("lines:").trim().trim_matches('"').to_string());
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

    // ========================================================================
    // Validation Tool (Phase 4)
    // ========================================================================

    #[tool(description = "**Use for quality audits** - validates a symbol's complexity, finds duplicates, and shows impact radius (callers). Combines complexity metrics, duplicate detection, and caller analysis into one comprehensive report. Useful after code review or before refactoring.")]
    async fn validate_symbol(
        &self,
        Parameters(request): Parameters<ValidateSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run generate_index first.", repo_path.display()
            ))]));
        }

        // Find the target symbol
        let (symbol_hash, symbol_entry) = if let Some(ref hash) = request.symbol_hash {
            // Look up by hash
            let entries = match cache.load_all_symbol_entries() {
                Ok(e) => e,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to load symbol index: {}", e
                ))])),
            };
            match entries.into_iter().find(|e| e.hash == *hash) {
                Some(entry) => (hash.clone(), entry),
                None => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol {} not found in index.", hash
                ))])),
            }
        } else if let (Some(ref file_path), Some(line)) = (&request.file_path, request.line) {
            // Look up by file + line
            let entries = match cache.load_all_symbol_entries() {
                Ok(e) => e,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to load symbol index: {}", e
                ))])),
            };

            // Find symbol that contains this line
            let matching = entries.into_iter().find(|e| {
                if !e.file.ends_with(file_path) && !file_path.ends_with(&e.file) {
                    return false;
                }
                if let Some((start, end)) = e.lines.split_once('-') {
                    if let (Ok(s), Ok(en)) = (start.parse::<usize>(), end.parse::<usize>()) {
                        return line >= s && line <= en;
                    }
                }
                false
            });

            match matching {
                Some(entry) => (entry.hash.clone(), entry),
                None => return Ok(CallToolResult::error(vec![Content::text(format!(
                    "No symbol found at {}:{}", file_path, line
                ))])),
            }
        } else {
            return Ok(CallToolResult::error(vec![Content::text(
                "Must provide either symbol_hash or file_path + line"
            )]));
        };

        // Start building the validation report
        let mut output = String::new();
        output.push_str("_type: validation_result\n");
        output.push_str(&format!("symbol: {}\n", symbol_entry.symbol));
        output.push_str(&format!("file: {}\n", symbol_entry.file));
        output.push_str(&format!("lines: {}\n", symbol_entry.lines));
        output.push_str(&format!("kind: {}\n", symbol_entry.kind));
        output.push_str(&format!("hash: {}\n", symbol_hash));

        // Complexity metrics (already computed during indexing)
        output.push_str("\ncomplexity:\n");
        output.push_str(&format!("  cognitive: {}\n", symbol_entry.cognitive_complexity));
        output.push_str(&format!("  max_nesting: {}\n", symbol_entry.max_nesting));
        output.push_str(&format!("  risk: {}\n", symbol_entry.risk));

        // Complexity assessment
        let complexity_concerns: Vec<&str> = {
            let mut concerns = Vec::new();
            if symbol_entry.cognitive_complexity > 15 {
                concerns.push("High cognitive complexity (>15) - consider breaking into smaller functions");
            } else if symbol_entry.cognitive_complexity > 10 {
                concerns.push("Moderate cognitive complexity (>10) - may be hard to maintain");
            }
            if symbol_entry.max_nesting > 4 {
                concerns.push("Deep nesting (>4) - consider early returns or guard clauses");
            }
            concerns
        };

        // Duplicate detection
        let threshold = request.duplicate_threshold.unwrap_or(0.85);
        let mut duplicates: Vec<(String, String, f64)> = Vec::new(); // (name, file, similarity)

        if let Ok(signatures) = load_signatures(&cache) {
            if let Some(target_sig) = signatures.iter().find(|s| s.symbol_hash == symbol_hash) {
                let detector = DuplicateDetector::new(threshold);
                let matches = detector.find_duplicates(target_sig, &signatures);

                for m in matches.iter().take(5) {
                    duplicates.push((
                        m.symbol.name.clone(),
                        m.symbol.file.clone(),
                        m.similarity,
                    ));
                }
            }
        }

        output.push_str("\nduplicates:\n");
        if duplicates.is_empty() {
            output.push_str("  (none found above threshold)\n");
        } else {
            output.push_str(&format!("  count: {}\n", duplicates.len()));
            output.push_str(&format!("  threshold: {:.0}%\n", threshold * 100.0));
            for (name, file, sim) in &duplicates {
                output.push_str(&format!("  - {} ({}) [{:.0}%]\n", name, file, sim * 100.0));
            }
        }

        // Caller analysis (impact radius)
        let mut callers: Vec<(String, String)> = Vec::new(); // (name, hash)
        let mut high_risk_callers: Vec<String> = Vec::new();

        let call_graph_path = cache.call_graph_path();
        if call_graph_path.exists() {
            if let Ok(content) = fs::read_to_string(&call_graph_path) {
                // Build reverse call graph
                let mut reverse_graph: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

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
                                    reverse_graph.entry(callee).or_default().push(caller.clone());
                                }
                            }
                        }
                    }
                }

                // Load symbol names for resolution
                let mut symbol_names: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new(); // hash -> (name, risk)
                if let Ok(entries) = cache.load_all_symbol_entries() {
                    for entry in entries {
                        symbol_names.insert(entry.hash.clone(), (entry.symbol.clone(), entry.risk.clone()));
                    }
                }

                // Find direct callers
                if let Some(caller_hashes) = reverse_graph.get(&symbol_hash) {
                    for caller_hash in caller_hashes.iter().take(20) {
                        if let Some((name, risk)) = symbol_names.get(caller_hash) {
                            callers.push((name.clone(), caller_hash.clone()));
                            if risk == "high" {
                                high_risk_callers.push(name.clone());
                            }
                        } else {
                            callers.push((caller_hash.clone(), caller_hash.clone()));
                        }
                    }
                }
            }
        }

        output.push_str("\ncallers:\n");
        output.push_str(&format!("  direct: {}\n", callers.len()));
        if !high_risk_callers.is_empty() {
            output.push_str(&format!("  high_risk_callers: [{}]\n", high_risk_callers.join(", ")));
        }
        if !callers.is_empty() {
            output.push_str("  list:\n");
            for (name, hash) in callers.iter().take(10) {
                output.push_str(&format!("    - {} ({})\n", name, hash));
            }
            if callers.len() > 10 {
                output.push_str(&format!("    ... and {} more\n", callers.len() - 10));
            }
        }

        // Generate suggestions
        output.push_str("\nsuggestions:\n");
        let mut has_suggestions = false;

        for concern in &complexity_concerns {
            output.push_str(&format!("  - {}\n", concern));
            has_suggestions = true;
        }

        if !duplicates.is_empty() {
            let (dup_name, _, dup_sim) = &duplicates[0];
            output.push_str(&format!(
                "  - {:.0}% similar to {} - consider consolidation\n",
                dup_sim * 100.0, dup_name
            ));
            has_suggestions = true;
        }

        if callers.len() > 10 {
            output.push_str("  - High impact radius (>10 callers) - changes require careful testing\n");
            has_suggestions = true;
        }

        if !has_suggestions {
            output.push_str("  (none - symbol looks good)\n");
        }

        // Include source if requested
        let include_source = request.include_source.unwrap_or(false);
        if include_source {
            if let Some(source) = get_symbol_source_snippet(&cache, &symbol_entry.file, &symbol_entry.lines, 2) {
                output.push_str("\n__source__:\n");
                output.push_str(&source);
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "**Use for conceptual code search** - finds symbols matching loose natural language queries like 'authentication', 'error handling', or 'database connection'. Unlike search_symbols which matches symbol names, this uses BM25 ranking to find conceptually related code. Great for discovering code when you don't know exact function names.")]
    async fn semantic_search(
        &self,
        Parameters(request): Parameters<SemanticSearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        use crate::bm25::Bm25Index;

        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.get_working_dir().await,
        };
        let cache = CacheDir::for_repo(&repo_path).map_err(|e| {
            McpError::internal_error(format!("Failed to access cache: {}", e), None)
        })?;

        // Check if BM25 index exists
        if !cache.has_bm25_index() {
            return Ok(CallToolResult::success(vec![Content::text(
                "_error: BM25 index not found. Run generate_index to create it.\n\
                 _hint: The BM25 index is generated automatically during indexing."
            )]));
        }

        // Load the BM25 index
        let bm25_path = cache.bm25_index_path();
        let index = match Bm25Index::load(&bm25_path) {
            Ok(idx) => idx,
            Err(e) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("_error: Failed to load BM25 index: {}", e)
                )]));
            }
        };

        let limit = request.limit.unwrap_or(20).min(100);
        let include_source = request.include_source.unwrap_or(false);

        // Perform BM25 search
        let mut results = index.search(&request.query, limit * 2); // Get extra for filtering

        // Apply filters
        if let Some(ref kind_filter) = request.kind {
            let kind_lower = kind_filter.to_lowercase();
            results.retain(|r| r.kind.to_lowercase() == kind_lower);
        }
        if let Some(ref module_filter) = request.module {
            let module_lower = module_filter.to_lowercase();
            results.retain(|r| r.module.to_lowercase() == module_lower);
        }

        // Limit results after filtering
        results.truncate(limit);

        if results.is_empty() {
            // Get query suggestions
            let suggestions = index.suggest_related_terms(&request.query, 5);
            let mut output = String::from("_type: semantic_search_results\n");
            output.push_str(&format!("query: \"{}\"\n", request.query));
            output.push_str("result_count: 0\n");
            if !suggestions.is_empty() {
                output.push_str(&format!("_hint: Try related terms: {}\n", suggestions.join(", ")));
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Build output
        let mut output = String::from("_type: semantic_search_results\n");
        output.push_str(&format!("query: \"{}\"\n", request.query));
        output.push_str(&format!("result_count: {}\n", results.len()));
        output.push_str("---\n");

        for result in &results {
            output.push_str(&format!("\n## {} ({})\n", result.symbol, result.kind));
            output.push_str(&format!("hash: {}\n", result.hash));
            output.push_str(&format!("file: {}\n", result.file));
            output.push_str(&format!("lines: {}\n", result.lines));
            output.push_str(&format!("module: {}\n", result.module));
            output.push_str(&format!("risk: {}\n", result.risk));
            output.push_str(&format!("score: {:.3}\n", result.score));
            output.push_str(&format!("matched_terms: {}\n", result.matched_terms.join(", ")));

            // Optionally include source snippet
            if include_source {
                if let Some(source) = get_symbol_source_snippet(&cache, &result.file, &result.lines, 2) {
                    output.push_str("__source__:\n");
                    output.push_str(&source);
                }
            }
        }

        // Add related term suggestions
        let suggestions = index.suggest_related_terms(&request.query, 5);
        if !suggestions.is_empty() {
            output.push_str(&format!("\n---\nrelated_terms: {}\n", suggestions.join(", ")));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

/// Get source snippet for a symbol given file and line range
fn get_symbol_source_snippet(cache: &CacheDir, file: &str, lines: &str, context: usize) -> Option<String> {
    let (start_line, end_line) = if let Some((s, e)) = lines.split_once('-') {
        (s.parse::<usize>().ok()?, e.parse::<usize>().ok()?)
    } else {
        return None;
    };

    let full_path = cache.repo_root.join(file);
    let source = fs::read_to_string(&full_path).ok()?;

    Some(format_source_snippet(&full_path, &source, start_line, end_line, context))
}

/// Format test results as compact TOON output
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
            format!("{}...(truncated)", truncate_to_char_boundary(&results.stdout, 500))
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

// ============================================================================
// Helper Functions (local to this module)
// ============================================================================

/// Analyze a collection of files and return their semantic summaries
fn analyze_files(files: &[PathBuf]) -> Vec<SemanticSummary> {
    let mut summaries = Vec::new();
    for file_path in files {
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if let Ok(summary) = parse_and_extract(file_path, &source, lang) {
            summaries.push(summary);
        }
    }
    summaries
}

/// Format the diff output for changed files
fn format_diff_output(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "diff: {} → {} ({} files)\n\n",
        base_ref, target_ref, changed_files.len()
    ));

    for changed_file in changed_files {
        let full_path = working_dir.join(&changed_file.path);

        output.push_str(&format!(
            "━━━ {} [{}] ━━━\n",
            changed_file.path,
            changed_file.change_type.as_str()
        ));

        if changed_file.change_type == crate::git::ChangeType::Deleted {
            output.push_str("(file deleted)\n\n");
            continue;
        }

        let lang = match Lang::from_path(&full_path) {
            Ok(l) => l,
            Err(_) => {
                output.push_str("(unsupported language)\n\n");
                continue;
            }
        };

        let source = match fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => {
                output.push_str("(could not read file)\n\n");
                continue;
            }
        };

        match parse_and_extract(&full_path, &source, lang) {
            Ok(summary) => {
                output.push_str(&encode_toon(&summary));
                output.push_str("\n\n");
            }
            Err(e) => {
                output.push_str(&format!("(analysis failed: {})\n\n", e));
            }
        }
    }

    output
}

/// Get the list of supported languages as a formatted string
fn get_supported_languages() -> String {
    let languages = vec![
        ("TypeScript", ".ts"),
        ("TSX", ".tsx"),
        ("JavaScript", ".js, .mjs, .cjs"),
        ("JSX", ".jsx"),
        ("Rust", ".rs"),
        ("Python", ".py, .pyi"),
        ("Go", ".go"),
        ("Java", ".java"),
        ("C#", ".cs"),
        ("C", ".c, .h"),
        ("C++", ".cpp, .cc, .cxx, .hpp, .hxx, .hh"),
        ("Kotlin", ".kt, .kts"),
        ("HTML", ".html, .htm"),
        ("CSS", ".css"),
        ("SCSS", ".scss, .sass"),
        ("JSON", ".json"),
        ("YAML", ".yaml, .yml"),
        ("TOML", ".toml"),
        ("XML", ".xml, .xsd, .xsl, .xslt, .svg, .plist, .pom"),
        ("HCL/Terraform", ".tf, .hcl, .tfvars"),
        ("Markdown", ".md, .markdown"),
        ("Vue", ".vue"),
        ("Bash/Shell", ".sh, .bash, .zsh, .fish"),
        ("Gradle", ".gradle"),
    ];

    let mut output = String::from("Supported Languages:\n\n");
    for (name, extensions) in languages {
        output.push_str(&format!("  {} ({})\n", name, extensions));
    }
    output
}

/// Resolve line range from request (either direct or via symbol hash lookup)
async fn resolve_line_range(
    file_path: &Path,
    request: &GetSymbolSourceRequest,
) -> Result<(usize, usize), String> {
    if let Some(ref hash) = request.symbol_hash {
        // Look up line range from symbol shard
        let repo_path = file_path.parent().unwrap_or(Path::new("."));
        let cache = find_cache_for_path(repo_path)?;

        let symbol_path = cache.symbol_path(hash);
        if !symbol_path.exists() {
            return Err(format!("Symbol {} not found in index.", hash));
        }

        let content = fs::read_to_string(&symbol_path)
            .map_err(|e| format!("Failed to read symbol shard: {}", e))?;

        // Parse lines field from TOON: lines: "123-456"
        for line in content.lines() {
            if line.starts_with("lines:") {
                let range_str = line.trim_start_matches("lines:").trim().trim_matches('"');
                if let Some((s, e)) = range_str.split_once('-') {
                    if let (Some(start), Some(end)) = (s.parse().ok(), e.parse().ok()) {
                        return Ok((start, end));
                    }
                }
                break;
            }
        }

        Err("Symbol does not have line range information. Use start_line/end_line directly.".to_string())
    } else {
        match (request.start_line, request.end_line) {
            (Some(s), Some(e)) => Ok((s, e)),
            _ => Err("Either symbol_hash OR both start_line and end_line are required.".to_string()),
        }
    }
}

/// Find cache directory by walking up the directory tree
fn find_cache_for_path(start_path: &Path) -> Result<CacheDir, String> {
    if let Ok(cache) = CacheDir::for_repo(start_path) {
        if cache.exists() {
            return Ok(cache);
        }
    }

    let mut current = start_path.to_path_buf();
    while let Some(parent) = current.parent() {
        if let Ok(c) = CacheDir::for_repo(parent) {
            if c.exists() {
                return Ok(c);
            }
        }
        current = parent.to_path_buf();
    }

    Err("Could not find sharded index. Use start_line/end_line directly or run generate_index.".to_string())
}

/// Extract source code for a symbol given its shard content
/// Parses the file path and line range from the TOON content
fn extract_source_for_symbol(cache: &CacheDir, symbol_content: &str, context: usize) -> Option<String> {
    // Parse file and lines from TOON: "file: ..." and "lines: ..."
    let mut file_path: Option<String> = None;
    let mut start_line: Option<usize> = None;
    let mut end_line: Option<usize> = None;

    for line in symbol_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("file:") {
            file_path = Some(
                trimmed
                    .trim_start_matches("file:")
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
        } else if trimmed.starts_with("lines:") {
            let range_str = trimmed
                .trim_start_matches("lines:")
                .trim()
                .trim_matches('"');
            if let Some((s, e)) = range_str.split_once('-') {
                start_line = s.parse().ok();
                end_line = e.parse().ok();
            }
        }
    }

    let file = file_path?;
    let start = start_line?;
    let end = end_line?;

    // Resolve file path relative to repo root
    let full_path = cache.repo_root.join(&file);
    let source = fs::read_to_string(&full_path).ok()?;

    Some(format_source_snippet(&full_path, &source, start, end, context))
}

/// Format a source code snippet with line numbers and markers
fn format_source_snippet(
    file_path: &Path,
    source: &str,
    start_line: usize,
    end_line: usize,
    context: usize,
) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let total_lines = lines.len();

    let context_start = start_line.saturating_sub(context + 1);
    let context_end = (end_line + context).min(total_lines);

    let mut output = String::new();
    output.push_str(&format!(
        "// {} (lines {}-{}, showing {}-{})\n",
        file_path.display(),
        start_line,
        end_line,
        context_start + 1,
        context_end
    ));

    for (i, line) in lines.iter().enumerate().skip(context_start).take(context_end - context_start) {
        let line_num = i + 1;
        let marker = if line_num >= start_line && line_num <= end_line { ">" } else { " " };
        output.push_str(&format!("{}{:4} | {}\n", marker, line_num, line));
    }

    output
}

/// Format search results as compact TOON
fn format_search_results(query: &str, results: &[SymbolIndexEntry]) -> String {
    let mut output = String::new();
    output.push_str("_type: search_results\n");
    output.push_str(&format!("query: \"{}\"\n", query));
    output.push_str(&format!("showing: {}\n", results.len()));

    if results.is_empty() {
        output.push_str("results: (none)\n");
    } else {
        output.push_str(&format!("results[{}]{{s,h,k,m,f,l,r}}:\n", results.len()));
        for entry in results {
            output.push_str(&format!(
                "  {},{},{},{},{},{},{}\n",
                entry.symbol, entry.hash, entry.kind, entry.module, entry.file, entry.lines, entry.risk
            ));
        }
    }

    output
}

/// Format ripgrep fallback results as compact TOON
fn format_ripgrep_results(query: &str, results: &[RipgrepSearchResult]) -> String {
    let mut output = String::new();
    output.push_str("_type: ripgrep_results\n");
    output.push_str("_note: Using ripgrep fallback (no semantic index). Run generate_index for semantic search.\n");
    output.push_str(&format!("query: \"{}\"\n", query));
    output.push_str(&format!("showing: {}\n", results.len()));

    if results.is_empty() {
        output.push_str("results: (none)\n");
    } else {
        output.push_str(&format!("results[{}]{{file,line,col,content}}:\n", results.len()));
        for entry in results {
            // Truncate long content for display
            let content = if entry.content.len() > 100 {
                format!("{}...", truncate_to_char_boundary(&entry.content, 100))
            } else {
                entry.content.clone()
            };
            output.push_str(&format!(
                "  {}:{}:{}: {}\n",
                entry.file, entry.line, entry.column, content.trim()
            ));
        }
    }

    output
}

/// Format working overlay search results (uncommitted files only)
fn format_working_overlay_results(query: &str, results: &[RipgrepSearchResult]) -> String {
    let mut output = String::new();
    output.push_str("_type: working_overlay_results\n");
    output.push_str("_note: Searching uncommitted files only (staged + unstaged changes)\n");
    output.push_str(&format!("query: \"{}\"\n", query));
    output.push_str(&format!("showing: {}\n", results.len()));

    if results.is_empty() {
        output.push_str("results: (none - no uncommitted files match)\n");
    } else {
        // Group by file for cleaner output
        let mut by_file: std::collections::BTreeMap<&str, Vec<&RipgrepSearchResult>> =
            std::collections::BTreeMap::new();
        for entry in results {
            by_file.entry(&entry.file).or_default().push(entry);
        }

        output.push_str(&format!("files[{}]:\n", by_file.len()));
        for (file, entries) in by_file {
            output.push_str(&format!("  {}:\n", file));
            for entry in entries {
                // Truncate long content for display
                let content = if entry.content.len() > 80 {
                    format!("{}...", truncate_to_char_boundary(&entry.content, 80))
                } else {
                    entry.content.clone()
                };
                output.push_str(&format!(
                    "    L{}: {}\n",
                    entry.line, content.trim()
                ));
            }
        }
    }

    output
}

/// Format merged blocks from ripgrep search as compact TOON
fn format_merged_blocks(query: &str, blocks: &[MergedBlock], search_path: &std::path::Path) -> String {
    let mut output = String::new();
    output.push_str("_type: raw_search_results\n");
    output.push_str(&format!("pattern: \"{}\"\n", query));
    output.push_str(&format!("blocks: {}\n", blocks.len()));

    let total_matches: usize = blocks.iter().map(|b| b.match_count).sum();
    output.push_str(&format!("total_matches: {}\n", total_matches));

    if blocks.is_empty() {
        output.push_str("results: (none)\n");
    } else {
        output.push_str("\n");
        for block in blocks {
            let file = block.file.strip_prefix(search_path)
                .unwrap_or(&block.file)
                .to_string_lossy();
            output.push_str(&format!("## {} (lines {}-{})\n", file, block.start_line, block.end_line));
            for line in &block.lines {
                let marker = if line.is_match { ">" } else { " " };
                output.push_str(&format!("{}{:4}: {}\n", marker, line.line, line.content));
            }
            output.push_str("\n");
        }
    }

    output
}

/// Format module symbols listing as compact TOON
fn format_module_symbols(module: &str, results: &[SymbolIndexEntry], cache: &CacheDir) -> String {
    let mut output = String::new();
    output.push_str("_type: module_symbols\n");
    output.push_str(&format!("module: \"{}\"\n", module));
    output.push_str(&format!("total: {}\n", results.len()));

    if results.is_empty() {
        let available = cache.list_modules();
        output.push_str("symbols: (none)\n");
        output.push_str(&format!("hint: available modules are: {}\n", available.join(", ")));
    } else {
        output.push_str(&format!("symbols[{}]{{s,h,k,f,l,r}}:\n", results.len()));
        for entry in results {
            output.push_str(&format!(
                "  {},{},{},{},{},{}\n",
                entry.symbol, entry.hash, entry.kind, entry.file, entry.lines, entry.risk
            ));
        }
    }

    output
}

// ============================================================================
// Duplicate Detection Helpers
// ============================================================================

use crate::duplicate::{DuplicateCluster, DuplicateMatch};
use std::io::{BufRead, BufReader};

/// Load function signatures from the signature index
fn load_signatures(cache: &CacheDir) -> Result<Vec<FunctionSignature>, String> {
    let sig_path = cache.signature_index_path();
    if !sig_path.exists() {
        return Err("Signature index not found".to_string());
    }

    let file = fs::File::open(&sig_path)
        .map_err(|e| format!("Failed to open signature index: {}", e))?;
    let reader = BufReader::new(file);

    let mut signatures = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<FunctionSignature>(&line) {
            Ok(sig) => signatures.push(sig),
            Err(e) => {
                // Skip malformed lines but log for debugging
                tracing::warn!("Skipping malformed signature: {}", e);
            }
        }
    }

    Ok(signatures)
}

/// Format call graph with pagination
fn format_call_graph_paginated(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
    offset: usize,
    limit: usize,
) -> String {
    use crate::schema::SCHEMA_VERSION;

    let mut output = String::new();
    output.push_str("_type: call_graph\n");
    output.push_str(&format!("schema_version: \"{}\"\n", SCHEMA_VERSION));
    output.push_str(&format!("total_edges: {}\n", total_edges));
    output.push_str(&format!("filtered_edges: {}\n", filtered_count));
    output.push_str(&format!("showing: {}\n", edges.len()));
    output.push_str(&format!("offset: {}\n", offset));
    output.push_str(&format!("limit: {}\n", limit));

    // Pagination hint
    if offset + edges.len() < filtered_count {
        output.push_str(&format!(
            "next_offset: {} (use offset={} to get next page)\n",
            offset + edges.len(),
            offset + limit
        ));
    }

    if edges.is_empty() {
        output.push_str("message: No edges match the filter criteria.\n");
        return output;
    }

    output.push_str("\n");

    for (caller, callees) in edges {
        let callees_str = callees.iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(",");
        output.push_str(&format!("{}: [{}]\n", caller, callees_str));
    }

    output
}

/// Format call graph summary (statistics only, no edges)
fn format_call_graph_summary(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
) -> String {
    use crate::schema::SCHEMA_VERSION;
    use std::collections::HashMap;

    let mut output = String::new();
    output.push_str("_type: call_graph_summary\n");
    output.push_str(&format!("schema_version: \"{}\"\n", SCHEMA_VERSION));
    output.push_str(&format!("total_edges: {}\n", total_edges));
    output.push_str(&format!("filtered_edges: {}\n", filtered_count));

    // Calculate statistics
    let total_calls: usize = edges.iter().map(|(_, callees)| callees.len()).sum();
    let avg_calls = if edges.is_empty() { 0.0 } else { total_calls as f64 / edges.len() as f64 };
    let max_calls = edges.iter().map(|(_, callees)| callees.len()).max().unwrap_or(0);

    output.push_str(&format!("total_calls: {}\n", total_calls));
    output.push_str(&format!("avg_calls_per_symbol: {:.1}\n", avg_calls));
    output.push_str(&format!("max_calls_in_symbol: {}\n", max_calls));

    // Top callers (symbols that make the most calls)
    let mut callers_by_count: Vec<_> = edges.iter()
        .map(|(caller, callees)| (caller.clone(), callees.len()))
        .collect();
    callers_by_count.sort_by(|a, b| b.1.cmp(&a.1));

    output.push_str("\ntop_callers[10]:\n");
    for (caller, count) in callers_by_count.iter().take(10) {
        output.push_str(&format!("  - {} (calls: {})\n", caller, count));
    }

    // Top callees (most called symbols)
    let mut callee_counts: HashMap<String, usize> = HashMap::new();
    for (_, callees) in edges {
        for callee in callees {
            *callee_counts.entry(callee.clone()).or_insert(0) += 1;
        }
    }
    let mut callees_by_count: Vec<_> = callee_counts.into_iter().collect();
    callees_by_count.sort_by(|a, b| b.1.cmp(&a.1));

    output.push_str("\ntop_callees[10]:\n");
    for (callee, count) in callees_by_count.iter().take(10) {
        output.push_str(&format!("  - {} (called: {} times)\n", callee, count));
    }

    // Leaf functions (call nothing)
    let leaf_count = edges.iter().filter(|(_, callees)| callees.is_empty()).count();
    output.push_str(&format!("\nleaf_functions: {}\n", leaf_count));

    output
}

/// Format duplicate clusters as compact TOON output
fn format_duplicate_clusters(clusters: &[DuplicateCluster], threshold: f64) -> String {
    format_duplicate_clusters_paginated(clusters, threshold, clusters.len(), 0, clusters.len(), "similarity")
}

/// Format duplicate clusters with pagination info
fn format_duplicate_clusters_paginated(
    clusters: &[DuplicateCluster],
    threshold: f64,
    total_clusters: usize,
    offset: usize,
    limit: usize,
    sort_by: &str,
) -> String {
    let mut output = String::new();
    output.push_str("_type: duplicate_results\n");
    output.push_str(&format!("threshold: {:.2}\n", threshold));
    output.push_str(&format!("total_clusters: {}\n", total_clusters));
    output.push_str(&format!("showing: {}\n", clusters.len()));
    output.push_str(&format!("offset: {}\n", offset));
    output.push_str(&format!("limit: {}\n", limit));
    output.push_str(&format!("sort_by: {}\n", sort_by));

    // Pagination hint
    if offset + clusters.len() < total_clusters {
        output.push_str(&format!(
            "next_offset: {} (use offset={} to get next page)\n",
            offset + clusters.len(),
            offset + limit
        ));
    }

    if clusters.is_empty() {
        if total_clusters == 0 {
            output.push_str("message: No duplicate clusters found above threshold.\n");
        } else {
            output.push_str(&format!(
                "message: No clusters at offset {}. Total clusters: {}.\n",
                offset, total_clusters
            ));
        }
        return output;
    }

    // Count total duplicates in this page
    let page_duplicates: usize = clusters.iter()
        .map(|c| c.duplicates.len())
        .sum();
    output.push_str(&format!("page_duplicates: {}\n\n", page_duplicates));

    for (i, cluster) in clusters.iter().enumerate() {
        output.push_str(&format!("cluster[{}]:\n", offset + i + 1));
        output.push_str(&format!("  primary: {} ({})\n", cluster.primary.name, cluster.primary.file));
        output.push_str(&format!("  hash: {}\n", cluster.primary.hash));
        if cluster.primary.start_line > 0 {
            output.push_str(&format!("  lines: {}-{}\n", cluster.primary.start_line, cluster.primary.end_line));
        }
        output.push_str(&format!("  duplicates[{}]:\n", cluster.duplicates.len()));

        for dup in &cluster.duplicates {
            let kind_str = match dup.kind {
                crate::duplicate::DuplicateKind::Exact => "exact",
                crate::duplicate::DuplicateKind::Near => "near",
                crate::duplicate::DuplicateKind::Divergent => "divergent",
            };
            output.push_str(&format!(
                "    - {} ({}) [{} {:.0}%]\n",
                dup.symbol.name, dup.symbol.file, kind_str, dup.similarity * 100.0
            ));

            // Show differences for near/divergent matches
            if !dup.differences.is_empty() && dup.differences.len() <= 3 {
                for diff in &dup.differences {
                    output.push_str(&format!("      {}\n", diff));
                }
            }
        }
        output.push_str("\n");
    }

    output
}

/// Format duplicate matches for a single symbol as compact TOON output
fn format_duplicate_matches(
    symbol_name: &str,
    symbol_file: &str,
    matches: &[DuplicateMatch],
    threshold: f64,
) -> String {
    let mut output = String::new();
    output.push_str("_type: duplicate_check\n");
    output.push_str(&format!("symbol: {}\n", symbol_name));
    output.push_str(&format!("file: {}\n", symbol_file));
    output.push_str(&format!("threshold: {:.2}\n", threshold));
    output.push_str(&format!("matches: {}\n", matches.len()));

    if matches.is_empty() {
        output.push_str("message: No duplicates found for this symbol.\n");
        return output;
    }

    output.push_str("\nsimilar_functions:\n");
    for m in matches {
        let kind_str = match m.kind {
            crate::duplicate::DuplicateKind::Exact => "EXACT",
            crate::duplicate::DuplicateKind::Near => "NEAR",
            crate::duplicate::DuplicateKind::Divergent => "DIVERGENT",
        };
        output.push_str(&format!(
            "  - {} ({})\n    similarity: {:.0}% [{}]\n",
            m.symbol.name, m.symbol.file, m.similarity * 100.0, kind_str
        ));
        if m.symbol.start_line > 0 {
            output.push_str(&format!("    lines: {}-{}\n", m.symbol.start_line, m.symbol.end_line));
        }
        output.push_str(&format!("    hash: {}\n", m.symbol.hash));

        // Show differences
        if !m.differences.is_empty() {
            output.push_str("    differences:\n");
            for diff in &m.differences {
                output.push_str(&format!("      - {}\n", diff));
            }
        }
    }

    output
}

// ============================================================================
// Instructions
// ============================================================================

const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Purpose
Produces highly compressed semantic summaries in TOON format, enabling efficient code review without reading entire files. Supports both on-demand analysis and pre-built sharded indexes for massive repositories.

## IMPORTANT: Use Tools Instead of Direct File Access

**Do NOT use direct file reads (Read tool, cat, etc.) when this MCP server is available.**

All code analysis and exploration should use these MCP tools:
- `analyze_diff` - For reviewing changes
- `search_symbols` - For finding code by name
- `list_symbols` - For browsing module contents
- `get_symbol` - For detailed semantic info
- `get_symbol_source` - For viewing actual source code (surgical read)
- `get_repo_overview` - For architecture understanding

Direct file reads waste tokens and bypass the semantic compression that makes large codebases manageable. Use `get_symbol_source` when you need actual code.

## Quick Start - Query-Driven Workflow (RECOMMENDED)

For token-efficient exploration, use the **query-driven workflow**:

1. **First time**: Run `generate_index` to create the index
2. **Get overview**: Call `get_repo_overview` to understand architecture
3. **Search**: Use `search_symbols("login")` to find relevant symbols (~400 tokens for 20 results)
4. **Browse module**: Use `list_symbols("auth")` for lightweight module listing (~800 tokens for 50 results)
5. **Deep dive**: Use `get_symbol(hash)` for specific symbols (~350 tokens each)
6. **Get code**: Use `get_symbol_source(...)` for actual source code (~400 tokens)

**Token budget per query:**
- search_symbols: ~400 tokens (20 results)
- list_symbols: ~800 tokens (50 results)
- get_symbol: ~350 tokens
- get_symbol_source: ~400 tokens (50 lines)

## Tools

### Query-Driven API (Most token-efficient)
- **search_symbols**: Search for symbols by name across repository. Returns lightweight entries only.
- **list_symbols**: List all symbols in a module. Returns lightweight entries only.
- **get_symbol**: Get detailed semantic info for a specific symbol by hash.
- **get_symbol_source**: Get actual source code with line numbers (surgical read).

### Sharded Index (Full module access)
- **get_repo_overview**: Get high-level architecture summary
- **list_modules**: List available module shards
- **get_module**: Get ALL symbols in a module (expensive - prefer list_symbols)
- **get_call_graph**: Get function call relationships
- **generate_index**: Create/regenerate the sharded index

### On-Demand Analysis (For small repos or quick checks)
- **analyze_file**: Analyze a single source file
- **analyze_directory**: Analyze entire codebase
- **analyze_diff**: Compare git branches/commits, or analyze uncommitted changes
- **list_languages**: Show supported programming languages

### Test Runner (VALIDATE phase)
- **run_tests**: Run tests with auto-detected framework (pytest, cargo, npm, go). Returns structured results.
- **detect_tests**: Detect test frameworks in a project. Useful for monorepos.

### Analyzing Uncommitted Changes
To review uncommitted changes (staged + unstaged), use:
```
analyze_diff(base_ref="HEAD", target_ref="WORKING")
```
This compares your working tree against HEAD, showing all uncommitted modifications.

## Workflow Examples

### Token-efficient exploration (RECOMMENDED)
```
1. get_repo_overview           → Understand architecture
2. search_symbols("login")     → Find login-related symbols (~400 tokens)
3. list_symbols("auth")        → Browse auth module (~800 tokens)
4. get_symbol("abc123")        → Get details for specific symbol (~350 tokens)
5. get_symbol_source(...)      → Get actual code to edit (~400 tokens)
TOTAL: ~2,000 tokens
```

### Legacy workflow (more expensive)
```
1. get_repo_overview           → Understand architecture
2. get_module("auth")          → Load FULL module (~8,000 tokens)
3. get_module("components")    → Load FULL module (~10,000 tokens)
TOTAL: ~20,000 tokens
```

AVOID:
- `get_module` when you only need a few symbols (use search_symbols + get_symbol)
- `analyze_diff` without filters for large diffs

## Output Fields
- symbol: Primary function/class/component name
- symbol_kind: function|component|class|struct|trait|enum
- behavioral_risk: low|medium|high (based on complexity and I/O)
- added_dependencies: Imports and dependencies
- state_changes: Variables with {name, type, initializer}
- control_flow: List of if/for/while/match/try constructs
- calls: Deduplicated function calls with await/try context
- insertions: Semantic descriptions (e.g., "Next.js API route (GET)")

## Code Review Guidelines
When reviewing code using analyze_diff output:

1. **Security Review** (behavioral_risk: high)
   - Check path traversal in file operations
   - Validate user input handling
   - Review authentication/authorization patterns
   - Identify SQL injection, XSS, command injection risks

2. **Quality Review**
   - Consistent error handling (calls with try: Y are wrapped)
   - Proper async patterns (calls with await: Y)
   - State management complexity (state_changes count)
   - Control flow complexity (control_flow patterns)

3. **Architecture Review**
   - Module dependencies (added_dependencies)
   - Public API changes (public_surface_changed)
   - Framework patterns (insertions describe detected patterns)

4. **Action Items**
   - For high-risk files: Use `get_symbol_source` to view specific code sections
   - For medium-risk: Note concerns, suggest improvements
   - For low-risk: Approve or note minor style issues

Act as a senior/staff engineer focused on production readiness. Provide actionable feedback with specific file:line references where possible.

## Remember
- NEVER use direct file reads when MCP tools are available
- Always prefer `get_symbol_source` over reading entire files
- Use `search_symbols` to find relevant code instead of grep
- Use `analyze_diff` for code reviews instead of reading raw diffs"#;

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
