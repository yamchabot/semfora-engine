//! MCP Server for mcp-diff
//!
//! This module provides an MCP (Model Context Protocol) server that exposes
//! the semantic code analysis capabilities of mcp-diff as tools that can be
//! called by AI assistants like Claude.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{
    encode_toon, encode_toon_directory, extract, generate_repo_overview, Lang, McpDiffError,
    SemanticSummary, CacheDir, ShardWriter,
};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to analyze a single file
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeFileRequest {
    /// The absolute or relative path to the file to analyze
    #[schemars(description = "Path to the source file to analyze")]
    pub path: String,

    /// Output format: "toon" (default) or "json"
    #[schemars(description = "Output format: 'toon' (compact) or 'json' (structured)")]
    pub format: Option<String>,
}

/// Request to analyze a directory
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDirectoryRequest {
    /// The path to the directory to analyze
    #[schemars(description = "Path to the directory to analyze")]
    pub path: String,

    /// Maximum depth for recursive analysis (default: 10)
    #[schemars(description = "Maximum directory depth to traverse (default: 10)")]
    pub max_depth: Option<usize>,

    /// Whether to include only the summary overview
    #[schemars(description = "If true, only return the repository overview, not individual files")]
    pub summary_only: Option<bool>,

    /// File extensions to include (e.g., ["ts", "tsx", "js"])
    #[schemars(description = "File extensions to include (e.g., ['ts', 'tsx']). If empty, all supported extensions are included.")]
    pub extensions: Option<Vec<String>>,
}

/// Request to analyze git diff
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDiffRequest {
    /// The base branch or commit to compare against (e.g., "main", "HEAD~1")
    #[schemars(description = "Base branch or commit to compare against (e.g., 'main', 'HEAD~1')")]
    pub base_ref: String,

    /// The target branch or commit (defaults to HEAD)
    #[schemars(description = "Target branch or commit (defaults to 'HEAD')")]
    pub target_ref: Option<String>,

    /// Working directory (defaults to current directory)
    #[schemars(description = "Working directory for git operations (defaults to current directory)")]
    pub working_dir: Option<String>,
}

/// Request to list supported languages
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLanguagesRequest {}

/// Request to get repository overview from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetRepoOverviewRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

/// Request to get a module from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetModuleRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Name of the module to retrieve (e.g., "api", "components", "lib")
    #[schemars(description = "Module name (e.g., 'api', 'components', 'lib', 'tests')")]
    pub module_name: String,
}

/// Request to get a symbol from sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSymbolRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,

    /// Symbol hash (from repo_overview or module listing)
    #[schemars(description = "Symbol hash from the repo overview or module shard")]
    pub symbol_hash: String,
}

/// Request to list modules in a sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListModulesRequest {
    /// Path to the repository (defaults to current directory)
    #[schemars(description = "Path to the repository root (defaults to current directory)")]
    pub path: Option<String>,
}

/// Request to generate/regenerate sharded index
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GenerateIndexRequest {
    /// Path to the repository to index
    #[schemars(description = "Path to the repository to index")]
    pub path: String,

    /// Maximum directory depth (default: 10)
    #[schemars(description = "Maximum directory depth for file collection (default: 10)")]
    pub max_depth: Option<usize>,
}

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
        // Use current directory, falling back to a sensible default if unavailable
        let working_dir = std::env::current_dir().unwrap_or_else(|_| {
            // Fallback to temp directory or root as last resort
            std::env::temp_dir()
        });
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
    /// Analyze a single source file and return semantic summary
    #[tool(description = "Analyze a source file and extract semantic information (symbols, imports, state, control flow). Returns a compact TOON or JSON summary that is much smaller than the original source code.")]
    async fn analyze_file(
        &self,
        Parameters(request): Parameters<AnalyzeFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = self.resolve_path(&request.path).await;

        // Check file exists
        if !file_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "File not found: {}",
                file_path.display()
            ))]));
        }

        // Detect language
        let lang = match Lang::from_path(&file_path) {
            Ok(l) => l,
            Err(_) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unsupported file type: {}",
                    file_path.display()
                ))]));
            }
        };

        // Read file
        let source = match fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file: {}",
                    e
                ))]));
            }
        };

        // Parse and extract
        let summary = match parse_and_extract(&file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Analysis failed: {}",
                    e
                ))]));
            }
        };

        // Format output
        let output = match request.format.as_deref() {
            Some("json") => {
                serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".to_string())
            }
            _ => encode_toon(&summary),
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Analyze a directory and return semantic summaries for all files
    #[tool(description = "Analyze all source files in a directory recursively. Returns a repository overview with framework detection, module grouping, and risk assessment, plus individual file summaries. The output is highly compressed compared to raw source code.")]
    async fn analyze_directory(
        &self,
        Parameters(request): Parameters<AnalyzeDirectoryRequest>,
    ) -> Result<CallToolResult, McpError> {
        let dir_path = self.resolve_path(&request.path).await;

        if !dir_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}",
                dir_path.display()
            ))]));
        }

        if !dir_path.is_dir() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Not a directory: {}",
                dir_path.display()
            ))]));
        }

        let max_depth = request.max_depth.unwrap_or(10);
        let summary_only = request.summary_only.unwrap_or(false);
        let extensions = request.extensions.unwrap_or_default();

        // Collect files
        let files = collect_files(&dir_path, max_depth, &extensions);

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "directory: {}\nfiles_found: 0\n",
                dir_path.display()
            ))]));
        }

        // Analyze all files
        let mut summaries: Vec<SemanticSummary> = Vec::new();

        for file_path in &files {
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

        // Generate output
        let dir_str = dir_path.display().to_string();
        let overview = generate_repo_overview(&summaries, &dir_str);

        let output = if summary_only {
            encode_toon_directory(&overview, &[])
        } else {
            encode_toon_directory(&overview, &summaries)
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Analyze git diff between branches
    #[tool(description = "Analyze changes between git branches or commits. Shows semantic diff of what changed, including new symbols, modified functions, changed dependencies, and risk assessment for each changed file.")]
    async fn analyze_diff(
        &self,
        Parameters(request): Parameters<AnalyzeDiffRequest>,
    ) -> Result<CallToolResult, McpError> {
        let working_dir = match &request.working_dir {
            Some(wd) => self.resolve_path(wd).await,
            None => self.working_dir.lock().await.clone(),
        };

        // Check if in git repo
        if !crate::git::is_git_repo(Some(&working_dir)) {
            return Ok(CallToolResult::error(vec![Content::text(
                "Not a git repository",
            )]));
        }

        let base_ref = &request.base_ref;
        let target_ref = request.target_ref.as_deref().unwrap_or("HEAD");

        // Get merge base for accurate diff
        let merge_base = crate::git::get_merge_base(base_ref, target_ref, Some(&working_dir))
            .unwrap_or_else(|_| base_ref.to_string());

        // Get changed files
        let changed_files =
            match crate::git::get_changed_files(&merge_base, target_ref, Some(&working_dir)) {
                Ok(files) => files,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to get changed files: {}",
                        e
                    ))]));
                }
            };

        if changed_files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No files changed.\n",
            )]));
        }

        let mut output = String::new();
        output.push_str(&format!(
            "diff: {} → {} ({} files)\n\n",
            base_ref,
            target_ref,
            changed_files.len()
        ));

        for changed_file in &changed_files {
            let full_path = working_dir.join(&changed_file.path);

            output.push_str(&format!(
                "━━━ {} [{}] ━━━\n",
                changed_file.path,
                changed_file.change_type.as_str()
            ));

            // Skip deleted files
            if changed_file.change_type == crate::git::ChangeType::Deleted {
                output.push_str("(file deleted)\n\n");
                continue;
            }

            // Try to analyze current version
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

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// List all supported programming languages
    #[tool(
        description = "List all programming languages supported by mcp-diff for semantic analysis"
    )]
    fn list_languages(
        &self,
        Parameters(_request): Parameters<ListLanguagesRequest>,
    ) -> Result<CallToolResult, McpError> {
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

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ========================================================================
    // Sharded Index Query Tools
    // ========================================================================

    /// Get repository overview from sharded index (small, high-level summary)
    #[tool(description = "Get the repository overview from a pre-built sharded index. Returns a compact summary (~300KB even for massive repos) with framework detection, module list, risk breakdown, and entry points. Use this FIRST to understand a codebase before diving into specific modules.")]
    async fn get_repo_overview(
        &self,
        Parameters(request): Parameters<GetRepoOverviewRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.working_dir.lock().await.clone(),
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]));
            }
        };

        let overview_path = cache.repo_overview_path();
        if !overview_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}. Run `mcp-diff --shard {}` to generate one, or use generate_index tool.",
                repo_path.display(),
                repo_path.display()
            ))]));
        }

        match fs::read_to_string(&overview_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read overview: {}",
                e
            ))])),
        }
    }

    /// List available modules in sharded index
    #[tool(description = "List all modules available in a repository's sharded index. Returns module names that can be queried with get_module.")]
    async fn list_modules(
        &self,
        Parameters(request): Parameters<ListModulesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.working_dir.lock().await.clone(),
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]));
            }
        };

        if !cache.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No sharded index found for {}",
                repo_path.display()
            ))]));
        }

        let modules = cache.list_modules();
        if modules.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No modules found in index.",
            )]));
        }

        let mut output = format!("Available modules ({}):\n", modules.len());
        for module in &modules {
            output.push_str(&format!("  - {}\n", module));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Get a specific module from sharded index
    #[tool(description = "Get detailed semantic information for a specific module (e.g., 'api', 'components', 'lib'). Returns all symbols in that module with their risk levels, dependencies, and function calls. Use after get_repo_overview to drill down into specific areas.")]
    async fn get_module(
        &self,
        Parameters(request): Parameters<GetModuleRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.working_dir.lock().await.clone(),
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]));
            }
        };

        let module_path = cache.module_path(&request.module_name);
        if !module_path.exists() {
            let available = cache.list_modules();
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Module '{}' not found. Available modules: {}",
                request.module_name,
                available.join(", ")
            ))]));
        }

        match fs::read_to_string(&module_path) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read module: {}",
                e
            ))])),
        }
    }

    /// Get a specific symbol from sharded index
    #[tool(description = "Get detailed semantic information for a specific symbol by its hash. Symbol hashes are found in the repo_overview or module shards. Returns the complete semantic summary including all calls, state changes, and control flow.")]
    async fn get_symbol(
        &self,
        Parameters(request): Parameters<GetSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        let repo_path = match &request.path {
            Some(p) => self.resolve_path(p).await,
            None => self.working_dir.lock().await.clone(),
        };

        let cache = match CacheDir::for_repo(&repo_path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to access cache: {}",
                    e
                ))]));
            }
        };

        let symbol_path = cache.symbol_path(&request.symbol_hash);
        if !symbol_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Symbol '{}' not found in index",
                request.symbol_hash
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

    /// Generate or regenerate sharded index for a repository
    #[tool(description = "Generate a sharded semantic index for a repository. This creates a queryable cache with repo_overview, module shards, symbol shards, and dependency graphs. Run this once for a repo, then use get_repo_overview/get_module/get_symbol for fast queries.")]
    async fn generate_index(
        &self,
        Parameters(request): Parameters<GenerateIndexRequest>,
    ) -> Result<CallToolResult, McpError> {
        let dir_path = self.resolve_path(&request.path).await;

        if !dir_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Directory not found: {}",
                dir_path.display()
            ))]));
        }

        if !dir_path.is_dir() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Not a directory: {}",
                dir_path.display()
            ))]));
        }

        let max_depth = request.max_depth.unwrap_or(10);

        // Create shard writer
        let mut shard_writer = match ShardWriter::new(&dir_path) {
            Ok(w) => w,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to initialize shard writer: {}",
                    e
                ))]));
            }
        };

        // Collect files
        let files = collect_files(&dir_path, max_depth, &[]);

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No supported files found in {}",
                dir_path.display()
            ))]));
        }

        // Analyze all files
        let mut summaries: Vec<SemanticSummary> = Vec::new();
        let mut total_bytes = 0usize;

        for file_path in &files {
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

        // Add summaries and write shards
        shard_writer.add_summaries(summaries.clone());

        let dir_str = dir_path.display().to_string();
        let stats = match shard_writer.write_all(&dir_str) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to write shards: {}",
                    e
                ))]));
            }
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
}

#[tool_handler]
impl ServerHandler for McpDiffServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "mcp-diff".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("MCP Semantic Diff".to_string()),
                website_url: None,
                icons: None,
            },
            instructions: Some(MCP_INSTRUCTIONS.to_string()),
        }
    }
}

/// Instructions for AI agents using the mcp-diff tools
const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Purpose
Produces highly compressed semantic summaries in TOON format, enabling efficient code review without reading entire files. Supports both on-demand analysis and pre-built sharded indexes for massive repositories.

## Quick Start for Large Codebases

For repositories over 100 files, use the **sharded index workflow**:

1. **First time**: Run `generate_index` to create the index (or CLI: `mcp-diff --shard <path>`)
2. **Get overview**: Call `get_repo_overview` to understand architecture (~300KB even for 2GB repos)
3. **Drill down**: Use `get_module` to explore specific areas (api, components, tests, etc.)
4. **Deep dive**: Use `get_symbol` for individual function/class details

## Tools

### Sharded Index (Recommended for large repos)
- **get_repo_overview**: Get high-level architecture summary (framework, modules, risk breakdown, entry points)
- **list_modules**: List available module shards (api, components, lib, tests, etc.)
- **get_module**: Get all symbols in a specific module
- **get_symbol**: Get detailed info for a specific symbol by hash
- **generate_index**: Create/regenerate the sharded index

### On-Demand Analysis (For small repos or quick checks)
- **analyze_file**: Analyze a single source file
- **analyze_directory**: Analyze entire codebase (may be large for big repos)
- **analyze_diff**: Compare git branches/commits for code review
- **list_languages**: Show supported programming languages

## Workflow Examples

### Understanding a new codebase
```
1. get_repo_overview           → See framework, patterns, module breakdown
2. get_module("api")           → Examine API routes
3. get_module("components")    → Examine UI components
4. get_symbol("abc123")        → Deep dive on specific function
```

### Code review workflow
```
1. analyze_diff(base="main")   → See what changed
2. get_module("api")           → If API changed, get full context
3. Read high-risk files        → Use standard file read for details
```

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
   - For high-risk files: Request full file read for detailed review
   - For medium-risk: Note concerns, suggest improvements
   - For low-risk: Approve or note minor style issues

Act as a senior/staff engineer focused on production readiness. Provide actionable feedback with specific file:line references where possible."#;

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse source and extract semantic summary
fn parse_and_extract(
    file_path: &Path,
    source: &str,
    lang: Lang,
) -> Result<SemanticSummary, McpDiffError> {
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

/// Recursively collect supported files from a directory
fn collect_files(dir: &Path, max_depth: usize, extensions: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, max_depth, 0, extensions, &mut files);
    files
}

fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    extensions: &[String],
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
                || name == "vendor"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, current_depth + 1, extensions, files);
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Check extension filter if provided
                if !extensions.is_empty() && !extensions.iter().any(|e| e == ext) {
                    continue;
                }

                // Check if language is supported
                if Lang::from_extension(ext).is_ok() {
                    files.push(path);
                }
            }
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
        assert_eq!(info.server_info.name, "mcp-diff");
    }
}
