//! Query command handler - Query the semantic index for symbols, source, callers, etc.

use std::fs;
use std::path::PathBuf;

use crate::cache::{CacheDir, SymbolIndexEntry};
use crate::cli::{OutputFormat, QueryArgs, QueryType, SymbolScope};
use crate::commands::toon_parser::read_cached_file;
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::git::{get_current_branch, get_last_commit};

/// Run the query command
pub fn run_query(args: &QueryArgs, ctx: &CommandContext) -> Result<String> {
    match &args.query_type {
        QueryType::Overview {
            path,
            modules,
            max_modules,
            exclude_test_dirs,
            include_git_context,
        } => run_overview(
            path.as_ref(),
            *modules,
            *max_modules,
            *exclude_test_dirs,
            *include_git_context,
            ctx,
        ),
        QueryType::Module {
            name,
            symbols,
            kind,
            risk,
            limit,
            symbol_scope,
            include_escape_refs,
        } => {
            if *symbols {
                run_list_module_symbols(
                    name,
                    kind.as_deref(),
                    risk.as_deref(),
                    *limit,
                    *symbol_scope,
                    *include_escape_refs,
                    ctx,
                )
            } else {
                run_get_module(name, ctx)
            }
        }
        QueryType::Symbol {
            hash,
            path,
            file,
            line,
            source,
            context,
        } => run_get_symbol(
            path.as_ref(),
            hash.as_deref(),
            file.as_deref(),
            *line,
            *source,
            *context,
            ctx,
        ),
        QueryType::Source {
            file,
            path,
            start,
            end,
            hash,
            context,
        } => run_get_source(
            path.as_ref(),
            file.as_deref(),
            *start,
            *end,
            hash.as_deref(),
            *context,
            ctx,
        ),
        QueryType::Callers {
            hash,
            path,
            depth,
            source,
            limit,
        } => run_get_callers(path.as_ref(), hash, *depth, *source, *limit, ctx),
        QueryType::Callgraph {
            path,
            module,
            symbol,
            export,
            stats_only,
            limit,
            offset,
            include_escape_refs,
        } => run_get_callgraph(
            path.as_ref(),
            module.as_deref(),
            symbol.as_deref(),
            export.as_deref(),
            *stats_only,
            *limit,
            *offset,
            *include_escape_refs,
            ctx,
        ),
        QueryType::File {
            path,
            repo_path,
            source,
            kind,
            risk,
            context,
            symbol_scope,
            include_escape_refs,
        } => run_file_symbols(
            repo_path.as_ref(),
            path,
            *source,
            kind.as_deref(),
            risk.as_deref(),
            *context,
            *symbol_scope,
            *include_escape_refs,
            ctx,
        ),
        QueryType::Languages => run_list_languages(ctx),
    }
}

/// Get repository overview (DEDUP-201: unified CLI/MCP handler)
///
/// If path is None, uses the current directory.
pub fn run_overview(
    path: Option<&PathBuf>,
    include_modules: bool,
    max_modules: usize,
    exclude_test_dirs: bool,
    include_git_context: bool,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
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

    // Build git context if requested
    let git_context = if include_git_context {
        build_git_context(&repo_dir)
    } else {
        None
    };

    // Filter modules based on flags
    let filtered_content =
        filter_overview_content(&content, include_modules, max_modules, exclude_test_dirs);

    match ctx.format {
        OutputFormat::Json => {
            // Convert TOON to JSON structure
            let mut json = toon_to_json_overview(&filtered_content);
            if let (Some(ctx), Some(obj)) = (git_context.as_ref(), json.as_object_mut()) {
                obj.insert("git_context".to_string(), ctx.clone());
            }
            Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
        }
        OutputFormat::Toon => {
            // Return TOON format with git context prepended
            let mut output = String::new();
            if let Some(ctx) = git_context {
                output.push_str("git_context:\n");
                if let Some(branch) = ctx.get("branch").and_then(|b| b.as_str()) {
                    output.push_str(&format!("  branch: \"{}\"\n", branch));
                }
                if let Some(commit) = ctx.get("last_commit").and_then(|c| c.as_str()) {
                    output.push_str(&format!("  last_commit: \"{}\"\n", commit));
                }
                output.push('\n');
            }
            output.push_str(&filtered_content);
            Ok(output)
        }
        OutputFormat::Text => {
            // Human-readable text format with header
            let mut output = String::new();
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  REPOSITORY OVERVIEW\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            if let Some(ctx) = git_context {
                if let Some(branch) = ctx.get("branch").and_then(|b| b.as_str()) {
                    output.push_str(&format!("Branch: {}\n", branch));
                }
                if let Some(commit) = ctx.get("last_commit").and_then(|c| c.as_str()) {
                    output.push_str(&format!("Last commit: {}\n", commit));
                }
                output.push('\n');
            }
            output.push_str(&filtered_content);
            Ok(output)
        }
    }
}

/// Build git context information
fn build_git_context(repo_dir: &std::path::Path) -> Option<serde_json::Value> {
    let branch = get_current_branch(Some(repo_dir)).ok();
    let commit = get_last_commit(Some(repo_dir)).map(|c| {
        let msg = format!("{} {}", c.short_sha, c.subject);
        if msg.len() > 60 {
            format!("{}...", &msg[..57])
        } else {
            msg
        }
    });

    if branch.is_none() && commit.is_none() {
        return None;
    }

    let mut ctx = serde_json::Map::new();
    if let Some(b) = branch {
        ctx.insert("branch".to_string(), serde_json::json!(b));
    }
    if let Some(c) = commit {
        ctx.insert("last_commit".to_string(), serde_json::json!(c));
    }
    Some(serde_json::Value::Object(ctx))
}

/// Check if a module name appears to be a test directory
/// Used for filtering test modules from overview output
pub fn is_test_module(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let test_patterns = [
        "test",
        "tests",
        "__test__",
        "__tests__",
        "spec",
        "specs",
        "__spec__",
        "__specs__",
        "mock",
        "mocks",
        "__mock__",
        "__mocks__",
        "fixture",
        "fixtures",
        "__fixture__",
        "__fixtures__",
        "e2e",
        "integration",
        "unit",
    ];

    // Check if name contains test patterns
    for pattern in &test_patterns {
        if name_lower.contains(pattern) {
            return true;
        }
    }

    // Check for .test. or .spec. in the name
    if name_lower.contains(".test.") || name_lower.contains(".spec.") {
        return true;
    }

    false
}

/// Filter overview content based on module flags (DEDUP-201: unified with MCP)
fn filter_overview_content(
    content: &str,
    include_modules: bool,
    max_modules: usize,
    exclude_test_dirs: bool,
) -> String {
    if !include_modules {
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
        return output;
    }

    // Include modules with filtering
    let mut output = String::new();
    let mut in_modules = false;
    let mut modules_collected: Vec<&str> = Vec::new();
    let mut excluded_count = 0;

    for line in content.lines() {
        // Detect modules section start: "modules[N]{...}:"
        if line.starts_with("modules[") && line.ends_with(':') {
            in_modules = true;
            continue;
        }

        if in_modules {
            // Module lines are indented with 2 spaces and contain commas
            if line.starts_with("  ") && line.contains(',') {
                // Parse module name (first field)
                let module_name = line.trim().split(',').next().unwrap_or("");

                // Check if should exclude
                if exclude_test_dirs && is_test_module(module_name) {
                    excluded_count += 1;
                    continue;
                }

                modules_collected.push(line);
            } else {
                // End of modules section - flush collected modules
                in_modules = false;

                // Apply limit
                let final_modules: Vec<&str> = modules_collected
                    .iter()
                    .take(max_modules)
                    .copied()
                    .collect();

                // Write module header with actual count
                let shown = final_modules.len();
                let truncated = modules_collected.len() > max_modules;
                let header = if truncated || excluded_count > 0 {
                    let notes = if excluded_count > 0 && truncated {
                        format!(
                            "excl:{} trunc:{}",
                            excluded_count,
                            modules_collected.len() - shown
                        )
                    } else if excluded_count > 0 {
                        format!("excl:{}", excluded_count)
                    } else {
                        format!("trunc:{}", modules_collected.len() - shown)
                    };
                    format!("modules[{}]{{{}}}:\n", shown, notes)
                } else {
                    format!("modules[{}]:\n", shown)
                };
                output.push_str(&header);

                for m in final_modules {
                    output.push_str(m);
                    output.push('\n');
                }

                // Continue with remaining content
                output.push_str(line);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    // Handle case where modules section was last (no trailing content)
    if in_modules && !modules_collected.is_empty() {
        let final_modules: Vec<&str> = modules_collected
            .iter()
            .take(max_modules)
            .copied()
            .collect();
        let shown = final_modules.len();
        let truncated = modules_collected.len() > max_modules;
        let header = if truncated || excluded_count > 0 {
            let notes = if excluded_count > 0 && truncated {
                format!(
                    "excl:{} trunc:{}",
                    excluded_count,
                    modules_collected.len() - shown
                )
            } else if excluded_count > 0 {
                format!("excl:{}", excluded_count)
            } else {
                format!("trunc:{}", modules_collected.len() - shown)
            };
            format!("modules[{}]{{{}}}:\n", shown, notes)
        } else {
            format!("modules[{}]:\n", shown)
        };
        output.push_str(&header);
        for m in final_modules {
            output.push_str(m);
            output.push('\n');
        }
    }

    output
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
    symbol_scope: SymbolScope,
    include_escape_refs: bool,
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

    let symbol_scope = symbol_scope.for_kind(kind_filter);

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
            if !symbol_scope.matches_kind(&entry.kind) {
                continue;
            }
            if !include_escape_refs && entry.is_escape_local {
                continue;
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

/// Get symbol(s) by hash or file+line location (DEDUP-306: unified CLI/MCP handler)
/// 1. Hash mode: single hash or comma-separated hashes for batch queries
/// 2. File+line mode: find symbol at specific file:line location
/// 3. Combined: file+line takes precedence if both provided
pub fn run_get_symbol(
    path: Option<&PathBuf>,
    hash: Option<&str>,
    file: Option<&str>,
    line: Option<usize>,
    include_source: bool,
    context: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    let mut results: Vec<SymbolIndexEntry> = Vec::new();

    // File+line mode: find symbol at specific location
    if let (Some(file_path), Some(line_num)) = (file, line) {
        let symbol = find_symbol_by_location(&cache, file_path, line_num)?;
        results.push(symbol);
    } else if let Some(hash_str) = hash {
        // Hash mode: handle comma-separated hashes for batch queries
        let hashes: Vec<&str> = hash_str.split(',').map(|s| s.trim()).collect();

        for h in &hashes {
            if let Some(symbol) = load_symbol_from_cache(&cache, h)? {
                results.push(symbol);
            }
        }

        if results.is_empty() {
            return Err(McpDiffError::FileNotFound {
                path: format!("Symbol(s) not found: {}", hash_str),
            });
        }
    } else {
        return Err(McpDiffError::GitError {
            message: "Either hash or file+line must be provided".to_string(),
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
                        get_source_for_symbol(&cache, &symbol.file, &symbol.lines, context)
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

/// Find symbol at a specific file:line location
fn find_symbol_by_location(
    cache: &CacheDir,
    file_path: &str,
    line: usize,
) -> Result<SymbolIndexEntry> {
    let entries = cache
        .load_all_symbol_entries()
        .map_err(|e| McpDiffError::FileNotFound {
            path: format!("Failed to load symbol index: {}", e),
        })?;

    entries
        .into_iter()
        .find(|e| {
            // Check if file matches (allow partial path matching)
            if !e.file.ends_with(file_path) && !file_path.ends_with(&e.file) {
                return false;
            }
            // Check if line is within range
            if let Some((start, end)) = e.lines.split_once('-') {
                if let (Ok(s), Ok(en)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    return line >= s && line <= en;
                }
            }
            false
        })
        .ok_or_else(|| McpDiffError::FileNotFound {
            path: format!("No symbol found at {}:{}", file_path, line),
        })
}

/// Get source code for a file or symbol(s) (DEDUP-306: unified CLI/MCP handler)
/// Supports three modes:
/// 1. Batch mode: comma-separated hashes (get source for each)
/// 2. Single hash mode: get source for one symbol by hash
/// 3. File mode: file + start/end lines
pub fn run_get_source(
    path: Option<&PathBuf>,
    file: Option<&str>,
    start: Option<usize>,
    end: Option<usize>,
    hash: Option<&str>,
    context: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Batch mode: comma-separated hashes
    if let Some(hash_str) = hash {
        if hash_str.contains(',') {
            // Batch mode
            let hashes: Vec<&str> = hash_str.split(',').map(|s| s.trim()).take(20).collect();
            return run_batch_source(&cache, &hashes, context, ctx);
        }

        // Single hash mode: get file info from symbol
        if let Some(symbol) = load_symbol_from_cache(&cache, hash_str)? {
            let parts: Vec<&str> = symbol.lines.split('-').collect();
            let actual_start: usize = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1);
            let actual_end: usize = parts
                .get(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(actual_start);
            return format_file_source(
                &repo_dir,
                &symbol.file,
                actual_start,
                actual_end,
                context,
                ctx,
            );
        } else {
            return Err(McpDiffError::FileNotFound {
                path: format!("Symbol not found: {}", hash_str),
            });
        }
    }

    // File mode: requires file path
    let file_str = file.ok_or_else(|| McpDiffError::GitError {
        message: "Either file or hash must be provided".to_string(),
    })?;

    let actual_start = start.unwrap_or(1);
    let actual_end = end.unwrap_or(actual_start + 50);

    format_file_source(&repo_dir, file_str, actual_start, actual_end, context, ctx)
}

/// Get source for multiple symbols by hash (batch mode)
fn run_batch_source(
    cache: &CacheDir,
    hashes: &[&str],
    context: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let mut found: Vec<serde_json::Value> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();

    for hash in hashes {
        if let Some(symbol) = load_symbol_from_cache(cache, hash)? {
            // Get source for this symbol
            let parts: Vec<&str> = symbol.lines.split('-').collect();
            let start: usize = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1);
            let end: usize = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(start);

            if let Some(source) = get_source_for_symbol(cache, &symbol.file, &symbol.lines, context)
            {
                found.push(serde_json::json!({
                    "hash": hash,
                    "symbol": symbol.symbol,
                    "file": symbol.file,
                    "lines": format!("{}-{}", start, end),
                    "source": source
                }));
            } else {
                not_found.push(hash.to_string());
            }
        } else {
            not_found.push(hash.to_string());
        }
    }

    let json_value = serde_json::json!({
        "_type": "batch_source",
        "requested": hashes.len(),
        "found": found.len(),
        "sources": found,
        "not_found": not_found
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
            output.push_str(&format!(
                "Batch source: {} requested, {} found\n",
                hashes.len(),
                found.len()
            ));
            output.push_str("═══════════════════════════════════════════\n\n");

            for item in &found {
                if let (Some(hash), Some(symbol), Some(source)) = (
                    item.get("hash").and_then(|v| v.as_str()),
                    item.get("symbol").and_then(|v| v.as_str()),
                    item.get("source").and_then(|v| v.as_str()),
                ) {
                    output.push_str(&format!("--- {} ({}) ---\n", hash, symbol));
                    output.push_str(source);
                    output.push_str("\n\n");
                }
            }

            if !not_found.is_empty() {
                output.push_str(&format!("Not found: {}\n", not_found.join(", ")));
            }
        }
    }

    Ok(output)
}

/// Format source for a specific file and line range
fn format_file_source(
    repo_dir: &std::path::Path,
    file: &str,
    actual_start: usize,
    actual_end: usize,
    context: usize,
    ctx: &CommandContext,
) -> Result<String> {
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

/// Get callers of a symbol (DEDUP-306: unified CLI/MCP handler)
pub fn run_get_callers(
    path: Option<&PathBuf>,
    hash: &str,
    depth: usize,
    include_source: bool,
    limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    use std::collections::{HashMap, HashSet};

    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Load call graph
    let call_graph = cache.load_call_graph()?;
    if call_graph.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: "Call graph not found or empty. Run `semfora index generate` first.".to_string(),
        });
    }

    // Build reverse call graph (callee -> callers)
    let mut reverse_graph: HashMap<String, Vec<String>> = HashMap::new();
    for (caller, callees) in &call_graph {
        for callee in callees {
            // Skip external calls
            if !callee.starts_with("ext:") {
                reverse_graph
                    .entry(callee.clone())
                    .or_default()
                    .push(caller.clone());
            }
        }
    }

    // Load symbol entries for resolution
    let mut symbol_names: HashMap<String, String> = HashMap::new();
    let mut target_entry: Option<SymbolIndexEntry> = None;
    if let Ok(entries) = cache.load_all_symbol_entries() {
        for entry in entries {
            if entry.hash == hash {
                target_entry = Some(entry.clone());
            }
            symbol_names.insert(entry.hash.clone(), entry.symbol.clone());
        }
    }

    // Get target name and framework entry point
    let target_name = symbol_names
        .get(hash)
        .cloned()
        .unwrap_or_else(|| hash.to_string());
    let target_fep = target_entry
        .as_ref()
        .map(|e| e.framework_entry_point)
        .unwrap_or_default();

    // BFS to find callers at each depth level
    let depth = depth.min(3); // Max depth 3 like MCP
    let mut all_callers: Vec<(String, String, usize)> = Vec::new(); // (hash, name, depth)
    let mut visited: HashSet<String> = HashSet::new();
    let mut current_level: Vec<String> = vec![hash.to_string()];

    for current_depth in 1..=depth {
        let mut next_level: Vec<String> = Vec::new();

        for h in &current_level {
            if let Some(callers) = reverse_graph.get(h) {
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
    let callers_json: Vec<serde_json::Value> = all_callers
        .iter()
        .map(|(h, n, d)| {
            serde_json::json!({
                "hash": h,
                "name": n,
                "depth": d
            })
        })
        .collect();

    // Include framework entry point info for no-caller symbols
    let fep_str = if target_fep.is_none() {
        None
    } else {
        Some(format!("{:?}", target_fep).to_lowercase())
    };

    let json_value = serde_json::json!({
        "_type": "callers",
        "target": target_name,
        "target_hash": hash,
        "depth": depth,
        "callers": callers_json,
        "count": all_callers.len(),
        "framework_entry_point": fep_str,
        "is_framework_entry_point": !target_fep.is_none()
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output.push_str(&super::toon_header("callers"));
            output.push_str(&format!("target: {} ({})\n", target_name, hash));
            output.push_str(&format!("depth: {}\n", depth));
            output.push_str(&format!("total_callers: {}\n", all_callers.len()));

            if all_callers.is_empty() {
                if !target_fep.is_none() {
                    output.push_str(&format!(
                        "callers: (none - {} framework entry point)\n",
                        format!("{:?}", target_fep).to_lowercase()
                    ));
                } else {
                    output
                        .push_str("callers: (none - may be unused or an undetected entry point)\n");
                }
            } else {
                output.push_str(&format!(
                    "callers[{}]{{name,hash,depth}}:\n",
                    all_callers.len()
                ));
                for (h, n, d) in &all_callers {
                    output.push_str(&format!("  {},{},{}\n", n, h, d));
                }
            }

            // Include source snippets if requested (for MCP parity)
            if include_source && !all_callers.is_empty() {
                output.push_str("\n__caller_sources__:\n");
                for (caller_hash, caller_name, _) in all_callers.iter().take(5) {
                    if let Some(symbol) = load_symbol_from_cache(&cache, caller_hash)? {
                        if let Some(source) =
                            get_source_for_symbol(&cache, &symbol.file, &symbol.lines, 1)
                        {
                            output
                                .push_str(&format!("--- {} ({}) ---\n", caller_name, caller_hash));
                            output.push_str(&format!("# {}:{}\n", symbol.file, symbol.lines));
                            for line in source.lines().take(5) {
                                output.push_str(&format!("{}\n", line));
                            }
                        }
                    }
                }
            }
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  CALLERS\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("target: {} ({})\n", target_name, hash));
            output.push_str(&format!("depth: {}\n", depth));
            output.push_str(&format!("callers[{}]:\n", all_callers.len()));

            if all_callers.is_empty() {
                if !target_fep.is_none() {
                    output.push_str(&format!(
                        "  (none - {} framework entry point)\n",
                        format!("{:?}", target_fep).to_lowercase()
                    ));
                } else {
                    output.push_str("  (none - may be unused or an undetected entry point)\n");
                }
            } else {
                for (caller_hash, caller_name, d) in &all_callers {
                    output.push_str(&format!("  [d{}] {} ({})\n", d, caller_name, caller_hash));

                    if include_source {
                        if let Some(symbol) = load_symbol_from_cache(&cache, caller_hash)? {
                            if let Some(source) =
                                get_source_for_symbol(&cache, &symbol.file, &symbol.lines, 1)
                            {
                                output.push_str(&format!(
                                    "       # {}:{}\n",
                                    symbol.file, symbol.lines
                                ));
                                for line in source.lines().take(5) {
                                    output.push_str(&format!("       {}\n", line));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Get call graph (DEDUP-306: unified CLI/MCP handler)
/// Supports: module filtering, symbol filtering, pagination, stats mode, SQLite export
#[allow(clippy::too_many_arguments)]
pub fn run_get_callgraph(
    path: Option<&PathBuf>,
    module: Option<&str>,
    symbol: Option<&str>,
    export: Option<&str>,
    stats_only: bool,
    limit: usize,
    offset: usize,
    include_escape_refs: bool,
    ctx: &CommandContext,
) -> Result<String> {
    use std::collections::{HashMap, HashSet};

    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Handle SQLite export
    if let Some(export_path) = export {
        return run_export_sqlite(export_path, &cache, include_escape_refs, ctx);
    }

    // Load call graph using TOON parser
    let call_graph = cache.load_call_graph()?;
    if call_graph.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: "Call graph not found or empty. Run `semfora index generate` first.".to_string(),
        });
    }

    let call_graph = filter_escape_edges(call_graph, include_escape_refs);

    // Build hash-to-name mapping for symbol resolution
    let hash_to_name: HashMap<String, String> = cache
        .load_all_symbol_entries()
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.hash, e.symbol))
        .collect();

    // Resolve symbol filter to matching hashes (enables name-based lookup)
    let resolved_symbol_hashes: Option<HashSet<String>> = if let Some(sym) = symbol {
        let sym_lower = sym.to_lowercase();

        // Check if it looks like a hash
        let is_hash_like =
            sym.contains(':') && sym.chars().all(|c| c.is_ascii_hexdigit() || c == ':');

        if is_hash_like {
            Some(std::iter::once(normalize_edge_hash(sym)).collect())
        } else {
            // Search by name - first try exact match, then partial
            let exact_matches: HashSet<String> = hash_to_name
                .iter()
                .filter(|(_, name)| name.to_lowercase() == sym_lower)
                .map(|(hash, _)| hash.clone())
                .collect();

            if !exact_matches.is_empty() {
                Some(exact_matches)
            } else {
                // Fallback to partial match
                let partial_matches: HashSet<String> = hash_to_name
                    .iter()
                    .filter(|(_, name)| name.to_lowercase().contains(&sym_lower))
                    .map(|(hash, _)| hash.clone())
                    .collect();

                Some(partial_matches)
            }
        }
    } else {
        None
    };

    // Check if symbol filter was provided but resolved to empty set
    if let Some(ref hashes) = resolved_symbol_hashes {
        if hashes.is_empty() {
            if let Some(sym) = symbol {
                return Err(McpDiffError::FileNotFound {
                    path: format!("No symbol found matching: {}", sym),
                });
            }
        }
    }

    // Total edge count
    let total_edges = call_graph.len();
    let total_calls: usize = call_graph.values().map(|v| v.len()).sum();

    if stats_only {
        // Collect top callers by fan-out
        let mut caller_stats: Vec<(&String, usize)> = call_graph
            .iter()
            .map(|(caller, callees)| (caller, callees.len()))
            .filter(|(_, count)| *count > 5)
            .collect();
        caller_stats.sort_by(|a, b| b.1.cmp(&a.1));
        caller_stats.truncate(15);

        let top_callers: Vec<serde_json::Value> = caller_stats
            .iter()
            .map(|(hash, count)| {
                let name = hash_to_name.get(*hash).map(|n| n.as_str()).unwrap_or(*hash);
                serde_json::json!({
                    "symbol": name,
                    "callees": count
                })
            })
            .collect();

        let json_value = serde_json::json!({
            "_type": "call_graph_summary",
            "total_callers": total_edges,
            "total_call_edges": total_calls,
            "avg_callees_per_caller": total_calls as f64 / total_edges.max(1) as f64,
            "module_filter": module,
            "symbol_filter": symbol,
            "top_callers_by_fan_out": top_callers
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
                output.push_str("  CALL GRAPH SUMMARY\n");
                output.push_str("═══════════════════════════════════════════\n\n");
                output.push_str(&format!("total_callers: {}\n", total_edges));
                output.push_str(&format!("total_call_edges: {}\n", total_calls));
                output.push_str(&format!(
                    "avg_callees_per_caller: {:.1}\n",
                    total_calls as f64 / total_edges.max(1) as f64
                ));
                if let Some(m) = module {
                    output.push_str(&format!("module_filter: {}\n", m));
                }
                if let Some(s) = symbol {
                    output.push_str(&format!("symbol_filter: {}\n", s));
                }

                if !caller_stats.is_empty() {
                    output.push_str("\ntop_callers_by_fan_out:\n");
                    for (hash, count) in &caller_stats {
                        let name = hash_to_name.get(*hash).map(|n| n.as_str()).unwrap_or(*hash);
                        output.push_str(&format!("  {} ({} callees)\n", name, count));
                    }
                }
            }
        }

        return Ok(output);
    }

    // Filter and paginate edges
    let filtered_edges: Vec<(&String, &Vec<String>)> = call_graph
        .iter()
        .filter(|(caller, callees)| {
            let module_match = module
                .map(|m| caller.contains(m) || callees.iter().any(|c| c.contains(m)))
                .unwrap_or(true);

            // Use resolved symbol hashes for filtering
            let symbol_match = if let Some(ref hashes) = resolved_symbol_hashes {
                hashes.contains(*caller)
                    || callees.iter().any(|c| {
                        let edge = crate::schema::CallGraphEdge::decode(c);
                        hashes.contains(&edge.callee)
                    })
            } else {
                true
            };

            module_match && symbol_match
        })
        .skip(offset)
        .take(limit)
        .collect();

    let filtered_count = filtered_edges.len();

    // Resolve hashes to names for display
    let edges_json: Vec<serde_json::Value> = filtered_edges
        .iter()
        .map(|(caller_hash, callee_hashes)| {
            let caller_name = hash_to_name
                .get(*caller_hash)
                .map(|n| n.as_str())
                .unwrap_or(*caller_hash);
            let callee_names: Vec<String> = callee_hashes
                .iter()
                .map(|h| format_callee_display(h, &hash_to_name))
                .collect();
            serde_json::json!({
                "caller": caller_name,
                "caller_hash": caller_hash,
                "callees": callee_names,
                "callee_count": callee_hashes.len()
            })
        })
        .collect();

    let json_value = serde_json::json!({
        "_type": "call_graph",
        "total_edges": total_edges,
        "filtered_count": filtered_count,
        "offset": offset,
        "limit": limit,
        "edges": edges_json,
        "has_more": filtered_count == limit
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
            output.push_str(&format!(
                "edges[{}] (offset: {}, limit: {}):\n",
                filtered_count, offset, limit
            ));
            for (caller_hash, callees) in &filtered_edges {
                let caller_name = hash_to_name
                    .get(*caller_hash)
                    .map(|n| n.as_str())
                    .unwrap_or(*caller_hash);
                // Show caller -> [callees] format with resolved names
                let callees_display: Vec<String> = callees
                    .iter()
                    .take(3)
                    .map(|h| format_callee_display(h, &hash_to_name))
                    .collect();
                let callees_str = if callees.len() > 3 {
                    format!(
                        "[{}, ... +{} more]",
                        callees_display.join(", "),
                        callees.len() - 3
                    )
                } else {
                    format!("[{}]", callees_display.join(", "))
                };
                output.push_str(&format!("  {} -> {}\n", caller_name, callees_str));
            }
            if filtered_count == limit {
                output.push_str(&format!(
                    "\nhint: use --offset {} for next page\n",
                    offset + limit
                ));
            }
        }
    }

    Ok(output)
}

fn filter_escape_edges(
    call_graph: std::collections::HashMap<String, Vec<String>>,
    include_escape_refs: bool,
) -> std::collections::HashMap<String, Vec<String>> {
    if include_escape_refs {
        return call_graph;
    }

    let mut filtered = std::collections::HashMap::new();

    for (caller, callees) in call_graph {
        let kept: Vec<String> = callees
            .into_iter()
            .filter(|callee| {
                let edge = crate::schema::CallGraphEdge::decode(callee);
                !edge.edge_kind.is_escape_ref()
            })
            .collect();

        if !kept.is_empty() {
            filtered.insert(caller, kept);
        }
    }

    filtered
}

fn normalize_edge_hash(sym: &str) -> String {
    crate::schema::CallGraphEdge::decode(sym).callee
}

fn format_callee_display(
    callee: &str,
    hash_to_name: &std::collections::HashMap<String, String>,
) -> String {
    let edge = crate::schema::CallGraphEdge::decode(callee);
    let base = hash_to_name
        .get(&edge.callee)
        .map(|n| n.as_str())
        .unwrap_or(edge.callee.as_str());

    if edge.edge_kind == crate::schema::RefKind::None {
        base.to_string()
    } else {
        format!("{}:{}", base, edge.edge_kind.as_edge_kind())
    }
}

/// Export call graph to SQLite
fn run_export_sqlite(
    path: &str,
    cache: &CacheDir,
    include_escape_refs: bool,
    _ctx: &CommandContext,
) -> Result<String> {
    use crate::sqlite_export::{default_export_path, SqliteExporter};

    let output_path = if path.is_empty() {
        default_export_path(cache)
    } else {
        std::path::PathBuf::from(path)
    };

    eprintln!("Exporting call graph to: {}", output_path.display());

    let exporter = SqliteExporter::new();
    let stats = exporter.export(cache, &output_path, None, include_escape_refs)?;

    Ok(format!(
        "Export complete:\n  Path: {}\n  Nodes: {}\n  Edges: {}\n  Size: {} bytes",
        output_path.display(),
        stats.nodes_inserted,
        stats.edges_inserted,
        stats.file_size_bytes
    ))
}

/// Get all symbols in a file (DEDUP-306: unified CLI/MCP handler)
/// Supports kind filtering, risk filtering, and source code inclusion
#[allow(clippy::too_many_arguments)]
pub fn run_file_symbols(
    repo_path: Option<&PathBuf>,
    file_path: &str,
    include_source: bool,
    kind_filter: Option<&str>,
    risk_filter: Option<&str>,
    context: usize,
    symbol_scope: SymbolScope,
    include_escape_refs: bool,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = match repo_path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Load all symbol entries and filter by file
    let target_file = file_path.trim_start_matches("./");
    let symbol_scope = symbol_scope.for_kind(kind_filter);

    let symbols: Vec<SymbolIndexEntry> = cache
        .load_all_symbol_entries()
        .map_err(|e| McpDiffError::FileNotFound {
            path: format!("Failed to load symbol index: {}", e),
        })?
        .into_iter()
        .filter(|e| {
            let entry_file = e.file.trim_start_matches("./");
            entry_file == target_file
                || entry_file.ends_with(target_file)
                || target_file.ends_with(entry_file)
        })
        .filter(|e| {
            kind_filter.map_or(true, |k| {
                e.kind.to_lowercase() == k.to_lowercase()
                    || e.kind.to_lowercase().contains(&k.to_lowercase())
            })
        })
        .filter(|e| risk_filter.map_or(true, |r| e.risk.to_lowercase() == r.to_lowercase()))
        .filter(|e| symbol_scope.matches_kind(&e.kind))
        .filter(|e| include_escape_refs || !e.is_escape_local)
        .collect();

    if symbols.is_empty() {
        // Respect output format even for empty results
        return match ctx.format {
            OutputFormat::Json => Ok(serde_json::json!({
                "_type": "file_symbols",
                "file": file_path,
                "count": 0,
                "symbols": [],
                "hint": "File may not be indexed or path doesn't match."
            }).to_string()),
            OutputFormat::Toon | OutputFormat::Text => Ok(format!(
                "{}file: \"{}\"\nshowing: 0\nsymbols: (none)\nhint: File may not be indexed or path doesn't match.\n",
                super::toon_header("file_symbols"),
                file_path
            )),
        };
    }

    // Build JSON representation
    let symbols_json: Vec<serde_json::Value> = symbols
        .iter()
        .map(|sym| {
            serde_json::json!({
                "name": sym.symbol,
                "hash": sym.hash,
                "kind": sym.kind,
                "lines": sym.lines,
                "risk": sym.risk,
                "module": sym.module
            })
        })
        .collect();

    let json_value = serde_json::json!({
        "_type": "file_symbols",
        "file": file_path,
        "count": symbols.len(),
        "kind_filter": kind_filter,
        "risk_filter": risk_filter,
        "symbols": symbols_json
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            // Compact TOON format with key columns
            output.push_str(&super::toon_header("file_symbols"));
            output.push_str(&format!("file: \"{}\"\n", file_path));
            output.push_str(&format!("showing: {}\n", symbols.len()));
            output.push_str(&format!(
                "symbols[{}]{{name,hash,kind,lines,risk}}:\n",
                symbols.len()
            ));

            for sym in &symbols {
                output.push_str(&format!(
                    "  {},{},{},{},{}\n",
                    sym.symbol, sym.hash, sym.kind, sym.lines, sym.risk
                ));
            }

            // Include source if requested
            if include_source && !symbols.is_empty() {
                output.push_str("\n__sources__:\n");
                for sym in &symbols {
                    if let Some(source) =
                        get_source_for_symbol(&cache, &sym.file, &sym.lines, context)
                    {
                        output.push_str(&format!("\n--- {} ({}) ---\n", sym.symbol, sym.lines));
                        output.push_str(&source);
                    }
                }
            }
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str(&format!("  FILE: {}\n", file_path));
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("symbols[{}]:\n", symbols.len()));

            for sym in &symbols {
                output.push_str(&format!(
                    "  {} ({}) L{} [{}]\n",
                    sym.symbol, sym.kind, sym.lines, sym.risk
                ));
                output.push_str(&format!("    hash: {}\n", sym.hash));

                if include_source {
                    if let Some(source) =
                        get_source_for_symbol(&cache, &sym.file, &sym.lines, context)
                    {
                        output.push_str("    source:\n");
                        for line in source.lines().take(5 + context) {
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
        is_escape_local: sym
            .get("is_escape_local")
            .or_else(|| sym.get("el"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        framework_entry_point: sym
            .get("framework_entry_point")
            .or_else(|| sym.get("fep"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        is_exported: sym
            .get("is_exported")
            .or_else(|| sym.get("exp"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        decorators: sym
            .get("decorators")
            .or_else(|| sym.get("dec"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        arity: sym
            .get("arity")
            .or_else(|| sym.get("ar"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        is_async: sym
            .get("is_async")
            .or_else(|| sym.get("async"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        return_type: sym
            .get("return_type")
            .or_else(|| sym.get("rt"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        ext_package: sym
            .get("ext_package")
            .or_else(|| sym.get("pkg"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_classes: sym
            .get("base_classes")
            .or_else(|| sym.get("bc"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
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
