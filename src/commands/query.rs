//! Query command handler - Query the semantic index for symbols, source, callers, etc.

use std::fs;

use crate::cache::{CacheDir, SymbolIndexEntry};
use crate::cli::{OutputFormat, QueryArgs, QueryType};
use crate::commands::toon_parser::read_cached_file;
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};

/// Run the query command
pub fn run_query(args: &QueryArgs, ctx: &CommandContext) -> Result<String> {
    match &args.query_type {
        QueryType::Overview {
            modules,
            max_modules,
        } => run_overview(*modules, *max_modules, ctx),
        QueryType::Module {
            name,
            symbols,
            kind,
            risk,
            limit,
        } => {
            if *symbols {
                run_list_module_symbols(name, kind.as_deref(), risk.as_deref(), *limit, ctx)
            } else {
                run_get_module(name, ctx)
            }
        }
        QueryType::Symbol { hash, source } => run_get_symbol(hash, *source, ctx),
        QueryType::Source {
            file,
            start,
            end,
            hash,
            context,
        } => run_get_source(file, *start, *end, hash.as_deref(), *context, ctx),
        QueryType::Callers {
            hash,
            depth,
            source,
            limit,
        } => run_get_callers(hash, *depth, *source, *limit, ctx),
        QueryType::Callgraph {
            module,
            symbol,
            export,
            stats_only,
            limit,
        } => run_get_callgraph(
            module.as_deref(),
            symbol.as_deref(),
            export.as_deref(),
            *stats_only,
            *limit,
            ctx,
        ),
        QueryType::File { path, source, kind } => {
            run_file_symbols(path, *source, kind.as_deref(), ctx)
        }
        QueryType::Languages => run_list_languages(ctx),
    }
}

/// Get repository overview
fn run_overview(include_modules: bool, max_modules: usize, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    // Read overview
    let overview_path = cache.repo_overview_path();
    if !overview_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "repo_overview.toon not found in cache".to_string(),
        });
    }

    let content = fs::read_to_string(&overview_path)?;

    // The repo_overview.toon is stored in TOON format
    // Filter modules based on flags
    let filtered_content = filter_overview_content(&content, include_modules, max_modules);

    match ctx.format {
        OutputFormat::Json => {
            // Convert TOON to JSON structure
            let json = toon_to_json_overview(&filtered_content);
            Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
        }
        OutputFormat::Toon => {
            // Return TOON as-is (it's already in TOON format)
            Ok(filtered_content)
        }
        OutputFormat::Text => {
            // Human-readable text format with header
            let mut output = String::new();
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  REPOSITORY OVERVIEW\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&filtered_content);
            Ok(output)
        }
    }
}

/// Filter overview content based on module flags
fn filter_overview_content(content: &str, include_modules: bool, max_modules: usize) -> String {
    if include_modules && max_modules > 0 {
        // Return all content, just limit modules
        let mut output = String::new();
        let mut in_modules = false;
        let mut module_count = 0;

        for line in content.lines() {
            if line.starts_with("modules[") {
                in_modules = true;
                output.push_str(line);
                output.push('\n');
            } else if in_modules && line.starts_with("  ") {
                module_count += 1;
                if module_count <= max_modules {
                    output.push_str(line);
                    output.push('\n');
                }
            } else if in_modules && !line.starts_with("  ") {
                in_modules = false;
                output.push_str(line);
                output.push('\n');
            } else {
                output.push_str(line);
                output.push('\n');
            }
        }
        output
    } else if !include_modules {
        // Strip modules section entirely
        let mut output = String::new();
        let mut in_modules = false;

        for line in content.lines() {
            if line.starts_with("modules[") {
                in_modules = true;
            } else if in_modules && !line.starts_with("  ") {
                in_modules = false;
                output.push_str(line);
                output.push('\n');
            } else if !in_modules {
                output.push_str(line);
                output.push('\n');
            }
        }
        output
    } else {
        content.to_string()
    }
}

/// Convert TOON overview content to JSON
fn toon_to_json_overview(content: &str) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    result.insert("_type".to_string(), serde_json::json!("repo_overview"));

    let mut modules = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("_type:") {
            // Skip, already added
        } else if line.starts_with("schema_version:") {
            if let Some(val) = line.strip_prefix("schema_version:") {
                result.insert(
                    "schema_version".to_string(),
                    serde_json::json!(val.trim().trim_matches('"')),
                );
            }
        } else if line.starts_with("framework:") {
            if let Some(val) = line.strip_prefix("framework:") {
                result.insert(
                    "framework".to_string(),
                    serde_json::json!(val.trim().trim_matches('"')),
                );
            }
        } else if line.starts_with("patterns[") {
            // Parse patterns array
            if let Some(colon_idx) = line.find(':') {
                let patterns_str = &line[colon_idx + 1..];
                let patterns: Vec<String> = patterns_str
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect();
                result.insert("patterns".to_string(), serde_json::json!(patterns));
            }
        } else if line.starts_with("modules[") {
            // We'll collect modules separately
        } else if line.starts_with("  ") && line.contains(',') {
            // Module line: name,purpose,files,risk
            let parts: Vec<&str> = line.trim().split(',').collect();
            if parts.len() >= 4 {
                modules.push(serde_json::json!({
                    "name": parts[0].trim().trim_matches('"'),
                    "purpose": parts[1].trim().trim_matches('"'),
                    "files": parts[2].trim().parse::<i32>().unwrap_or(0),
                    "risk": parts[3].trim().trim_matches('"')
                }));
            }
        }
    }

    if !modules.is_empty() {
        result.insert("modules".to_string(), serde_json::json!(modules));
    }

    serde_json::Value::Object(result)
}

/// Get a specific module's details
fn run_get_module(name: &str, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    let module_file_path = cache.module_path(name);
    if !module_file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Module '{}' not found", name),
        });
    }

    // Use toon_parser to handle TOON format
    let cached = read_cached_file(&module_file_path)?;

    match ctx.format {
        OutputFormat::Json => Ok(cached.as_json()),
        OutputFormat::Toon => Ok(cached.as_toon()),
        OutputFormat::Text => {
            // Human-readable text format
            let mut output = String::new();
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  MODULE: {}\n", name));
            output.push_str("═══════════════════════════════════════════\n\n");

            if let Some(symbols) = cached.json.get("symbols").and_then(|s| s.as_array()) {
                output.push_str(&format!("symbols[{}]:\n", symbols.len()));
                for sym in symbols.iter().take(50) {
                    let sym_name = sym
                        .get("symbol")
                        .or_else(|| sym.get("s"))
                        .or_else(|| sym.get("name"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("?");
                    let kind = sym
                        .get("kind")
                        .or_else(|| sym.get("k"))
                        .and_then(|k| k.as_str())
                        .unwrap_or("?");
                    let file = sym
                        .get("file")
                        .or_else(|| sym.get("f"))
                        .and_then(|f| f.as_str())
                        .unwrap_or("?");
                    let lines = sym
                        .get("lines")
                        .or_else(|| sym.get("l"))
                        .and_then(|l| l.as_str())
                        .unwrap_or("?");
                    output.push_str(&format!("  {} ({}) - {}:{}\n", sym_name, kind, file, lines));
                }
            }

            Ok(output)
        }
    }
}

/// List symbols in a module
fn run_list_module_symbols(
    module_name: &str,
    kind_filter: Option<&str>,
    risk_filter: Option<&str>,
    limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Read and parse module file using toon_parser
    let module_file_path = cache.module_path(module_name);
    if !module_file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Module '{}' not found", module_name),
        });
    }

    let cached = read_cached_file(&module_file_path)?;

    let mut symbols: Vec<SymbolIndexEntry> = Vec::new();

    if let Some(sym_array) = cached.json.get("symbols").and_then(|s| s.as_array()) {
        for sym in sym_array {
            let entry = symbol_from_json(sym, module_name);

            // Apply filters
            if let Some(kf) = kind_filter {
                if !entry.kind.eq_ignore_ascii_case(kf) {
                    continue;
                }
            }
            if let Some(rf) = risk_filter {
                if !entry.risk.eq_ignore_ascii_case(rf) {
                    continue;
                }
            }

            symbols.push(entry);
            if symbols.len() >= limit {
                break;
            }
        }
    }

    let json_value = serde_json::json!({
        "_type": "module_symbols",
        "module": module_name,
        "symbols": symbols,
        "count": symbols.len()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  MODULE SYMBOLS: {}\n", module_name));
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("symbols[{}]:\n", symbols.len()));
            for sym in &symbols {
                output.push_str(&format!(
                    "  {} ({}) [{}] {}:{}\n",
                    sym.symbol, sym.kind, sym.risk, sym.file, sym.lines
                ));
            }
        }
    }

    Ok(output)
}

/// Get a specific symbol by hash
fn run_get_symbol(hash: &str, include_source: bool, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Handle comma-separated hashes for batch queries
    let hashes: Vec<&str> = hash.split(',').map(|s| s.trim()).collect();

    let mut results: Vec<SymbolIndexEntry> = Vec::new();
    for h in &hashes {
        if let Some(symbol) = load_symbol_from_cache(&cache, h)? {
            results.push(symbol);
        }
    }

    if results.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: format!("Symbol(s) not found: {}", hash),
        });
    }

    let json_value = if results.len() == 1 {
        let mut val = serde_json::to_value(&results[0]).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("_type".to_string(), serde_json::json!("symbol"));
        }
        val
    } else {
        serde_json::json!({
            "_type": "symbols",
            "symbols": results,
            "count": results.len()
        })
    };

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  SYMBOL DETAILS\n");
            output.push_str("═══════════════════════════════════════════\n\n");

            for symbol in &results {
                output.push_str(&format!("## {} ({})\n", symbol.symbol, symbol.kind));
                output.push_str(&format!("hash: {}\n", symbol.hash));
                output.push_str(&format!("file: {}\n", symbol.file));
                output.push_str(&format!("lines: {}\n", symbol.lines));
                output.push_str(&format!("module: {}\n", symbol.module));
                output.push_str(&format!("risk: {}\n", symbol.risk));

                if include_source {
                    if let Some(source) =
                        get_source_for_symbol(&cache, &symbol.file, &symbol.lines, 3)
                    {
                        output.push_str("\n__source__:\n");
                        output.push_str(&source);
                    }
                }
                output.push('\n');
            }
        }
    }

    Ok(output)
}

/// Get source code for a file or symbol
fn run_get_source(
    file: &str,
    start: Option<usize>,
    end: Option<usize>,
    hash: Option<&str>,
    context: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    // If hash is provided, look up line range from symbol
    let (actual_start, actual_end) = if let Some(h) = hash {
        let cache = CacheDir::for_repo(&repo_dir)?;
        if let Some(symbol) = load_symbol_from_cache(&cache, h)? {
            let parts: Vec<&str> = symbol.lines.split('-').collect();
            let s: usize = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1);
            let e: usize = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(s);
            (s, e)
        } else {
            return Err(McpDiffError::FileNotFound {
                path: format!("Symbol not found: {}", h),
            });
        }
    } else {
        (start.unwrap_or(1), end.unwrap_or(start.unwrap_or(1) + 50))
    };

    let file_path = repo_dir.join(file);
    if !file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: file.to_string(),
        });
    }

    let content = fs::read_to_string(&file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Calculate range with context
    let start_with_ctx = actual_start.saturating_sub(context + 1);
    let end_with_ctx = (actual_end + context).min(lines.len());

    let source_lines: Vec<serde_json::Value> = lines
        .iter()
        .enumerate()
        .skip(start_with_ctx)
        .take(end_with_ctx - start_with_ctx)
        .map(|(i, line)| {
            let line_num = i + 1;
            serde_json::json!({
                "line": line_num,
                "content": line,
                "in_range": line_num >= actual_start && line_num <= actual_end
            })
        })
        .collect();

    let json_value = serde_json::json!({
        "_type": "source",
        "file": file,
        "start": actual_start,
        "end": actual_end,
        "context": context,
        "lines": source_lines
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("file: {}\n", file));
            output.push_str(&format!("range: {}-{}\n", actual_start, actual_end));
            output.push_str("---\n");

            for (i, line) in lines
                .iter()
                .enumerate()
                .skip(start_with_ctx)
                .take(end_with_ctx - start_with_ctx)
            {
                let line_num = i + 1;
                let prefix = if line_num >= actual_start && line_num <= actual_end {
                    ">"
                } else {
                    " "
                };
                output.push_str(&format!("{} {:>4} | {}\n", prefix, line_num, line));
            }
        }
    }

    Ok(output)
}

/// Get callers of a symbol
fn run_get_callers(
    hash: &str,
    depth: usize,
    include_source: bool,
    limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Load call graph using TOON parser
    let call_graph = cache.load_call_graph()?;
    if call_graph.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: "Call graph not found or empty. Run `semfora index generate` first.".to_string(),
        });
    }

    // Find callers (reverse lookup)
    // TOON format: caller_hash -> [callee1, callee2, ...]
    // We need to find all callers where our hash appears in their callee list
    let mut callers = Vec::new();
    let hash_lower = hash.to_lowercase();
    for (caller, callees) in &call_graph {
        // Check if our target hash is in this caller's callee list
        let is_caller = callees.iter().any(|callee| {
            callee.to_lowercase().contains(&hash_lower)
                || hash_lower.contains(&callee.to_lowercase())
        });
        if is_caller {
            callers.push(caller.clone());
        }
        if callers.len() >= limit {
            break;
        }
    }

    let json_value = serde_json::json!({
        "_type": "callers",
        "symbol_hash": hash,
        "depth": depth,
        "callers": callers,
        "count": callers.len()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  CALLERS\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("symbol: {}\n", hash));
            output.push_str(&format!("callers[{}]:\n", callers.len()));
            for caller in &callers {
                output.push_str(&format!("  - {}\n", caller));

                if include_source {
                    if let Some(symbol) = load_symbol_from_cache(&cache, caller)? {
                        if let Some(source) =
                            get_source_for_symbol(&cache, &symbol.file, &symbol.lines, 1)
                        {
                            output.push_str(&format!("    # {}:{}\n", symbol.file, symbol.lines));
                            for line in source.lines().take(5) {
                                output.push_str(&format!("    {}\n", line));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Get call graph
fn run_get_callgraph(
    module: Option<&str>,
    symbol: Option<&str>,
    export: Option<&str>,
    stats_only: bool,
    limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Handle SQLite export
    if let Some(export_path) = export {
        return run_export_sqlite(export_path, &cache, ctx);
    }

    // Load call graph using TOON parser
    let call_graph = cache.load_call_graph()?;
    if call_graph.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: "Call graph not found or empty. Run `semfora index generate` first.".to_string(),
        });
    }

    // Total edge count (each entry is caller -> [callees], count total callee references)
    let total_edges = call_graph.len();
    let total_calls: usize = call_graph.values().map(|v| v.len()).sum();

    if stats_only {
        let json_value = serde_json::json!({
            "_type": "call_graph_stats",
            "total_callers": total_edges,
            "total_call_edges": total_calls,
            "module_filter": module,
            "symbol_filter": symbol
        });

        let mut output = String::new();

        match ctx.format {
            OutputFormat::Json => {
                output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
            }
            OutputFormat::Toon => {
                output = super::encode_toon(&json_value);
            }
            OutputFormat::Text => {
                output.push_str("═══════════════════════════════════════════\n");
                output.push_str("  CALL GRAPH STATISTICS\n");
                output.push_str("═══════════════════════════════════════════\n\n");
                output.push_str(&format!("total_callers: {}\n", total_edges));
                output.push_str(&format!("total_call_edges: {}\n", total_calls));
                if let Some(m) = module {
                    output.push_str(&format!("module_filter: {}\n", m));
                }
                if let Some(s) = symbol {
                    output.push_str(&format!("symbol_filter: {}\n", s));
                }
            }
        }

        return Ok(output);
    }

    // Filter and return edges
    // TOON format: caller_hash -> [callee1, callee2, ...]
    let filtered_edges: Vec<(&String, &Vec<String>)> = call_graph
        .iter()
        .filter(|(caller, callees)| {
            let module_match = module
                .map(|m| caller.contains(m) || callees.iter().any(|c| c.contains(m)))
                .unwrap_or(true);

            let symbol_match = symbol
                .map(|s| caller.contains(s) || callees.iter().any(|c| c.contains(s)))
                .unwrap_or(true);

            module_match && symbol_match
        })
        .take(limit)
        .collect();

    let edges_json: Vec<serde_json::Value> = filtered_edges
        .iter()
        .map(|(caller, callees)| {
            serde_json::json!({
                "caller": caller,
                "callees": callees
            })
        })
        .collect();

    let json_value = serde_json::json!({
        "_type": "call_graph",
        "edges": edges_json,
        "count": filtered_edges.len()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  CALL GRAPH\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("edges[{}]:\n", filtered_edges.len()));
            for (caller, callees) in &filtered_edges {
                // Show caller -> [callees] format
                let callees_str = if callees.len() > 3 {
                    format!(
                        "[{}, ... +{} more]",
                        callees[..3].join(", "),
                        callees.len() - 3
                    )
                } else {
                    format!("[{}]", callees.join(", "))
                };
                output.push_str(&format!("  {} -> {}\n", caller, callees_str));
            }
        }
    }

    Ok(output)
}

/// Export call graph to SQLite
fn run_export_sqlite(path: &str, cache: &CacheDir, _ctx: &CommandContext) -> Result<String> {
    use crate::sqlite_export::{default_export_path, SqliteExporter};

    let output_path = if path.is_empty() {
        default_export_path(cache)
    } else {
        std::path::PathBuf::from(path)
    };

    eprintln!("Exporting call graph to: {}", output_path.display());

    let exporter = SqliteExporter::new();
    let stats = exporter.export(cache, &output_path, None)?;

    Ok(format!(
        "Export complete:\n  Path: {}\n  Nodes: {}\n  Edges: {}\n  Size: {} bytes",
        output_path.display(),
        stats.nodes_inserted,
        stats.edges_inserted,
        stats.file_size_bytes
    ))
}

/// Get all symbols in a file
fn run_file_symbols(
    path: &str,
    include_source: bool,
    kind_filter: Option<&str>,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Search through modules to find symbols in this file
    let symbols = find_symbols_in_file(&cache, path, kind_filter)?;

    let json_value = serde_json::json!({
        "_type": "file_symbols",
        "file": path,
        "symbols": symbols,
        "count": symbols.len()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  FILE: {}\n", path));
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("symbols[{}]:\n", symbols.len()));

            for sym in &symbols {
                output.push_str(&format!("  {} ({}) L{}\n", sym.symbol, sym.kind, sym.lines));
                output.push_str(&format!("    hash: {}\n", sym.hash));

                if include_source {
                    if let Some(source) = get_source_for_symbol(&cache, path, &sym.lines, 1) {
                        output.push_str("    source:\n");
                        for line in source.lines().take(5) {
                            output.push_str(&format!("      {}\n", line));
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

/// List supported languages
fn run_list_languages(ctx: &CommandContext) -> Result<String> {
    // All supported languages with their extensions
    let languages = vec![
        ("TypeScript", vec!["ts", "mts", "cts"]),
        ("Tsx", vec!["tsx"]),
        ("JavaScript", vec!["js", "mjs", "cjs"]),
        ("Jsx", vec!["jsx"]),
        ("Rust", vec!["rs"]),
        ("Python", vec!["py", "pyi"]),
        ("Go", vec!["go"]),
        ("Java", vec!["java"]),
        ("C", vec!["c", "h"]),
        ("Cpp", vec!["cpp", "cc", "cxx", "hpp", "hxx", "hh"]),
        ("CSharp", vec!["cs"]),
        ("Kotlin", vec!["kt", "kts"]),
        ("Html", vec!["html", "htm"]),
        ("Css", vec!["css"]),
        ("Scss", vec!["scss", "sass"]),
        ("Json", vec!["json"]),
        ("Yaml", vec!["yaml", "yml"]),
        ("Toml", vec!["toml"]),
        (
            "Xml",
            vec!["xml", "xsd", "xsl", "xslt", "svg", "plist", "pom"],
        ),
        ("Hcl", vec!["tf", "hcl", "tfvars"]),
        ("Markdown", vec!["md", "markdown"]),
        ("Vue", vec!["vue"]),
        ("Bash", vec!["sh", "bash", "zsh", "fish"]),
        ("Gradle", vec!["gradle"]),
        ("Dockerfile", vec!["dockerfile"]),
    ];

    let json_value = serde_json::json!({
        "_type": "languages",
        "languages": languages.iter().map(|(name, exts)| serde_json::json!({
            "name": name,
            "extensions": exts
        })).collect::<Vec<_>>(),
        "count": languages.len()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  SUPPORTED LANGUAGES\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("languages[{}]:\n", languages.len()));
            for (name, exts) in &languages {
                output.push_str(&format!("  {}: {}\n", name, exts.join(", ")));
            }
        }
    }

    Ok(output)
}

// ============================================
// Helper Functions
// ============================================

/// Convert a JSON symbol object to SymbolIndexEntry
///
/// Handles both full JSON keys and abbreviated TOON keys (s, h, k, f, l, r)
fn symbol_from_json(sym: &serde_json::Value, module_name: &str) -> SymbolIndexEntry {
    SymbolIndexEntry {
        symbol: sym
            .get("symbol")
            .or_else(|| sym.get("s"))
            .or_else(|| sym.get("name"))
            .and_then(|s| s.as_str())
            .unwrap_or("?")
            .to_string(),
        hash: sym
            .get("hash")
            .or_else(|| sym.get("h"))
            .and_then(|h| h.as_str())
            .unwrap_or("")
            .to_string(),
        semantic_hash: String::new(),
        kind: sym
            .get("kind")
            .or_else(|| sym.get("k"))
            .and_then(|k| k.as_str())
            .unwrap_or("?")
            .to_string(),
        module: module_name.to_string(),
        file: sym
            .get("file")
            .or_else(|| sym.get("f"))
            .and_then(|f| f.as_str())
            .unwrap_or("?")
            .to_string(),
        lines: sym
            .get("lines")
            .or_else(|| sym.get("l"))
            .and_then(|l| l.as_str())
            .unwrap_or("?")
            .to_string(),
        risk: sym
            .get("risk")
            .or_else(|| sym.get("r"))
            .and_then(|r| r.as_str())
            .unwrap_or("low")
            .to_string(),
        cognitive_complexity: sym.get("cc").and_then(|c| c.as_u64()).unwrap_or(0) as usize,
        max_nesting: sym.get("nest").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
    }
}

/// Load a symbol from the cache by hash
fn load_symbol_from_cache(cache: &CacheDir, hash: &str) -> Result<Option<SymbolIndexEntry>> {
    // Try to find the symbol in the symbol shard
    let symbol_path = cache.symbol_path(hash);
    if symbol_path.exists() {
        // Symbol shards are also in TOON format
        let cached = read_cached_file(&symbol_path)?;
        let entry = symbol_from_json(&cached.json, "");
        return Ok(Some(entry));
    }

    // Fall back to searching through modules
    let modules_dir = cache.modules_dir();
    if modules_dir.exists() {
        for entry in fs::read_dir(&modules_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .map(|e| e == "toon" || e == "json")
                .unwrap_or(false)
            {
                // Use toon_parser to handle both formats
                if let Ok(cached) = read_cached_file(&path) {
                    if let Some(symbols) = cached.json.get("symbols").and_then(|s| s.as_array()) {
                        for sym in symbols {
                            let sym_hash = sym
                                .get("hash")
                                .or_else(|| sym.get("h"))
                                .and_then(|h| h.as_str())
                                .unwrap_or("");
                            if sym_hash == hash
                                || sym_hash.contains(hash)
                                || hash.contains(sym_hash)
                            {
                                let module_name =
                                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                                return Ok(Some(symbol_from_json(sym, module_name)));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Find all symbols in a specific file
fn find_symbols_in_file(
    cache: &CacheDir,
    file_path: &str,
    kind_filter: Option<&str>,
) -> Result<Vec<SymbolIndexEntry>> {
    let mut symbols = Vec::new();

    let modules_dir = cache.modules_dir();
    if !modules_dir.exists() {
        return Ok(symbols);
    }

    for entry in fs::read_dir(&modules_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .map(|e| e == "toon" || e == "json")
            .unwrap_or(false)
        {
            // Use toon_parser to handle both formats
            if let Ok(cached) = read_cached_file(&path) {
                if let Some(sym_array) = cached.json.get("symbols").and_then(|s| s.as_array()) {
                    let module_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                    for sym in sym_array {
                        let sym_file = sym
                            .get("file")
                            .or_else(|| sym.get("f"))
                            .and_then(|f| f.as_str())
                            .unwrap_or("");
                        if sym_file == file_path
                            || sym_file.ends_with(file_path)
                            || file_path.ends_with(sym_file)
                        {
                            let kind = sym
                                .get("kind")
                                .or_else(|| sym.get("k"))
                                .and_then(|k| k.as_str())
                                .unwrap_or("?");

                            if let Some(kf) = kind_filter {
                                if !kind.eq_ignore_ascii_case(kf) {
                                    continue;
                                }
                            }

                            symbols.push(symbol_from_json(sym, module_name));
                        }
                    }
                }
            }
        }
    }

    Ok(symbols)
}

/// Helper to get source for a symbol
fn get_source_for_symbol(
    cache: &CacheDir,
    file: &str,
    lines: &str,
    context: usize,
) -> Option<String> {
    let parts: Vec<&str> = lines.split('-').collect();
    let start: usize = parts.first()?.parse().ok()?;
    let end: usize = parts.get(1).unwrap_or(&parts[0]).parse().ok()?;

    let file_path = cache.repo_root.join(file);
    let content = fs::read_to_string(&file_path).ok()?;
    let all_lines: Vec<&str> = content.lines().collect();

    let start_with_ctx = start.saturating_sub(context + 1);
    let end_with_ctx = (end + context).min(all_lines.len());

    let mut snippet = String::new();
    for (i, line) in all_lines
        .iter()
        .enumerate()
        .skip(start_with_ctx)
        .take(end_with_ctx - start_with_ctx)
    {
        let line_num = i + 1;
        let prefix = if line_num >= start && line_num <= end {
            ">"
        } else {
            " "
        };
        snippet.push_str(&format!("{} {:>4} | {}\n", prefix, line_num, line));
    }

    Some(snippet)
}
