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

use crate::analysis::{calculate_cognitive_complexity, max_nesting_depth};
use crate::cache::{CacheDir, IndexingStatus, SourceFileInfo};
use crate::duplicate::FunctionSignature;
use crate::error::Result;
use crate::schema::{RepoOverview, RiskLevel, SemanticSummary, SymbolId, SymbolInfo, SymbolKind, SCHEMA_VERSION};
use crate::toon::{encode_toon, generate_repo_overview, is_meaningful_call};

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

    /// Create a shard writer with a custom cache directory
    /// Useful for worktrees where we need to use CacheDir::for_worktree
    pub fn with_cache(cache: CacheDir) -> Result<Self> {
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

        // Write function signature index (duplicate detection)
        self.write_signature_index(&mut stats)?;

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
    ///
    /// This now iterates over summary.symbols to capture ALL symbols in each file,
    /// not just the primary symbol. This is the key fix for multi-symbol files.
    fn write_symbol_shards(&self, stats: &mut ShardStats) -> Result<()> {
        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    let symbol_id = symbol_info.to_symbol_id(&namespace);
                    let toon = encode_symbol_shard_from_info(summary, symbol_info, &symbol_id);
                    let path = self.cache.symbol_path(&symbol_id.hash);

                    let mut file = fs::File::create(&path)?;
                    file.write_all(toon.as_bytes())?;

                    stats.symbol_bytes += toon.len();
                    stats.symbols_written += 1;
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format for backward compatibility
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
    ///
    /// Now writes entries for ALL symbols in summary.symbols, not just the primary one.
    fn write_symbol_index(&self, stats: &mut ShardStats) -> Result<()> {
        use crate::cache::SymbolIndexEntry;

        let path = self.cache.symbol_index_path();
        let mut file = fs::File::create(&path)?;

        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);
            let module_name = extract_module_name(&summary.file);

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    let symbol_id = symbol_info.to_symbol_id(&namespace);

                    // Calculate cognitive complexity from control flow
                    // If symbol has its own control_flow, use that
                    // Otherwise, filter summary's control_flow_changes by symbol's line range
                    let (cc, nest) = if !symbol_info.control_flow.is_empty() {
                        (
                            calculate_cognitive_complexity(&symbol_info.control_flow),
                            max_nesting_depth(&symbol_info.control_flow)
                        )
                    } else {
                        // Filter file-level control flow by symbol's line range
                        let symbol_cf: Vec<_> = summary.control_flow_changes.iter()
                            .filter(|cf| {
                                cf.location.line >= symbol_info.start_line
                                    && cf.location.line <= symbol_info.end_line
                            })
                            .cloned()
                            .collect();
                        (
                            calculate_cognitive_complexity(&symbol_cf),
                            max_nesting_depth(&symbol_cf)
                        )
                    };

                    let entry = SymbolIndexEntry {
                        symbol: symbol_info.name.clone(),
                        hash: symbol_id.hash.clone(),
                        kind: format!("{:?}", symbol_info.kind).to_lowercase(),
                        module: module_name.clone(),
                        file: summary.file.clone(),
                        lines: format!("{}-{}", symbol_info.start_line, symbol_info.end_line),
                        risk: format!("{:?}", symbol_info.behavioral_risk).to_lowercase(),
                        cognitive_complexity: cc,
                        max_nesting: nest,
                    };

                    // Write as JSONL (one JSON object per line)
                    let json = serde_json::to_string(&entry)
                        .map_err(|e| crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize symbol index entry: {}", e),
                        })?;
                    writeln!(file, "{}", json)?;

                    stats.index_entries += 1;
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format - use summary's control flow
                let cc = calculate_cognitive_complexity(&summary.control_flow_changes);
                let nest = max_nesting_depth(&summary.control_flow_changes);

                let entry = SymbolIndexEntry {
                    symbol: summary.symbol.clone().unwrap_or_default(),
                    hash: symbol_id.hash.clone(),
                    kind: summary.symbol_kind
                        .map(|k| format!("{:?}", k).to_lowercase())
                        .unwrap_or_else(|| "unknown".to_string()),
                    module: module_name,
                    file: summary.file.clone(),
                    lines: match (summary.start_line, summary.end_line) {
                        (Some(s), Some(e)) => format!("{}-{}", s, e),
                        (Some(s), None) => format!("{}", s),
                        _ => String::new(),
                    },
                    risk: format!("{:?}", summary.behavioral_risk).to_lowercase(),
                    cognitive_complexity: cc,
                    max_nesting: nest,
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

    /// Write the function signature index for duplicate detection
    ///
    /// Generates FunctionSignature entries for each symbol to enable
    /// fast duplicate detection via two-phase matching.
    fn write_signature_index(&self, stats: &mut ShardStats) -> Result<()> {
        let path = self.cache.signature_index_path();
        let mut file = fs::File::create(&path)?;

        for summary in &self.all_summaries {
            let namespace = SymbolId::namespace_from_path(&summary.file);

            // If we have symbols in the new multi-symbol format, use those
            if !summary.symbols.is_empty() {
                for symbol_info in &summary.symbols {
                    // Skip non-function symbols (classes, interfaces, etc. don't get signatures)
                    if !matches!(
                        symbol_info.kind,
                        SymbolKind::Function | SymbolKind::Method | SymbolKind::Component
                    ) {
                        continue;
                    }

                    let symbol_id = symbol_info.to_symbol_id(&namespace);
                    let signature = FunctionSignature::from_symbol_info(
                        symbol_info,
                        &symbol_id.hash,
                        &summary.file,
                        None, // Use default boilerplate config
                    );

                    // Write as JSONL (one JSON object per line)
                    let json = serde_json::to_string(&signature)
                        .map_err(|e| crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize signature: {}", e),
                        })?;
                    writeln!(file, "{}", json)?;

                    stats.signature_entries += 1;
                }
            } else if let Some(ref symbol_id) = summary.symbol_id {
                // Fallback to old single-symbol format
                // Create a minimal SymbolInfo from the summary
                if let Some(ref name) = summary.symbol {
                    let symbol_info = SymbolInfo {
                        name: name.clone(),
                        kind: summary.symbol_kind.unwrap_or_default(),
                        start_line: summary.start_line.unwrap_or(1),
                        end_line: summary.end_line.unwrap_or(1),
                        is_exported: true,
                        is_default_export: false,
                        hash: Some(symbol_id.hash.clone()),
                        arguments: summary.arguments.clone(),
                        props: summary.props.clone(),
                        return_type: summary.return_type.clone(),
                        calls: summary.calls.clone(),
                        control_flow: summary.control_flow_changes.clone(),
                        state_changes: summary.state_changes.clone(),
                        behavioral_risk: summary.behavioral_risk,
                    };

                    let signature = FunctionSignature::from_symbol_info(
                        &symbol_info,
                        &symbol_id.hash,
                        &summary.file,
                        None,
                    );

                    let json = serde_json::to_string(&signature)
                        .map_err(|e| crate::McpDiffError::ExtractionFailure {
                            message: format!("Failed to serialize signature: {}", e),
                        })?;
                    writeln!(file, "{}", json)?;

                    stats.signature_entries += 1;
                }
            }
        }

        stats.signature_bytes = fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
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

    /// Number of entries in signature index (duplicate detection)
    pub signature_entries: usize,

    /// Bytes written for signature index
    pub signature_bytes: usize,
}

impl ShardStats {
    /// Total bytes written
    pub fn total_bytes(&self) -> usize {
        self.overview_bytes + self.module_bytes + self.symbol_bytes + self.graph_bytes + self.index_bytes + self.signature_bytes
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
///
/// Now lists ALL symbols from each file's summary.symbols, not just the primary one.
fn encode_module_shard(module_name: &str, summaries: &[SemanticSummary], repo_root: &Path) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: module_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("module: \"{}\"", module_name));
    lines.push(format!("file_count: {}", summaries.len()));

    // Collect ALL symbols from all files in this module
    let mut all_symbols: Vec<(String, String, SymbolKind, String, RiskLevel)> = Vec::new();
    let mut high = 0;
    let mut medium = 0;
    let mut low = 0;

    for summary in summaries {
        let namespace = SymbolId::namespace_from_path(&summary.file);

        // If we have multi-symbol format, use those
        if !summary.symbols.is_empty() {
            for symbol_info in &summary.symbols {
                let symbol_id = symbol_info.to_symbol_id(&namespace);
                let lines_str = format!("{}-{}", symbol_info.start_line, symbol_info.end_line);
                all_symbols.push((
                    symbol_id.hash,
                    symbol_info.name.clone(),
                    symbol_info.kind,
                    lines_str,
                    symbol_info.behavioral_risk,
                ));

                match symbol_info.behavioral_risk {
                    RiskLevel::High => high += 1,
                    RiskLevel::Medium => medium += 1,
                    RiskLevel::Low => low += 1,
                }
            }
        } else if let Some(ref symbol_id) = summary.symbol_id {
            // Fallback to old format
            let lines_str = match (summary.start_line, summary.end_line) {
                (Some(s), Some(e)) => format!("{}-{}", s, e),
                _ => String::new(),
            };
            all_symbols.push((
                symbol_id.hash.clone(),
                summary.symbol.clone().unwrap_or_default(),
                summary.symbol_kind.unwrap_or_default(),
                lines_str,
                summary.behavioral_risk,
            ));

            match summary.behavioral_risk {
                RiskLevel::High => high += 1,
                RiskLevel::Medium => medium += 1,
                RiskLevel::Low => low += 1,
            }
        }
    }

    lines.push(format!("risk_breakdown: \"high:{},medium:{},low:{}\"", high, medium, low));

    // List all symbols with expanded info
    if !all_symbols.is_empty() {
        lines.push(format!("symbols[{}]{{hash,name,kind,lines,risk}}:", all_symbols.len()));
        for (hash, name, kind, lines_str, risk) in &all_symbols {
            lines.push(format!(
                "  {},\"{}\",{},{},{}",
                hash,
                name,
                kind.as_str(),
                lines_str,
                risk.as_str()
            ));
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

/// Encode a single symbol shard (legacy format)
fn encode_symbol_shard(summary: &SemanticSummary) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: symbol_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));

    // Full symbol encoding
    lines.push(encode_toon(summary));

    lines.join("\n")
}

/// Encode a symbol shard from SymbolInfo (new multi-symbol format)
///
/// This creates a complete symbol shard from a SymbolInfo struct,
/// combining file-level metadata from the summary with symbol-specific data.
fn encode_symbol_shard_from_info(
    summary: &SemanticSummary,
    symbol_info: &SymbolInfo,
    symbol_id: &SymbolId,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("_type: symbol_shard"));
    lines.push(format!("schema_version: \"{}\"", SCHEMA_VERSION));
    lines.push(format!("file: \"{}\"", summary.file));
    lines.push(format!("language: {}", summary.language));
    lines.push(format!("symbol_id: {}", symbol_id.hash));
    lines.push(format!("symbol_namespace: \"{}\"", symbol_id.namespace));
    lines.push(format!("symbol: {}", symbol_info.name));
    lines.push(format!("symbol_kind: {}", symbol_info.kind.as_str()));
    lines.push(format!("lines: \"{}-{}\"", symbol_info.start_line, symbol_info.end_line));

    if symbol_info.is_exported {
        lines.push(format!("public_surface_changed: true"));
    }

    lines.push(format!("behavioral_risk: {}", symbol_info.behavioral_risk.as_str()));

    // Arguments
    if !symbol_info.arguments.is_empty() {
        let args: Vec<String> = symbol_info
            .arguments
            .iter()
            .map(|a| {
                if let Some(ref t) = a.arg_type {
                    format!("{}:{}", a.name, t)
                } else {
                    a.name.clone()
                }
            })
            .collect();
        lines.push(format!("arguments[{}]: {}", args.len(), args.join(",")));
    }

    // Props
    if !symbol_info.props.is_empty() {
        let props: Vec<String> = symbol_info
            .props
            .iter()
            .map(|p| {
                if let Some(ref t) = p.prop_type {
                    format!("{}:{}", p.name, t)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        lines.push(format!("props[{}]: {}", props.len(), props.join(",")));
    }

    // Return type
    if let Some(ref ret) = symbol_info.return_type {
        lines.push(format!("return_type: \"{}\"", ret));
    }

    // Control flow
    if !symbol_info.control_flow.is_empty() {
        let cf: Vec<String> = symbol_info
            .control_flow
            .iter()
            .map(|c| c.kind.as_str().to_string())
            .collect();
        lines.push(format!("control_flow[{}]: {}", cf.len(), cf.join(",")));
    }

    // Calls (use file-level calls for now, filtered by line range would be ideal)
    // For now, include all file-level calls as context
    if !summary.calls.is_empty() {
        let meaningful_calls: Vec<_> = summary
            .calls
            .iter()
            .filter(|c| is_meaningful_call(&c.name, c.object.as_deref()))
            .collect();

        if !meaningful_calls.is_empty() {
            lines.push(format!("calls[{}]{{name,obj,await,try,count}}:", meaningful_calls.len()));

            // Deduplicate calls
            let mut call_counts: HashMap<String, (Option<String>, bool, bool, usize)> = HashMap::new();
            for call in &meaningful_calls {
                let key = format!("{}:{:?}", call.name, call.object);
                call_counts
                    .entry(key)
                    .and_modify(|e| e.3 += 1)
                    .or_insert((call.object.clone(), call.is_awaited, call.in_try, 1));
            }

            for (key, (obj, is_awaited, in_try, count)) in call_counts {
                let name = key.split(':').next().unwrap_or(&key);
                let obj_str = obj.as_deref().unwrap_or("_");
                let await_str = if is_awaited { "Y" } else { "_" };
                let try_str = if in_try { "Y" } else { "_" };
                let count_str = if count > 1 {
                    format!("\"{}\"", count)
                } else {
                    "_".to_string()
                };
                lines.push(format!("  {},{},{},{},{}", name, obj_str, await_str, try_str, count_str));
            }
        }
    }

    // Include file-level dependencies as context
    if !summary.added_dependencies.is_empty() {
        let deps: Vec<String> = summary
            .added_dependencies
            .iter()
            .take(20) // Limit to avoid huge output
            .cloned()
            .collect();
        lines.push(format!("added_dependencies[{}]: {}", deps.len(), deps.join(",")));
    }

    lines.join("\n")
}

/// Build a lookup map from symbol name to their SymbolIds
/// Returns: name -> Vec<(hash, namespace)> for disambiguation
fn build_symbol_lookup(summaries: &[SemanticSummary]) -> HashMap<String, Vec<(String, String)>> {
    let mut lookup: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for summary in summaries {
        if let Some(ref symbol_id) = summary.symbol_id {
            lookup
                .entry(symbol_id.symbol.clone())
                .or_default()
                .push((symbol_id.hash.clone(), symbol_id.namespace.clone()));
        }

        // Also index symbols from the symbols array
        for symbol in &summary.symbols {
            let hash = crate::overlay::compute_symbol_hash(symbol, &summary.file);
            let namespace = SymbolId::namespace_from_path(&summary.file);
            lookup
                .entry(symbol.name.clone())
                .or_default()
                .push((hash, namespace));
        }
    }

    // Deduplicate entries (same hash can appear multiple times)
    for entries in lookup.values_mut() {
        entries.sort();
        entries.dedup();
    }

    lookup
}

/// Resolve a call name to a symbol hash if possible
/// Returns the hash if uniquely resolved, or the original name if ambiguous/external
fn resolve_call_to_hash(
    call_name: &str,
    lookup: &HashMap<String, Vec<(String, String)>>,
) -> String {
    // Try exact match first
    if let Some(matches) = lookup.get(call_name) {
        if matches.len() == 1 {
            // Unique match - return hash
            return matches[0].0.clone();
        }
        // Multiple matches - return first hash but log ambiguity
        // In future, could use import info to disambiguate
        if !matches.is_empty() {
            return matches[0].0.clone();
        }
    }

    // No match - external call, return as-is
    // Prefix with "ext:" to distinguish from hashes
    format!("ext:{}", call_name)
}

/// Build call graph from summaries with resolved symbol hashes
/// Now uses per-symbol calls (symbol.calls) instead of file-level calls
fn build_call_graph(summaries: &[SemanticSummary]) -> HashMap<String, Vec<String>> {
    use crate::overlay::compute_symbol_hash;

    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // Build lookup for resolving call names to hashes
    let symbol_lookup = build_symbol_lookup(summaries);

    for summary in summaries {
        // Process each symbol in the file and use symbol.calls for per-function call tracking
        for symbol in &summary.symbols {
            // Compute hash using absolute path (summary.file) - canonical rule everywhere
            let hash = compute_symbol_hash(symbol, &summary.file);
            let mut calls: Vec<String> = Vec::new();

            // Extract from the symbol's own calls array (SymbolInfo.calls)
            for c in &symbol.calls {
                let call_name = if let Some(ref obj) = c.object {
                    format!("{}.{}", obj, c.name)
                } else {
                    c.name.clone()
                };
                let resolved = resolve_call_to_hash(&call_name, &symbol_lookup);
                if !calls.contains(&resolved) {
                    calls.push(resolved);
                }
            }

            // Extract function calls from symbol's state_changes initializers
            for state in &symbol.state_changes {
                if !state.initializer.is_empty() {
                    if let Some(call_name) = extract_call_from_initializer(&state.initializer) {
                        let resolved = resolve_call_to_hash(&call_name, &symbol_lookup);
                        if !calls.contains(&resolved) {
                            calls.push(resolved);
                        }
                    }
                }
            }

            if !calls.is_empty() {
                graph.insert(hash, calls);
            }
        }

        // Also process file-level calls (for backward compatibility and module-level code)
        if let Some(ref symbol_id) = summary.symbol_id {
            let mut calls: Vec<String> = Vec::new();

            // Extract from file-level calls array (calls not inside any function)
            for c in &summary.calls {
                let call_name = if let Some(ref obj) = c.object {
                    format!("{}.{}", obj, c.name)
                } else {
                    c.name.clone()
                };
                let resolved = resolve_call_to_hash(&call_name, &symbol_lookup);
                if !calls.contains(&resolved) {
                    calls.push(resolved);
                }
            }

            // Extract function calls from file-level state_changes initializers
            for state in &summary.state_changes {
                if !state.initializer.is_empty() {
                    if let Some(call_name) = extract_call_from_initializer(&state.initializer) {
                        let resolved = resolve_call_to_hash(&call_name, &symbol_lookup);
                        if !calls.contains(&resolved) {
                            calls.push(resolved);
                        }
                    }
                }
            }

            // Extract from added_dependencies that look like function calls
            for dep in &summary.added_dependencies {
                if !dep.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    && !dep.contains("::")
                {
                    if dep.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                        let resolved = resolve_call_to_hash(dep, &symbol_lookup);
                        if !calls.contains(&resolved) {
                            calls.push(resolved);
                        }
                    }
                }
            }

            if !calls.is_empty() {
                // Merge with existing entry for this symbol
                graph.entry(symbol_id.hash.clone()).or_default().extend(calls);
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

/// Extract module/namespace from file path.
///
/// Returns the path-based namespace (directory structure after src/).
/// For languages with real namespaces (Rust, Python, Java, Go), the extractor
/// should override this with the actual language namespace.
fn extract_module_name(file_path: &str) -> String {
    // Extract the portion of the path after /src/ (or similar source roots)
    let source_markers = ["/src/", "/lib/", "/app/", "/pages/"];
    let mut relative_path = file_path;

    for marker in &source_markers {
        if let Some(pos) = file_path.find(marker) {
            relative_path = &file_path[pos + marker.len()..];
            break;
        }
    }

    // Also handle relative paths starting with src/
    if relative_path == file_path {
        for prefix in &["src/", "lib/", "app/", "pages/"] {
            if let Some(stripped) = file_path.strip_prefix(prefix) {
                relative_path = stripped;
                break;
            }
        }
    }

    // Get the directory path (everything before the filename)
    let path = std::path::Path::new(relative_path);
    if let Some(parent) = path.parent() {
        let parent_str = parent.to_string_lossy();
        if !parent_str.is_empty() && parent_str != "." {
            // Convert path separators to dots for namespace
            return parent_str.replace('/', ".").replace('\\', ".");
        }
    }

    // File is directly in src/ - use filename without extension
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("root");

    // Skip generic names
    if matches!(stem, "index" | "mod" | "lib" | "main" | "__init__") {
        return "root".to_string();
    }

    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module_name() {
        // Files in subdirectories get dotted namespace from directory path
        assert_eq!(extract_module_name("src/api/users.ts"), "api");
        assert_eq!(extract_module_name("src/components/Button.tsx"), "components");
        assert_eq!(extract_module_name("src/utils/format.ts"), "utils");
        assert_eq!(extract_module_name("src/features/auth/login.ts"), "features.auth");

        // Files directly in src/ use filename as namespace
        assert_eq!(extract_module_name("src/cache.rs"), "cache");
        assert_eq!(extract_module_name("src/schema.rs"), "schema");

        // Generic filenames fallback to "root"
        assert_eq!(extract_module_name("src/index.ts"), "root");
        assert_eq!(extract_module_name("src/main.rs"), "root");

        // Absolute paths - extracts after /src/
        assert_eq!(extract_module_name("/home/user/project/src/git/branch.rs"), "git");
        assert_eq!(extract_module_name("/home/user/project/src/mcp_server/mod.rs"), "mcp_server");
        assert_eq!(extract_module_name("/home/user/my-test-worktree/src/App.tsx"), "App");
        assert_eq!(extract_module_name("/home/user/my-test-worktree/src/utils/format.ts"), "utils");

        // Nested directories use dots
        assert_eq!(extract_module_name("/project/src/server/api/handlers/users.ts"), "server.api.handlers");
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
