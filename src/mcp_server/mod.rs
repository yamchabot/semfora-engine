//! MCP Server for semfora-mcp
//!
//! This module provides an MCP (Model Context Protocol) server that exposes
//! the semantic code analysis capabilities of semfora-mcp as tools that can be
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
    encode_toon, encode_toon_directory, generate_repo_overview, Lang, SemanticSummary,
    CacheDir, ShardWriter, SymbolIndexEntry,
};

// Re-export types for external use
pub use types::*;
use helpers::{check_cache_staleness, collect_files, parse_and_extract};

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
        }
    }

    /// Create a new MCP server with a specific working directory
    pub fn with_working_dir(working_dir: PathBuf) -> Self {
        Self {
            working_dir: Arc::new(Mutex::new(working_dir)),
            tool_router: Self::tool_router(),
        }
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

    #[tool(description = "Analyze changes between git branches or commits. Shows semantic diff of what changed, including new symbols, modified functions, changed dependencies, and risk assessment for each changed file. Use target_ref='WORKING' to analyze uncommitted changes.")]
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

    #[tool(description = "List all programming languages supported by semfora-mcp for semantic analysis")]
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

    #[tool(description = "Get the repository overview from a pre-built sharded index. Returns a compact summary (~300KB even for massive repos) with framework detection, module list, risk breakdown, and entry points. Use this FIRST to understand a codebase before diving into specific modules.")]
    async fn get_repo_overview(
        &self,
        Parameters(request): Parameters<GetRepoOverviewRequest>,
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

        let overview_path = cache.repo_overview_path();
        if !overview_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run `semfora-mcp --shard {}` to generate one, or use generate_index tool.",
                repo_path.display(), repo_path.display()
            ))]));
        }

        match fs::read_to_string(&overview_path) {
            Ok(content) => {
                let staleness_warning = check_cache_staleness(&cache);
                let output = match staleness_warning {
                    Some(warning) => format!("{}\n\n{}", warning, content),
                    None => content,
                };
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

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}", repo_path.display()
            ))]));
        }

        let modules = cache.list_modules();
        if modules.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No modules found in index.")]));
        }

        let mut output = format!("Available modules ({}):\n", modules.len());
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

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

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

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

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

        let mut shard_writer = match ShardWriter::new(&dir_path) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to initialize shard writer: {}", e
            ))])),
        };

        let files = collect_files(&dir_path, max_depth, &[]);

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No supported files found in {}", dir_path.display()
            ))]));
        }

        let (summaries, total_bytes) = analyze_files_with_stats(&files);

        shard_writer.add_summaries(summaries.clone());

        let dir_str = dir_path.display().to_string();
        let stats = match shard_writer.write_all(&dir_str) {
            Ok(s) => s,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to write shards: {}", e
            ))])),
        };

        let compression = if total_bytes > 0 {
            ((total_bytes as f64 - stats.total_bytes() as f64) / total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let output = format!(
            "Sharded index created for: {}\n\
             Cache: {}\n\n\
             Files analyzed: {}\n\
             Modules: {}\n\
             Symbols: {}\n\
             Compression: {:.1}%\n\n\
             Use get_repo_overview to see the high-level architecture.",
            dir_path.display(),
            shard_writer.cache_path().display(),
            summaries.len(),
            stats.modules_written,
            stats.symbols_written,
            compression
        );

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Get the call graph showing which functions call which other functions. Returns a mapping of symbol -> [called symbols] that can be used to understand code flow and impact radius of changes.")]
    async fn get_call_graph(
        &self,
        Parameters(request): Parameters<GetCallGraphRequest>,
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

        let call_graph_path = cache.call_graph_path();
        if !call_graph_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Call graph not found in index. The index may need to be regenerated."
            )]));
        }

        match fs::read_to_string(&call_graph_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read call graph: {}", e
            ))])),
        }
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

    #[tool(description = "Search for symbols by name across the repository. Returns lightweight index entries (symbol, hash, module, file, lines, risk) without full semantic details. Use get_symbol(hash) to fetch full details for specific symbols. This is much more token-efficient than get_module for targeted searches.")]
    async fn search_symbols(
        &self,
        Parameters(request): Parameters<SearchSymbolsRequest>,
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

        if !cache.has_symbol_index() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No symbol index found for {}. Run generate_index first to enable search.",
                repo_path.display()
            ))]));
        }

        let limit = request.limit.unwrap_or(20).min(100);

        let results = match cache.search_symbols(
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

        let output = format_search_results(&request.query, &results);
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

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to access cache: {}", e
            ))])),
        };

        if !cache.has_symbol_index() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No symbol index found for {}. Run generate_index first.",
                repo_path.display()
            ))]));
        }

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
}

#[tool_handler]
impl ServerHandler for McpDiffServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "semfora-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("MCP Semantic Diff".to_string()),
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

/// Analyze files and return summaries plus total bytes read
fn analyze_files_with_stats(files: &[PathBuf]) -> (Vec<SemanticSummary>, usize) {
    let mut summaries = Vec::new();
    let mut total_bytes = 0usize;

    for file_path in files {
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        total_bytes += source.len();

        if let Ok(summary) = parse_and_extract(file_path, &source, lang) {
            summaries.push(summary);
        }
    }

    (summaries, total_bytes)
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
        ("C", ".c, .h"),
        ("C++", ".cpp, .cc, .cxx, .hpp, .hxx, .hh"),
        ("HTML", ".html, .htm"),
        ("CSS", ".css"),
        ("JSON", ".json"),
        ("YAML", ".yaml, .yml"),
        ("TOML", ".toml"),
        ("Markdown", ".md, .markdown"),
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
        assert_eq!(info.server_info.name, "semfora-mcp");
    }
}
