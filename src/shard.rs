//! Sharded IR writer for massive repository support
//!
//! Splits semantic analysis output into queryable shards:
//! - repo_overview.toon - High-level architecture
//! - modules/{name}.toon - Per-module semantic slices
//! - symbols/{hash}.toon - Individual symbol details
//! - graphs/*.toon - Dependency and call graphs

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::cache::{CacheDir, IndexingStatus, SourceFileInfo};
use crate::error::Result;
use crate::schema::{RepoOverview, RiskLevel, SemanticSummary, SCHEMA_VERSION};
use crate::toon::{encode_toon, generate_repo_overview};

/// Write sharded IR output for a repository
pub struct ShardWriter {
    /// Cache directory manager
    cache: CacheDir,

    /// Summaries organized by module
    modules: HashMap<String, Vec<SemanticSummary>>,

    /// All summaries for graph building
    all_summaries: Vec<SemanticSummary>,

    /// Repository overview
    overview: Option<RepoOverview>,

    /// Indexing progress
    progress: IndexingStatus,
}

impl ShardWriter {
    /// Create a new shard writer for a repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let cache = CacheDir::for_repo(repo_path)?;
        cache.init()?;

        Ok(Self {
            cache,
            modules: HashMap::new(),
            all_summaries: Vec::new(),
            overview: None,
            progress: IndexingStatus::default(),
        })
    }

    /// Add summaries to be sharded
    pub fn add_summaries(&mut self, summaries: Vec<SemanticSummary>) {
        // Organize by module
        for summary in &summaries {
            let module_name = extract_module_name(&summary.file);
            self.modules
                .entry(module_name)
                .or_insert_with(Vec::new)
                .push(summary.clone());
        }

        self.all_summaries.extend(summaries);
    }

    /// Generate and write all shards
    pub fn write_all(&mut self, dir_path: &str) -> Result<ShardStats> {
        let mut stats = ShardStats::default();

        // Generate overview first (fast, gives agents something to work with)
        self.write_repo_overview(dir_path, &mut stats)?;

        // Write module shards
        self.write_module_shards(&mut stats)?;

        // Write symbol shards
        self.write_symbol_shards(&mut stats)?;

        // Write graph shards
        self.write_graph_shards(&mut stats)?;

        // Write symbol index (query-driven API v1)
        self.write_symbol_index(&mut stats)?;

        Ok(stats)
    }

    /// Write the repository overview
    fn write_repo_overview(&mut self, dir_path: &str, stats: &mut ShardStats) -> Result<()> {
        let overview = generate_repo_overview(&self.all_summaries, dir_path);
        self.overview = Some(overview.clone());

        // Create TOON output with metadata
        let toon = encode_repo_overview_with_meta(&overview, &self.progress);

        // Write to cache
        let path = self.cache.repo_overview_path();
        let mut file = fs::File::create(&path)?;
        file.write_all(toon.as_bytes())?;

        stats.overview_bytes = toon.len();
        stats.files_written += 1;

        Ok(())
    }

    /// Write per-module shards
    fn write_module_shards(&self, stats: &mut ShardStats) -> Result<()> {
        for (module_name, summaries) in &self.modules {
            let toon = encode_module_shard(module_name, summaries, &self.cache.repo_root);
            let path = self.cache.module_path(module_name);

            let mut file = fs::File::create(&path)?;
            file.write_all(toon.as_bytes())?;

            stats.module_bytes += toon.len();
            stats.modules_written += 1;
        }

        stats.files_written += stats.modules_written;
        Ok(())
    }

    /// Write per-symbol shards
    fn write_symbol_shards(&self, stats: &mut ShardStats) -> Result<()> {
        for summary in &self.all_summaries {
            if let Some(ref symbol_id) = summary.symbol_id {
                let toon = encode_symbol_shard(summary);
                let path = self.cache.symbol_path(&symbol_id.hash);

                let mut file = fs::File::create(&path)?;
                file.write_all(toon.as_bytes())?;

                stats.symbol_bytes += toon.len();
                stats.symbols_written += 1;
            }
        }

        stats.files_written += stats.symbols_written;
        Ok(())
    }

    /// Write graph shards (call graph, import graph, module graph)
    fn write_graph_shards(&self, stats: &mut ShardStats) -> Result<()> {
        // Build and write call graph
        let call_graph = build_call_graph(&self.all_summaries);
        let call_graph_toon = encode_call_graph(&call_graph);
        fs::write(self.cache.call_graph_path(), &call_graph_toon)?;
        stats.graph_bytes += call_graph_toon.len();

        // Build and write import graph
        let import_graph = build_import_graph(&self.all_summaries);
        let import_graph_toon = encode_import_graph(&import_graph);
        fs::write(self.cache.import_graph_path(), &import_graph_toon)?;
        stats.graph_bytes += import_graph_toon.len();

        // Build and write module graph
        let module_graph = build_module_graph(&self.modules);
        let module_graph_toon = encode_module_graph(&module_graph);
        fs::write(self.cache.module_graph_path(), &module_graph_toon)?;
        stats.graph_bytes += module_graph_toon.len();

        stats.files_written += 3;
        Ok(())
    }

    /// Write the lightweight symbol index for query-driven access
    fn write_symbol_index(&self, stats: &mut ShardStats) -> Result<()> {
        use crate::cache::SymbolIndexEntry;

        let path = self.cache.symbol_index_path();
        let mut file = fs::File::create(&path)?;

        for summary in &self.all_summaries {
            if let Some(ref symbol_id) = summary.symbol_id {
                let entry = SymbolIndexEntry {
                    symbol: summary.symbol.clone().unwrap_or_default(),
                    hash: symbol_id.hash.clone(),
                    kind: summary.symbol_kind
                        .map(|k| format!("{:?}", k).to_lowercase())
                        .unwrap_or_else(|| "unknown".to_string()),
                    module: extract_module_name(&summary.file),
                    file: summary.file.clone(),
                    lines: match (summary.start_line, summary.end_line) {
                        (Some(s), Some(e)) => format!("{}-{}", s, e),
                        (Some(s), None) => format!("{}", s),
                        _ => String::new(),
                    },
                    risk: format!("{:?}", summary.behavioral_risk).to_lowercase(),
                };

                // Write as JSONL (one JSON object per line)
                let json = serde_json::to_string(&entry)
                    .map_err(|e| crate::McpDiffError::ExtractionFailure {
                        message: format!("Failed to serialize symbol index entry: {}", e),
                    })?;
                writeln!(file, "{}", json)?;

                stats.index_entries += 1;
            }
        }

        stats.index_bytes = fs::metadata(&path)?.len() as usize;
        stats.files_written += 1;
        Ok(())
    }

    /// Get the cache directory path
    pub fn cache_path(&self) -> &Path {
        &self.cache.root
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, usize) {
        (self.cache.size(), self.modules.len())
    }
}

/// Statistics about the sharding operation
#[derive(Debug, Default)]
pub struct ShardStats {
    /// Total files written
    pub files_written: usize,

    /// Bytes written for overview
    pub overview_bytes: usize,

    /// Number of module shards
    pub modules_written: usize,

    /// Bytes written for modules
    pub module_bytes: usize,

    /// Number of symbol shards
    pub symbols_written: usize,

    /// Bytes written for symbols
    pub symbol_bytes: usize,

    /// Bytes written for graphs
    pub graph_bytes: usize,

    /// Number of entries in symbol index
    pub index_entries: usize,

    /// Bytes written for symbol index
    pub index_bytes: usize,
}

impl ShardStats {
    /// Total bytes written
    pub fn total_bytes(&self) -> usize {
        self.overview_bytes + self.module_bytes + self.symbol_bytes + self.graph_bytes + self.index_bytes
    }
}

// ============================================================================
// Encoding Functions
// ============================================================================

/// Encode repository overview with metadata
fn encode_repo_overview_with_meta(overview: &RepoOverview, progress: &IndexingStatus) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: repo_overview"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));

    if let Some(ref framework) = overview.framework {
        lines.push(format!("framework: \"{}\"", framework));
    }

    if let Some(ref database) = overview.database {
        lines.push(format!("database: \"{}\"", database));
    }

    // Patterns
    if !overview.patterns.is_empty() {
        let patterns: Vec<String> = overview.patterns.iter().map(|p| format!("\"{}\"", p)).collect();
        lines.push(format!("patterns[{}]: {}", overview.patterns.len(), patterns.join(",")));
    }

    // Modules summary
    if !overview.modules.is_empty() {
        lines.push(format!("modules[{}]{{name,purpose,files,risk}}:", overview.modules.len()));
        for m in &overview.modules {
            lines.push(format!(
                "  {},\"{}\",{},{}",
                m.name,
                m.purpose,
                m.file_count,
                m.risk.as_str()
            ));
        }
    }

    // Stats
    lines.push(format!("files: {}", overview.stats.total_files));
    lines.push(format!(
        "risk_breakdown: \"high:{},medium:{},low:{}\"",
        overview.stats.high_risk, overview.stats.medium_risk, overview.stats.low_risk
    ));

    // Entry points
    if !overview.entry_points.is_empty() {
        let entries: Vec<String> = overview.entry_points.iter().map(|e| e.to_string()).collect();
        lines.push(format!("entry_points[{}]: {}", entries.len(), entries.join(",")));
    }

    // Indexing status (if in progress)
    if progress.in_progress {
        lines.push(format!("indexing_status:"));
        lines.push(format!("  in_progress: true"));
        lines.push(format!("  files_indexed: {}", progress.files_indexed));
        lines.push(format!("  files_total: {}", progress.files_total));
        lines.push(format!("  percent: {}", progress.percent));
        if let Some(eta) = progress.eta_seconds {
            lines.push(format!("  eta_seconds: {}", eta));
        }
    }

    lines.join("\n")
}

/// Encode a module shard with all its files
fn encode_module_shard(module_name: &str, summaries: &[SemanticSummary], repo_root: &Path) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: module_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("module: \"{}\"", module_name));
    lines.push(format!("file_count: {}", summaries.len()));

    // Calculate aggregate risk
    let high = summaries.iter().filter(|s| s.behavioral_risk == RiskLevel::High).count();
    let medium = summaries.iter().filter(|s| s.behavioral_risk == RiskLevel::Medium).count();
    let low = summaries.len() - high - medium;
    lines.push(format!("risk_breakdown: \"high:{},medium:{},low:{}\"", high, medium, low));

    // List symbols in this module
    let symbols: Vec<_> = summaries
        .iter()
        .filter_map(|s| s.symbol_id.as_ref().map(|id| (&id.hash, s.symbol.as_ref(), &s.behavioral_risk)))
        .collect();

    if !symbols.is_empty() {
        lines.push(format!("symbols[{}]{{hash,name,risk}}:", symbols.len()));
        for (hash, name, risk) in symbols {
            let name_str = name.map(|n| n.as_str()).unwrap_or("_");
            lines.push(format!("  {},\"{}\",{}", hash, name_str, risk.as_str()));
        }
    }

    // Add source file info for staleness detection
    lines.push(format!("_meta:"));
    lines.push(format!("  generated_at: \"{}\"", chrono::Utc::now().to_rfc3339()));
    lines.push(format!("  source_files[{}]:", summaries.len()));
    for s in summaries {
        if let Some(info) = SourceFileInfo::from_path(Path::new(&s.file), repo_root) {
            lines.push(format!("    path: \"{}\"", info.path));
            lines.push(format!("    mtime: {}", info.mtime));
        }
    }

    lines.join("\n")
}

/// Encode a single symbol shard
fn encode_symbol_shard(summary: &SemanticSummary) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: symbol_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));

    // Full symbol encoding
    lines.push(encode_toon(summary));

    lines.join("\n")
}

/// Build call graph from summaries
fn build_call_graph(summaries: &[SemanticSummary]) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for summary in summaries {
        if let Some(ref symbol_id) = summary.symbol_id {
            let mut calls: Vec<String> = Vec::new();

            // Extract from explicit calls array (JS/TS/Python)
            for c in &summary.calls {
                let call_name = if let Some(ref obj) = c.object {
                    format!("{}.{}", obj, c.name)
                } else {
                    c.name.clone()
                };
                if !calls.contains(&call_name) {
                    calls.push(call_name);
                }
            }

            // Extract function calls from state_changes initializers (Rust/Go)
            // These look like: "CacheDir::for_repo(repo_path)?", "build_call_graph(&self.all_summaries)"
            for state in &summary.state_changes {
                if !state.initializer.is_empty() {
                    // Look for function call patterns: name(...) or path::name(...)
                    if let Some(call_name) = extract_call_from_initializer(&state.initializer) {
                        if !calls.contains(&call_name) {
                            calls.push(call_name);
                        }
                    }
                }
            }

            // Extract from added_dependencies that look like function calls
            // (e.g., "encode_toon", "generate_repo_overview")
            for dep in &summary.added_dependencies {
                // Skip type-like dependencies (capitalized or contains ::)
                if !dep.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    && !dep.contains("::")
                    && !calls.contains(dep)
                {
                    // Only add if it looks like a function (lowercase start)
                    if dep.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                        calls.push(dep.clone());
                    }
                }
            }

            if !calls.is_empty() {
                graph.insert(symbol_id.hash.clone(), calls);
            }
        }
    }

    graph
}

/// Extract a function call name from an initializer expression
fn extract_call_from_initializer(init: &str) -> Option<String> {
    let trimmed = init.trim();

    // Skip simple literals and keywords
    if trimmed.is_empty()
        || trimmed.starts_with('"')
        || trimmed.starts_with('\'')
        || trimmed.parse::<i64>().is_ok()
        || trimmed.parse::<f64>().is_ok()
        || trimmed == "true"
        || trimmed == "false"
        || trimmed == "None"
        || trimmed == "null"
        || trimmed == "undefined"
    {
        return None;
    }

    // Look for function call pattern: something(...)
    // Use split_once for guaranteed UTF-8 safety
    if let Some((before_paren, _)) = trimmed.split_once('(') {

        // Handle method chains: take the last part
        // e.g., "self.cache.repo_overview_path()" -> "repo_overview_path"
        let call_part = before_paren
            .rsplit(&['.', ':'][..])
            .next()
            .unwrap_or(before_paren)
            .trim();

        // Skip if it's a type constructor (starts with uppercase)
        if call_part.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            // But allow things like Vec::new, HashMap::new
            if call_part == "new" || call_part == "default" {
                // Try to get the type name
                if let Some(type_part) = before_paren.rsplit("::").nth(1) {
                    return Some(format!("{}::{}", type_part.trim(), call_part));
                }
            }
            return None;
        }

        // Skip very short names that are likely not meaningful
        if call_part.len() < 2 {
            return None;
        }

        // Skip common noise
        let noise = [
            "iter", "map", "filter", "collect", "clone", "to_string", "len",
            "is_empty", "unwrap", "unwrap_or", "ok", "err", "as_ref", "as_str",
            "into", "from", "push", "pop", "get", "insert", "remove",
        ];
        if noise.contains(&call_part) {
            return None;
        }

        return Some(call_part.to_string());
    }

    None
}

/// Encode call graph
fn encode_call_graph(graph: &HashMap<String, Vec<String>>) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: call_graph"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("edges: {}", graph.len()));

    for (symbol_hash, calls) in graph {
        let calls_str = calls.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(",");
        lines.push(format!("{}: [{}]", symbol_hash, calls_str));
    }

    lines.join("\n")
}

/// Build import graph from summaries
fn build_import_graph(summaries: &[SemanticSummary]) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for summary in summaries {
        if !summary.local_imports.is_empty() {
            graph.insert(summary.file.clone(), summary.local_imports.clone());
        }
    }

    graph
}

/// Encode import graph
fn encode_import_graph(graph: &HashMap<String, Vec<String>>) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: import_graph"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("files: {}", graph.len()));

    for (file, imports) in graph {
        let imports_str = imports.iter().map(|i| format!("\"{}\"", i)).collect::<Vec<_>>().join(",");
        lines.push(format!("\"{}\": [{}]", file, imports_str));
    }

    lines.join("\n")
}

/// Build module dependency graph
fn build_module_graph(modules: &HashMap<String, Vec<SemanticSummary>>) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for (module_name, summaries) in modules {
        let mut deps: Vec<String> = Vec::new();

        for summary in summaries {
            for import in &summary.local_imports {
                let import_module = extract_module_name(import);
                if import_module != *module_name && !deps.contains(&import_module) {
                    deps.push(import_module);
                }
            }
        }

        if !deps.is_empty() {
            graph.insert(module_name.clone(), deps);
        }
    }

    graph
}

/// Encode module graph
fn encode_module_graph(graph: &HashMap<String, Vec<String>>) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: module_graph"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("modules: {}", graph.len()));

    for (module, deps) in graph {
        let deps_str = deps.iter().map(|d| format!("\"{}\"", d)).collect::<Vec<_>>().join(",");
        lines.push(format!("\"{}\": [{}]", module, deps_str));
    }

    lines.join("\n")
}

/// Extract module name from a file path
fn extract_module_name(file_path: &str) -> String {
    let path_lower = file_path.to_lowercase();

    // Test files
    if path_lower.contains("test") || path_lower.contains("spec") || path_lower.contains("fixture") {
        return "tests".to_string();
    }

    // API routes
    if path_lower.contains("/api/") || path_lower.contains("/routes/") {
        return "api".to_string();
    }

    // Database
    if path_lower.contains("/db/") || path_lower.contains("/database/") || path_lower.contains("/schema") {
        return "database".to_string();
    }

    // Components
    if path_lower.contains("/components/") {
        return "components".to_string();
    }

    // Pages
    if path_lower.contains("/pages/") || path_lower.contains("/app/") {
        return "pages".to_string();
    }

    // Library/utils
    if path_lower.contains("/lib/") || path_lower.contains("/utils/") {
        return "lib".to_string();
    }

    // MCP server
    if path_lower.contains("/mcp_server/") || path_lower.contains("/mcp-server/") {
        return "mcp_server".to_string();
    }

    // Git module
    if path_lower.contains("/git/") {
        return "git".to_string();
    }

    // Find the src/ portion of the path (handles absolute paths)
    let src_marker = "/src/";
    if let Some(src_pos) = file_path.find(src_marker) {
        let after_src = &file_path[src_pos + src_marker.len()..];

        // Check if it's a direct file in src/ (like src/main.rs) or a subdirectory
        if let Some(slash_pos) = after_src.find('/') {
            // It's a subdirectory - use the directory name as module
            let module = &after_src[..slash_pos];
            if !module.is_empty() {
                return module.to_string();
            }
        } else {
            // Direct file in src/ - use the filename without extension as module
            let module = after_src
                .trim_end_matches(".rs")
                .trim_end_matches(".ts")
                .trim_end_matches(".tsx")
                .trim_end_matches(".js")
                .trim_end_matches(".jsx")
                .trim_end_matches(".go")
                .trim_end_matches(".py");

            if !module.is_empty() && module != "index" && module != "mod" && module != "lib" {
                return module.to_string();
            }
        }
    }

    // Fallback: Try relative path prefixes
    if let Some(stripped) = file_path.strip_prefix("src/").or_else(|| file_path.strip_prefix("./src/")) {
        if let Some(first_part) = stripped.split('/').next() {
            let module = first_part
                .trim_end_matches(".rs")
                .trim_end_matches(".ts")
                .trim_end_matches(".tsx")
                .trim_end_matches(".js")
                .trim_end_matches(".jsx")
                .trim_end_matches(".go")
                .trim_end_matches(".py");

            if !module.is_empty() && module != "index" && module != "mod" {
                return module.to_string();
            }
        }
    }

    "other".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module_name() {
        // Relative paths
        assert_eq!(extract_module_name("src/api/users.ts"), "api");
        assert_eq!(extract_module_name("src/components/Button.tsx"), "components");
        assert_eq!(extract_module_name("src/lib/utils.ts"), "lib");
        assert_eq!(extract_module_name("tests/unit/foo.test.ts"), "tests");
        assert_eq!(extract_module_name("src/main.rs"), "main");

        // Absolute paths
        assert_eq!(extract_module_name("/home/user/project/src/cache.rs"), "cache");
        assert_eq!(extract_module_name("/home/user/project/src/git/branch.rs"), "git");
        assert_eq!(extract_module_name("/home/user/project/src/mcp_server/mod.rs"), "mcp_server");
        assert_eq!(extract_module_name("/home/user/project/src/schema.rs"), "database"); // contains /schema

        // Edge cases
        assert_eq!(extract_module_name("/random/path/file.rs"), "other");
    }

    #[test]
    fn test_shard_stats() {
        let stats = ShardStats {
            overview_bytes: 100,
            module_bytes: 500,
            symbol_bytes: 300,
            graph_bytes: 200,
            ..Default::default()
        };

        assert_eq!(stats.total_bytes(), 1100);
    }
}
