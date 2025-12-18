//! Formatting helpers for MCP tool output
//!
//! This module contains all the formatting functions used to convert internal
//! data structures into the TOON (Token-Optimized Object Notation) format
//! that is returned by MCP tools.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::duplicate::{DuplicateCluster, DuplicateKind, DuplicateMatch, FunctionSignature};
use crate::schema::SCHEMA_VERSION;
use crate::security::{CVEMatch, CVEScanSummary, Severity};
use crate::utils::truncate_to_char_boundary;
use crate::{encode_toon, CacheDir, Lang, MergedBlock, RipgrepSearchResult, SemanticSummary, SymbolIndexEntry};

use super::helpers::parse_and_extract;
use super::GetSourceRequest;

// ============================================================================
// Analysis Helpers
// ============================================================================

/// Analyze a collection of files and return their semantic summaries
pub(super) fn analyze_files(files: &[std::path::PathBuf]) -> Vec<SemanticSummary> {
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
pub(super) fn format_diff_output(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "diff: {} -> {} ({} files)\n\n",
        base_ref, target_ref, changed_files.len()
    ));

    for changed_file in changed_files {
        let full_path = working_dir.join(&changed_file.path);

        output.push_str(&format!(
            "--- {} [{}] ---\n",
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

/// Format diff output with pagination support - TOON format
/// Returns paginated file analysis with semantic summaries
pub(super) fn format_diff_output_paginated(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
    offset: usize,
    limit: usize,
) -> String {
    use std::collections::HashMap;

    let total_files = changed_files.len();

    // Apply pagination
    let page_files: Vec<_> = changed_files
        .iter()
        .skip(offset)
        .take(limit)
        .collect();

    let mut output = String::new();

    // TOON header with pagination metadata
    output.push_str("_type: analyze_diff\n");
    output.push_str(&format!("base: \"{}\"\n", base_ref));
    output.push_str(&format!("target: \"{}\"\n", target_ref));
    output.push_str(&format!("total_files: {}\n", total_files));
    output.push_str(&format!("showing: {}\n", page_files.len()));
    output.push_str(&format!("offset: {}\n", offset));
    output.push_str(&format!("limit: {}\n", limit));

    // Pagination hint
    if offset + page_files.len() < total_files {
        output.push_str(&format!(
            "next_offset: {} (use offset={} for next page)\n",
            offset + page_files.len(),
            offset + limit
        ));
    }

    // Count change types for summary (BTreeMap for deterministic order)
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for f in changed_files {
        *by_type.entry(f.change_type.as_str()).or_insert(0) += 1;
    }
    let type_summary: Vec<_> = by_type.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("changes: {}\n", type_summary.join(", ")));

    if page_files.is_empty() {
        if total_files == 0 {
            output.push_str("\n_note: No files changed.\n");
        } else {
            output.push_str(&format!(
                "\n_note: No files at offset {}. Total: {}.\n",
                offset, total_files
            ));
        }
        return output;
    }

    output.push_str(&format!("\nfiles[{}]:\n", page_files.len()));

    // Format each file in the page
    for changed_file in page_files {
        let full_path = working_dir.join(&changed_file.path);

        output.push_str(&format!(
            "  {} [{}]\n",
            changed_file.path,
            changed_file.change_type.as_str()
        ));

        if changed_file.change_type == crate::git::ChangeType::Deleted {
            output.push_str("    (deleted)\n");
            continue;
        }

        let lang = match Lang::from_path(&full_path) {
            Ok(l) => l,
            Err(_) => {
                output.push_str("    (unsupported)\n");
                continue;
            }
        };

        let source = match fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => {
                output.push_str("    (unreadable)\n");
                continue;
            }
        };

        match parse_and_extract(&full_path, &source, lang) {
            Ok(summary) => {
                // Indent the TOON output
                let toon = encode_toon(&summary);
                for line in toon.lines() {
                    output.push_str(&format!("    {}\n", line));
                }
            }
            Err(e) => {
                output.push_str(&format!("    (error: {})\n", e));
            }
        }
    }

    output
}

/// Format diff summary only - compact overview without per-file details
/// Returns aggregate statistics for large diffs
pub(super) fn format_diff_summary(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
) -> String {
    use std::collections::HashMap;

    let mut output = String::new();

    // TOON header
    output.push_str("_type: analyze_diff_summary\n");
    output.push_str(&format!("base: \"{}\"\n", base_ref));
    output.push_str(&format!("target: \"{}\"\n", target_ref));
    output.push_str(&format!("total_files: {}\n", changed_files.len()));

    if changed_files.is_empty() {
        output.push_str("_note: No files changed.\n");
        return output;
    }

    // Count by change type (BTreeMap for deterministic order)
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for f in changed_files {
        *by_type.entry(f.change_type.as_str()).or_insert(0) += 1;
    }
    let type_summary: Vec<_> = by_type.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("by_change_type: {}\n", type_summary.join(", ")));

    // Count by language/extension
    let mut by_lang: HashMap<String, usize> = HashMap::new();
    for f in changed_files {
        let full_path = working_dir.join(&f.path);
        let lang_name = match Lang::from_path(&full_path) {
            Ok(l) => format!("{:?}", l),
            Err(_) => {
                // Use extension as fallback
                full_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "other".to_string())
            }
        };
        *by_lang.entry(lang_name).or_insert(0) += 1;
    }
    // Sort by count descending, take top 10
    let mut lang_vec: Vec<_> = by_lang.iter().collect();
    lang_vec.sort_by(|a, b| b.1.cmp(a.1));
    let lang_summary: Vec<_> = lang_vec.iter()
        .take(10)
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("by_language: {}\n", lang_summary.join(", ")));

    // Group by module (directory)
    let mut by_module: HashMap<String, usize> = HashMap::new();
    for f in changed_files {
        let module = std::path::Path::new(&f.path)
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| if s.is_empty() { "(root)" } else { s })
            .unwrap_or("(root)")
            .to_string();
        *by_module.entry(module).or_insert(0) += 1;
    }
    // Sort by count descending, take top 10
    let mut module_vec: Vec<_> = by_module.iter().collect();
    module_vec.sort_by(|a, b| b.1.cmp(a.1));
    let module_summary: Vec<_> = module_vec.iter()
        .take(10)
        .map(|(k, v)| format!("{} ({})", k, v))
        .collect();
    output.push_str(&format!("top_modules: {}\n", module_summary.join(", ")));

    // Quick risk assessment based on file types and locations
    let mut high_risk = 0;
    let mut medium_risk = 0;
    let mut low_risk = 0;

    for f in changed_files {
        let path_lower = f.path.to_lowercase();
        // Check for security-sensitive patterns (more specific to avoid false positives)
        let has_key_pattern = path_lower.contains("api_key")
            || path_lower.contains("apikey")
            || path_lower.contains("private_key")
            || path_lower.contains("secret_key")
            || path_lower.contains("encryption_key")
            || path_lower.contains("/keys/")
            || path_lower.ends_with("_key.rs")
            || path_lower.ends_with("_key.ts")
            || path_lower.ends_with("_key.py");
        if path_lower.contains("auth")
            || path_lower.contains("security")
            || path_lower.contains("crypt")
            || path_lower.contains("password")
            || path_lower.contains("secret")
            || has_key_pattern
            || path_lower.contains(".env")
        {
            high_risk += 1;
        } else if path_lower.contains("config")
            || path_lower.contains("api")
            || path_lower.contains("database")
            || path_lower.contains("migration")
            || path_lower.contains("schema")
        {
            medium_risk += 1;
        } else {
            low_risk += 1;
        }
    }
    output.push_str(&format!(
        "risk_estimate: high={}, medium={}, low={}\n",
        high_risk, medium_risk, low_risk
    ));

    // Hint for getting details
    output.push_str("\n_hint: Use limit/offset params to paginate file details, or omit summary_only for full analysis.\n");

    output
}

/// Get the list of supported languages as a formatted string
pub(super) fn get_supported_languages() -> String {
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

// ============================================================================
// Source Code Helpers
// ============================================================================

/// Resolve line range from request (either direct or via symbol hash lookup)
pub(super) async fn resolve_line_range(
    file_path: &Path,
    request: &GetSourceRequest,
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
pub(super) fn find_cache_for_path(start_path: &Path) -> Result<CacheDir, String> {
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
pub(super) fn extract_source_for_symbol(cache: &CacheDir, symbol_content: &str, context: usize) -> Option<String> {
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
pub(super) fn format_source_snippet(
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

// ============================================================================
// Search Result Formatting
// ============================================================================

/// Format search results as compact TOON
pub(super) fn format_search_results(query: &str, results: &[SymbolIndexEntry]) -> String {
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
pub(super) fn format_ripgrep_results(query: &str, results: &[RipgrepSearchResult]) -> String {
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
pub(super) fn format_working_overlay_results(query: &str, results: &[RipgrepSearchResult]) -> String {
    let mut output = String::new();
    output.push_str("_type: working_overlay_results\n");
    output.push_str("_note: Searching uncommitted files only (staged + unstaged changes)\n");
    output.push_str(&format!("query: \"{}\"\n", query));
    output.push_str(&format!("showing: {}\n", results.len()));

    if results.is_empty() {
        output.push_str("results: (none - no uncommitted files match)\n");
    } else {
        // Group by file for cleaner output
        let mut by_file: BTreeMap<&str, Vec<&RipgrepSearchResult>> = BTreeMap::new();
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
pub(super) fn format_merged_blocks(query: &str, blocks: &[MergedBlock], search_path: &Path) -> String {
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
pub(super) fn format_module_symbols(module: &str, results: &[SymbolIndexEntry], cache: &CacheDir) -> String {
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
// Call Graph Formatting
// ============================================================================

/// Format call graph with pagination
/// If hash_to_name is provided, displays "name (hash)" instead of just hash
pub(super) fn format_call_graph_paginated(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
    offset: usize,
    limit: usize,
    hash_to_name: Option<&std::collections::HashMap<String, String>>,
) -> String {
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

    // Helper to format a hash with optional name
    let format_symbol = |hash: &str| -> String {
        if let Some(names) = hash_to_name {
            if let Some(name) = names.get(hash) {
                return format!("{} ({})", name, hash);
            }
        }
        hash.to_string()
    };

    for (caller, callees) in edges {
        let caller_display = format_symbol(caller);
        let callees_str = callees.iter()
            .map(|c| {
                if c.starts_with("ext:") {
                    format!("\"{}\"", c) // External calls stay as-is
                } else {
                    format!("\"{}\"", format_symbol(c))
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        output.push_str(&format!("{}: [{}]\n", caller_display, callees_str));
    }

    output
}

/// Format call graph summary (statistics only, no edges)
/// If hash_to_name is provided, displays "name (hash)" for top callers/callees
pub(super) fn format_call_graph_summary(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
    hash_to_name: Option<&std::collections::HashMap<String, String>>,
) -> String {
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

    // Helper to format a hash with optional name
    let format_symbol = |hash: &str| -> String {
        if let Some(names) = hash_to_name {
            if let Some(name) = names.get(hash) {
                return format!("{} ({})", name, hash);
            }
        }
        hash.to_string()
    };

    // Top callers (symbols that make the most calls)
    let mut callers_by_count: Vec<_> = edges.iter()
        .map(|(caller, callees)| (caller.clone(), callees.len()))
        .collect();
    callers_by_count.sort_by(|a, b| b.1.cmp(&a.1));

    output.push_str("\ntop_callers[10]:\n");
    for (caller, count) in callers_by_count.iter().take(10) {
        output.push_str(&format!("  - {} (calls: {})\n", format_symbol(caller), count));
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
        let callee_display = if callee.starts_with("ext:") {
            callee.clone()
        } else {
            format_symbol(callee)
        };
        output.push_str(&format!("  - {} (called: {} times)\n", callee_display, count));
    }

    // Leaf functions (call nothing)
    let leaf_count = edges.iter().filter(|(_, callees)| callees.is_empty()).count();
    output.push_str(&format!("\nleaf_functions: {}\n", leaf_count));

    output
}

// ============================================================================
// Duplicate Detection Formatting
// ============================================================================

/// Load function signatures from the signature index
pub(super) fn load_signatures(cache: &CacheDir) -> Result<Vec<FunctionSignature>, String> {
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

/// Format duplicate clusters as compact TOON output
pub(super) fn format_duplicate_clusters(clusters: &[DuplicateCluster], threshold: f64) -> String {
    format_duplicate_clusters_paginated(clusters, threshold, clusters.len(), 0, clusters.len(), "similarity")
}

/// Extract a short module name from a file path
/// e.g., "/home/user/project/src/Presentation/Nop.Web/Factories/ProductModelFactory.cs"
///    -> "Nop.Web.Factories"
fn extract_module_name(file_path: &str) -> String {
    let path = Path::new(file_path);

    // Get parent directory and file stem
    let parent = path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy());
    let grandparent = path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy());

    match (grandparent, parent) {
        (Some(gp), Some(p)) => format!("{}.{}", gp, p),
        (None, Some(p)) => p.to_string(),
        _ => path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// Format duplicate clusters with pagination info - TOKEN OPTIMIZED
/// Groups duplicates by module to reduce output size by ~35x for large codebases
pub(super) fn format_duplicate_clusters_paginated(
    clusters: &[DuplicateCluster],
    threshold: f64,
    total_clusters: usize,
    offset: usize,
    limit: usize,
    sort_by: &str,
) -> String {
    let mut output = String::new();

    // Compact header
    output.push_str("_type: duplicate_results\n");
    output.push_str(&format!(
        "threshold: {:.0}% | clusters: {} | showing: {}-{} | sort: {}\n",
        threshold * 100.0,
        total_clusters,
        offset + 1,
        offset + clusters.len(),
        sort_by
    ));

    // Pagination hint
    if offset + clusters.len() < total_clusters {
        output.push_str(&format!(
            "next: offset={} for more\n",
            offset + limit
        ));
    }

    if clusters.is_empty() {
        if total_clusters == 0 {
            output.push_str("result: No duplicate clusters found above threshold.\n");
        } else {
            output.push_str(&format!(
                "result: No clusters at offset {}. Total: {}.\n",
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
        // Use actual module name from index, fallback to path extraction for old cached data
        let primary_module = if !cluster.primary.module.is_empty() {
            cluster.primary.module.clone()
        } else {
            extract_module_name(&cluster.primary.file)
        };
        let line_info = if cluster.primary.start_line > 0 {
            format!(":{}", cluster.primary.start_line)
        } else {
            String::new()
        };

        // Compact cluster header: [N] FunctionName (Module:line) → X dups
        output.push_str(&format!(
            "[{}] {} ({}{})\n",
            offset + i + 1,
            cluster.primary.name,
            primary_module,
            line_info
        ));
        output.push_str(&format!("  hash: {}\n", cluster.primary.hash));
        output.push_str(&format!("  duplicates: {}\n", cluster.duplicates.len()));

        // Group duplicates by module (actual index module names)
        let mut by_module: BTreeMap<String, Vec<&DuplicateMatch>> = BTreeMap::new();
        for dup in &cluster.duplicates {
            let module = if !dup.symbol.module.is_empty() {
                dup.symbol.module.clone()
            } else {
                extract_module_name(&dup.symbol.file)
            };
            by_module.entry(module).or_default().push(dup);
        }

        // Show modules with counts and similarity ranges (limit to top 5)
        output.push_str("  by_module:\n");
        let mut module_entries: Vec<_> = by_module.iter().collect();
        module_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len())); // Sort by count desc

        let max_modules_shown = 5;
        let mut remaining_count = 0;
        let mut remaining_modules = 0;

        for (idx, (module, dups)) in module_entries.iter().enumerate() {
            if idx >= max_modules_shown {
                remaining_count += dups.len();
                remaining_modules += 1;
                continue;
            }

            // Calculate similarity range for this module
            let min_sim = dups.iter().map(|d| d.similarity).fold(f64::INFINITY, f64::min);
            let max_sim = dups.iter().map(|d| d.similarity).fold(0.0_f64, f64::max);

            // Count by kind
            let exact = dups.iter().filter(|d| matches!(d.kind, DuplicateKind::Exact)).count();
            let near = dups.iter().filter(|d| matches!(d.kind, DuplicateKind::Near)).count();

            let kind_hint = if exact > 0 && near == 0 {
                "exact"
            } else if exact == 0 && near > 0 {
                "near"
            } else if exact > 0 {
                "mixed"
            } else {
                "divergent"
            };

            output.push_str(&format!(
                "    {}: {} ({:.0}-{:.0}%) [{}]\n",
                module,
                dups.len(),
                min_sim * 100.0,
                max_sim * 100.0,
                kind_hint
            ));
        }

        if remaining_modules > 0 {
            output.push_str(&format!(
                "    +{} more modules: {} dups\n",
                remaining_modules,
                remaining_count
            ));
        }

        // Show top 3 individual matches for actionable detail
        let mut top_matches: Vec<_> = cluster.duplicates.iter().collect();
        top_matches.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

        if !top_matches.is_empty() {
            output.push_str("  top_matches:\n");
            for dup in top_matches.iter().take(3) {
                // Use actual module name, fallback to path extraction for old cached data
                let dup_module = if !dup.symbol.module.is_empty() {
                    dup.symbol.module.clone()
                } else {
                    extract_module_name(&dup.symbol.file)
                };
                output.push_str(&format!(
                    "    {}@{} {:.0}%\n",
                    dup.symbol.name,
                    dup_module,
                    dup.similarity * 100.0
                ));
            }
        }

        output.push_str("\n");
    }

    output
}

/// Format duplicate matches for a single symbol as compact TOON output
pub(super) fn format_duplicate_matches(
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
            DuplicateKind::Exact => "EXACT",
            DuplicateKind::Near => "NEAR",
            DuplicateKind::Divergent => "DIVERGENT",
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
// CVE Scan Formatting
// ============================================================================

/// Format CVE scan results as compact TOON output
pub(super) fn format_cve_scan_results(
    summary: &CVEScanSummary,
    matches: &[CVEMatch],
    threshold: f32,
) -> String {
    let mut output = String::new();
    output.push_str("_type: cve_scan\n");
    output.push_str(&format!("functions_scanned: {}\n", summary.functions_scanned));
    output.push_str(&format!("patterns_checked: {}\n", summary.patterns_checked));
    output.push_str(&format!("similarity_threshold: {:.0}%\n", threshold * 100.0));
    output.push_str(&format!("total_matches: {}\n", summary.total_matches));
    output.push_str(&format!("scan_time_ms: {}\n", summary.scan_time_ms));

    // Severity breakdown
    if !summary.by_severity.is_empty() {
        output.push_str("by_severity:\n");
        for sev in [Severity::Critical, Severity::High, Severity::Medium, Severity::Low, Severity::None] {
            if let Some(&count) = summary.by_severity.get(&sev) {
                if count > 0 {
                    output.push_str(&format!("  {}: {}\n", sev, count));
                }
            }
        }
    }

    if matches.is_empty() {
        output.push_str("\nNo CVE pattern matches found.\n");
        return output;
    }

    output.push_str("\nmatches:\n");
    for (i, m) in matches.iter().enumerate() {
        output.push_str(&format!("\n  [{}] {} ({:.0}% match)\n", i + 1, m.cve_id, m.similarity * 100.0));
        output.push_str(&format!("      severity: {}\n", m.severity));
        if !m.cwe_ids.is_empty() {
            output.push_str(&format!("      cwe: {}\n", m.cwe_ids.join(", ")));
        }
        output.push_str(&format!("      file: {}:{}\n", m.file, m.line));
        output.push_str(&format!("      function: {}\n", m.function));

        // Truncate description if too long
        let desc = if m.description.len() > 200 {
            format!("{}...", &m.description[..197])
        } else {
            m.description.clone()
        };
        output.push_str(&format!("      description: {}\n", desc));

        if let Some(ref remediation) = m.remediation {
            let rem = if remediation.len() > 150 {
                format!("{}...", &remediation[..147])
            } else {
                remediation.clone()
            };
            output.push_str(&format!("      remediation: {}\n", rem));
        }
    }

    output
}

// ============================================================================
// Commit Preparation Formatting
// ============================================================================

/// Statistics for a file's diff
pub struct FileDiffStats {
    pub insertions: usize,
    pub deletions: usize,
}

/// Complexity metrics for a symbol
pub struct SymbolMetrics {
    pub name: String,
    pub kind: String,
    pub lines: String,
    pub cognitive: Option<usize>,
    pub cyclomatic: Option<usize>,
    pub max_nesting: Option<usize>,
    pub fan_out: Option<usize>,
    pub loc: Option<usize>,
    pub state_mutations: Option<usize>,
    pub io_operations: Option<usize>,
}

/// Analyzed file for prep-commit output
pub struct AnalyzedFile {
    pub path: String,
    pub change_type: String,
    pub diff_stats: Option<FileDiffStats>,
    pub symbols: Vec<SymbolMetrics>,
    pub error: Option<String>,
}

/// Context for git state
pub struct GitContext {
    pub branch: String,
    pub remote: Option<String>,
    pub last_commit_hash: Option<String>,
    pub last_commit_message: Option<String>,
}

/// Format prep-commit output as compact TOON
pub(super) fn format_prep_commit(
    git_context: &GitContext,
    staged_files: &[AnalyzedFile],
    unstaged_files: &[AnalyzedFile],
    include_complexity: bool,
    include_all_metrics: bool,
    show_diff_stats: bool,
) -> String {
    let mut output = String::new();
    output.push_str("_type: prep_commit\n");
    output.push_str("_note: Information for commit message. This tool DOES NOT commit.\n\n");

    // Git context section
    output.push_str("git_context:\n");
    output.push_str(&format!("  branch: \"{}\"\n", git_context.branch));
    if let Some(ref remote) = git_context.remote {
        output.push_str(&format!("  remote: \"{}\"\n", remote));
    }
    if let Some(ref hash) = git_context.last_commit_hash {
        output.push_str(&format!("  last_commit: \"{}\"\n", hash));
    }
    if let Some(ref msg) = git_context.last_commit_message {
        // Truncate message
        let truncated = if msg.len() > 60 {
            format!("{}...", truncate_to_char_boundary(msg, 57))
        } else {
            msg.clone()
        };
        output.push_str(&format!("  last_message: \"{}\"\n", truncated));
    }
    output.push('\n');

    // Summary counts
    let staged_symbol_count: usize = staged_files.iter().map(|f| f.symbols.len()).sum();
    let unstaged_symbol_count: usize = unstaged_files.iter().map(|f| f.symbols.len()).sum();

    output.push_str("summary:\n");
    output.push_str(&format!("  staged_files: {}\n", staged_files.len()));
    output.push_str(&format!("  staged_symbols: {}\n", staged_symbol_count));
    output.push_str(&format!("  unstaged_files: {}\n", unstaged_files.len()));
    output.push_str(&format!("  unstaged_symbols: {}\n", unstaged_symbol_count));

    // Total diff stats if enabled
    if show_diff_stats {
        let staged_insertions: usize = staged_files.iter()
            .filter_map(|f| f.diff_stats.as_ref())
            .map(|s| s.insertions)
            .sum();
        let staged_deletions: usize = staged_files.iter()
            .filter_map(|f| f.diff_stats.as_ref())
            .map(|s| s.deletions)
            .sum();
        output.push_str(&format!("  staged_changes: +{} -{}\n", staged_insertions, staged_deletions));
    }
    output.push('\n');

    // Staged changes section
    if !staged_files.is_empty() {
        output.push_str(&format!("staged_changes[{}]:\n", staged_files.len()));
        format_file_list(&mut output, staged_files, include_complexity, include_all_metrics, show_diff_stats);
        output.push('\n');
    } else {
        output.push_str("staged_changes: (none)\n\n");
    }

    // Unstaged changes section
    if !unstaged_files.is_empty() {
        output.push_str(&format!("unstaged_changes[{}]:\n", unstaged_files.len()));
        format_file_list(&mut output, unstaged_files, include_complexity, include_all_metrics, show_diff_stats);
    } else {
        output.push_str("unstaged_changes: (none)\n");
    }

    output
}

/// Helper to format a list of analyzed files
fn format_file_list(
    output: &mut String,
    files: &[AnalyzedFile],
    include_complexity: bool,
    include_all_metrics: bool,
    show_diff_stats: bool,
) {
    for file in files {
        // File header with change type and optional diff stats
        let mut header = format!("  {} [{}]", file.path, file.change_type);
        if show_diff_stats {
            if let Some(ref stats) = file.diff_stats {
                header.push_str(&format!(" (+{} -{})", stats.insertions, stats.deletions));
            }
        }
        output.push_str(&header);
        output.push('\n');

        // Handle errors
        if let Some(ref err) = file.error {
            output.push_str(&format!("    ({})\n", err));
            continue;
        }

        // Symbols
        if file.symbols.is_empty() {
            output.push_str("    symbols: (none detected)\n");
            continue;
        }

        output.push_str(&format!("    symbols[{}]:\n", file.symbols.len()));
        for sym in &file.symbols {
            // Basic info: name (kind) lines
            output.push_str(&format!("      - {} ({}) L{}\n", sym.name, sym.kind, sym.lines));

            // Complexity metrics if enabled
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

            // All metrics if requested
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ========================================================================
    // get_supported_languages Tests
    // ========================================================================

    #[test]
    fn test_get_supported_languages_includes_common_languages() {
        let output = get_supported_languages();
        assert!(output.contains("TypeScript"));
        assert!(output.contains("Rust"));
        assert!(output.contains("Python"));
        assert!(output.contains("JavaScript"));
        assert!(output.contains("Go"));
    }

    #[test]
    fn test_get_supported_languages_includes_extensions() {
        let output = get_supported_languages();
        assert!(output.contains(".ts"));
        assert!(output.contains(".rs"));
        assert!(output.contains(".py"));
        assert!(output.contains(".go"));
    }

    // ========================================================================
    // format_source_snippet Tests
    // ========================================================================

    #[test]
    fn test_format_source_snippet_basic() {
        let source = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let output = format_source_snippet(Path::new("test.ts"), source, 2, 3, 0);

        assert!(output.contains("test.ts"));
        assert!(output.contains("lines 2-3"));
        assert!(output.contains("line 2"));
        assert!(output.contains("line 3"));
    }

    #[test]
    fn test_format_source_snippet_with_context() {
        let source = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let output = format_source_snippet(Path::new("test.ts"), source, 3, 3, 1);

        // With context of 1, should show lines 2-4
        assert!(output.contains("line 2"));
        assert!(output.contains("line 3"));
        assert!(output.contains("line 4"));
    }

    #[test]
    fn test_format_source_snippet_markers() {
        let source = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let output = format_source_snippet(Path::new("test.ts"), source, 2, 3, 1);

        // Target lines should have > marker
        assert!(output.contains(">"));
    }

    // ========================================================================
    // format_search_results Tests
    // ========================================================================

    fn make_symbol_entry(name: &str, hash: &str, module: &str) -> SymbolIndexEntry {
        SymbolIndexEntry {
            symbol: name.to_string(),
            hash: hash.to_string(),
            semantic_hash: "".to_string(),
            kind: "fn".to_string(),
            module: module.to_string(),
            file: format!("src/{}.ts", module),
            lines: "1-10".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: 5,
            max_nesting: 2,
        }
    }

    #[test]
    fn test_format_search_results_empty() {
        let output = format_search_results("test", &[]);

        assert!(output.contains("_type: search_results"));
        assert!(output.contains("query: \"test\""));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("results: (none)"));
    }

    #[test]
    fn test_format_search_results_with_results() {
        let results = vec![
            make_symbol_entry("foo", "hash1", "utils"),
            make_symbol_entry("bar", "hash2", "api"),
        ];
        let output = format_search_results("test", &results);

        assert!(output.contains("showing: 2"));
        assert!(output.contains("foo"));
        assert!(output.contains("bar"));
        assert!(output.contains("hash1"));
        assert!(output.contains("utils"));
    }

    // ========================================================================
    // format_ripgrep_results Tests
    // ========================================================================

    #[test]
    fn test_format_ripgrep_results_empty() {
        let output = format_ripgrep_results("pattern", &[]);

        assert!(output.contains("_type: ripgrep_results"));
        assert!(output.contains("ripgrep fallback"));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("results: (none)"));
    }

    #[test]
    fn test_format_ripgrep_results_with_matches() {
        let results = vec![RipgrepSearchResult {
            file: "src/main.ts".to_string(),
            line: 10,
            column: 5,
            content: "const pattern = 'test'".to_string(),
        }];
        let output = format_ripgrep_results("pattern", &results);

        assert!(output.contains("showing: 1"));
        assert!(output.contains("src/main.ts"));
        assert!(output.contains("10"));
        assert!(output.contains("pattern"));
    }

    #[test]
    fn test_format_ripgrep_results_truncates_long_content() {
        let long_content = "x".repeat(150);
        let results = vec![RipgrepSearchResult {
            file: "src/main.ts".to_string(),
            line: 1,
            column: 1,
            content: long_content,
        }];
        let output = format_ripgrep_results("x", &results);

        // Should be truncated to 100 chars + "..."
        assert!(output.contains("..."));
    }

    // ========================================================================
    // extract_module_name Tests
    // ========================================================================

    #[test]
    fn test_extract_module_name_basic() {
        let result = extract_module_name("/home/user/project/src/utils/helper.ts");
        // Should get grandparent.parent -> "utils" or "src.utils"
        assert!(result.contains("utils") || result.contains("src"));
    }

    #[test]
    fn test_extract_module_name_dotnet_style() {
        let result = extract_module_name("/project/src/Nop.Web/Factories/ProductFactory.cs");
        assert!(result.contains("Nop.Web") || result.contains("Factories"));
    }

    #[test]
    fn test_extract_module_name_shallow_path() {
        let result = extract_module_name("main.ts");
        assert!(!result.is_empty());
    }

    // ========================================================================
    // format_call_graph_summary Tests
    // ========================================================================

    #[test]
    fn test_format_call_graph_summary_empty() {
        let edges: Vec<(String, Vec<String>)> = vec![];
        let output = format_call_graph_summary(&edges, 0, 0, None);

        assert!(output.contains("_type: call_graph_summary"));
        assert!(output.contains("total_edges: 0"));
        assert!(output.contains("avg_calls_per_symbol: 0.0"));
    }

    #[test]
    fn test_format_call_graph_summary_with_edges() {
        let edges = vec![
            ("caller1".to_string(), vec!["callee1".to_string(), "callee2".to_string()]),
            ("caller2".to_string(), vec!["callee1".to_string()]),
        ];
        let output = format_call_graph_summary(&edges, 3, 3, None);

        assert!(output.contains("total_calls: 3"));
        assert!(output.contains("caller1"));
        assert!(output.contains("callee1"));
    }

    #[test]
    fn test_format_call_graph_summary_with_name_mapping() {
        let edges = vec![
            ("hash1".to_string(), vec!["hash2".to_string()]),
        ];
        let mut names = HashMap::new();
        names.insert("hash1".to_string(), "myFunction".to_string());
        names.insert("hash2".to_string(), "otherFunction".to_string());

        let output = format_call_graph_summary(&edges, 1, 1, Some(&names));

        assert!(output.contains("myFunction"));
        assert!(output.contains("otherFunction"));
    }

    // ========================================================================
    // format_call_graph_paginated Tests
    // ========================================================================

    #[test]
    fn test_format_call_graph_paginated_empty() {
        let edges: Vec<(String, Vec<String>)> = vec![];
        let output = format_call_graph_paginated(&edges, 0, 0, 0, 10, None);

        assert!(output.contains("_type: call_graph"));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("No edges match"));
    }

    #[test]
    fn test_format_call_graph_paginated_with_pagination_hint() {
        let edges = vec![
            ("caller1".to_string(), vec!["callee1".to_string()]),
        ];
        // Total is more than shown
        let output = format_call_graph_paginated(&edges, 10, 5, 0, 2, None);

        assert!(output.contains("next_offset:"));
    }

    // ========================================================================
    // format_duplicate_matches Tests
    // ========================================================================

    #[test]
    fn test_format_duplicate_matches_empty() {
        let output = format_duplicate_matches("myFunc", "src/main.ts", &[], 0.9);

        assert!(output.contains("_type: duplicate_check"));
        assert!(output.contains("symbol: myFunc"));
        assert!(output.contains("matches: 0"));
        assert!(output.contains("No duplicates found"));
    }

    #[test]
    fn test_format_duplicate_matches_with_matches() {
        use crate::duplicate::{Difference, DuplicateKind, DuplicateMatch, SymbolRef};

        let symbol_ref = SymbolRef {
            hash: "hash123".to_string(),
            name: "similarFunc".to_string(),
            file: "src/utils.ts".to_string(),
            module: "utils".to_string(),
            start_line: 10,
            end_line: 20,
        };
        let matches = vec![DuplicateMatch {
            symbol: symbol_ref,
            similarity: 0.95,
            kind: DuplicateKind::Near,
            differences: vec![Difference::DifferentParamCount {
                expected: 2,
                actual: 3,
            }],
        }];

        let output = format_duplicate_matches("myFunc", "src/main.ts", &matches, 0.9);

        assert!(output.contains("matches: 1"));
        assert!(output.contains("similarFunc"));
        assert!(output.contains("95%"));
        assert!(output.contains("NEAR"));
    }

    // ========================================================================
    // format_cve_scan_results Tests
    // ========================================================================

    #[test]
    fn test_format_cve_scan_results_no_matches() {
        let summary = CVEScanSummary {
            functions_scanned: 100,
            patterns_checked: 50,
            total_matches: 0,
            scan_time_ms: 123,
            by_severity: HashMap::new(),
        };
        let output = format_cve_scan_results(&summary, &[], 0.75);

        assert!(output.contains("_type: cve_scan"));
        assert!(output.contains("functions_scanned: 100"));
        assert!(output.contains("No CVE pattern matches"));
    }

    #[test]
    fn test_format_cve_scan_results_with_matches() {
        let mut by_severity = HashMap::new();
        by_severity.insert(Severity::High, 1);

        let summary = CVEScanSummary {
            functions_scanned: 100,
            patterns_checked: 50,
            total_matches: 1,
            scan_time_ms: 123,
            by_severity,
        };
        let matches = vec![CVEMatch {
            cve_id: "CVE-2021-12345".to_string(),
            severity: Severity::High,
            cwe_ids: vec!["CWE-89".to_string()],
            similarity: 0.85,
            file: "src/db.ts".to_string(),
            line: 42,
            function: "queryDatabase".to_string(),
            description: "SQL injection vulnerability".to_string(),
            remediation: Some("Use parameterized queries".to_string()),
        }];

        let output = format_cve_scan_results(&summary, &matches, 0.75);

        assert!(output.contains("CVE-2021-12345"));
        assert!(output.contains("85%"));
        assert!(output.contains("CWE-89"));
        assert!(output.contains("queryDatabase"));
    }

    // ========================================================================
    // format_prep_commit Tests
    // ========================================================================

    #[test]
    fn test_format_prep_commit_no_changes() {
        let git_context = GitContext {
            branch: "main".to_string(),
            remote: Some("origin".to_string()),
            last_commit_hash: Some("abc123".to_string()),
            last_commit_message: Some("Initial commit".to_string()),
        };

        let output = format_prep_commit(&git_context, &[], &[], false, false, false);

        assert!(output.contains("_type: prep_commit"));
        assert!(output.contains("branch: \"main\""));
        assert!(output.contains("staged_files: 0"));
        assert!(output.contains("unstaged_files: 0"));
    }

    #[test]
    fn test_format_prep_commit_with_staged_changes() {
        let git_context = GitContext {
            branch: "feature".to_string(),
            remote: None,
            last_commit_hash: None,
            last_commit_message: None,
        };

        let staged = vec![AnalyzedFile {
            path: "src/main.ts".to_string(),
            change_type: "modified".to_string(),
            diff_stats: Some(FileDiffStats { insertions: 10, deletions: 5 }),
            symbols: vec![SymbolMetrics {
                name: "myFunction".to_string(),
                kind: "fn".to_string(),
                lines: "1-10".to_string(),
                cognitive: Some(8),
                cyclomatic: None,
                max_nesting: Some(3),
                fan_out: None,
                loc: None,
                state_mutations: None,
                io_operations: None,
            }],
            error: None,
        }];

        let output = format_prep_commit(&git_context, &staged, &[], true, false, true);

        assert!(output.contains("staged_files: 1"));
        assert!(output.contains("src/main.ts"));
        assert!(output.contains("myFunction"));
        assert!(output.contains("+10 -5"));
        assert!(output.contains("cognitive=8"));
    }

    // ========================================================================
    // FileDiffStats / SymbolMetrics / AnalyzedFile / GitContext Struct Tests
    // ========================================================================

    #[test]
    fn test_file_diff_stats_struct() {
        let stats = FileDiffStats {
            insertions: 100,
            deletions: 50,
        };
        assert_eq!(stats.insertions, 100);
        assert_eq!(stats.deletions, 50);
    }

    #[test]
    fn test_symbol_metrics_struct() {
        let metrics = SymbolMetrics {
            name: "testFn".to_string(),
            kind: "fn".to_string(),
            lines: "1-10".to_string(),
            cognitive: Some(5),
            cyclomatic: Some(3),
            max_nesting: Some(2),
            fan_out: Some(4),
            loc: Some(10),
            state_mutations: Some(1),
            io_operations: Some(0),
        };
        assert_eq!(metrics.name, "testFn");
        assert_eq!(metrics.cognitive, Some(5));
    }

    #[test]
    fn test_analyzed_file_struct() {
        let file = AnalyzedFile {
            path: "src/test.ts".to_string(),
            change_type: "added".to_string(),
            diff_stats: None,
            symbols: vec![],
            error: None,
        };
        assert_eq!(file.path, "src/test.ts");
        assert_eq!(file.change_type, "added");
    }

    #[test]
    fn test_git_context_struct() {
        let ctx = GitContext {
            branch: "develop".to_string(),
            remote: Some("upstream".to_string()),
            last_commit_hash: Some("def456".to_string()),
            last_commit_message: Some("Fix bug".to_string()),
        };
        assert_eq!(ctx.branch, "develop");
        assert_eq!(ctx.remote, Some("upstream".to_string()));
    }

    // ========================================================================
    // format_diff_output_paginated Tests
    // ========================================================================

    fn make_changed_file(path: &str, change_type: crate::git::ChangeType) -> crate::git::ChangedFile {
        crate::git::ChangedFile {
            path: path.to_string(),
            old_path: None,
            change_type,
        }
    }

    #[test]
    fn test_format_diff_output_paginated_empty() {
        let temp = tempfile::tempdir().unwrap();
        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &[],
            0,
            20,
        );

        assert!(output.contains("_type: analyze_diff"));
        assert!(output.contains("base: \"main\""));
        assert!(output.contains("target: \"HEAD\""));
        assert!(output.contains("total_files: 0"));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("No files changed"));
    }

    #[test]
    fn test_format_diff_output_paginated_single_file() {
        let temp = tempfile::tempdir().unwrap();
        // Create a simple TypeScript file to parse
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/main.ts"),
            "export function hello(): string { return 'hi'; }",
        ).unwrap();

        let files = vec![
            make_changed_file("src/main.ts", crate::git::ChangeType::Modified),
        ];

        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &files,
            0,
            20,
        );

        assert!(output.contains("_type: analyze_diff"));
        assert!(output.contains("total_files: 1"));
        assert!(output.contains("showing: 1"));
        assert!(output.contains("src/main.ts [modified]"));
        // Should NOT have next_offset since all files fit
        assert!(!output.contains("next_offset:"));
    }

    #[test]
    fn test_format_diff_output_paginated_with_pagination() {
        let temp = tempfile::tempdir().unwrap();
        // Create multiple files
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        for i in 0..5 {
            std::fs::write(
                temp.path().join(format!("src/file{}.ts", i)),
                format!("export function f{}(): number {{ return {}; }}", i, i),
            ).unwrap();
        }

        let files: Vec<_> = (0..5)
            .map(|i| make_changed_file(&format!("src/file{}.ts", i), crate::git::ChangeType::Added))
            .collect();

        // Request only 2 files at offset 0
        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &files,
            0,
            2,
        );

        assert!(output.contains("total_files: 5"));
        assert!(output.contains("showing: 2"));
        assert!(output.contains("offset: 0"));
        assert!(output.contains("limit: 2"));
        assert!(output.contains("next_offset: 2"));
        // Should contain first two files
        assert!(output.contains("file0.ts"));
        assert!(output.contains("file1.ts"));
        // Should NOT contain later files
        assert!(!output.contains("file4.ts"));
    }

    #[test]
    fn test_format_diff_output_paginated_offset_at_end() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/main.ts"),
            "export function f(): void {}",
        ).unwrap();

        let files = vec![
            make_changed_file("src/main.ts", crate::git::ChangeType::Modified),
        ];

        // Offset beyond available files
        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &files,
            10,
            20,
        );

        assert!(output.contains("total_files: 1"));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("No files at offset 10"));
    }

    #[test]
    fn test_format_diff_output_paginated_deleted_file() {
        let temp = tempfile::tempdir().unwrap();

        let files = vec![
            make_changed_file("src/deleted.ts", crate::git::ChangeType::Deleted),
        ];

        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &files,
            0,
            20,
        );

        assert!(output.contains("src/deleted.ts [deleted]"));
        assert!(output.contains("(deleted)"));
    }

    #[test]
    fn test_format_diff_output_paginated_change_type_summary() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/a.ts"), "// a").unwrap();
        std::fs::write(temp.path().join("src/b.ts"), "// b").unwrap();

        let files = vec![
            make_changed_file("src/a.ts", crate::git::ChangeType::Added),
            make_changed_file("src/b.ts", crate::git::ChangeType::Modified),
            make_changed_file("src/c.ts", crate::git::ChangeType::Deleted),
        ];

        let output = format_diff_output_paginated(
            temp.path(),
            "main",
            "HEAD",
            &files,
            0,
            20,
        );

        // Should have change type counts in summary
        assert!(output.contains("changes:"));
        assert!(output.contains("added=1"));
        assert!(output.contains("modified=1"));
        assert!(output.contains("deleted=1"));
    }

    // ========================================================================
    // format_diff_summary Tests
    // ========================================================================

    #[test]
    fn test_format_diff_summary_empty() {
        let temp = tempfile::tempdir().unwrap();
        let output = format_diff_summary(
            temp.path(),
            "main",
            "HEAD",
            &[],
        );

        assert!(output.contains("_type: analyze_diff_summary"));
        assert!(output.contains("total_files: 0"));
        assert!(output.contains("No files changed"));
    }

    #[test]
    fn test_format_diff_summary_multiple_types() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/app.ts"), "// ts").unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "// rs").unwrap();

        let files = vec![
            make_changed_file("src/app.ts", crate::git::ChangeType::Added),
            make_changed_file("src/app.ts", crate::git::ChangeType::Added),
            make_changed_file("src/lib.rs", crate::git::ChangeType::Modified),
        ];

        let output = format_diff_summary(
            temp.path(),
            "main",
            "HEAD",
            &files,
        );

        assert!(output.contains("_type: analyze_diff_summary"));
        assert!(output.contains("total_files: 3"));
        assert!(output.contains("by_change_type:"));
        assert!(output.contains("by_language:"));
        assert!(output.contains("top_modules:"));
    }

    #[test]
    fn test_format_diff_summary_risk_assessment() {
        let temp = tempfile::tempdir().unwrap();

        let files = vec![
            make_changed_file("src/auth/login.ts", crate::git::ChangeType::Modified),
            make_changed_file("src/api/handler.ts", crate::git::ChangeType::Modified),
            make_changed_file("src/utils/format.ts", crate::git::ChangeType::Modified),
        ];

        let output = format_diff_summary(
            temp.path(),
            "main",
            "HEAD",
            &files,
        );

        assert!(output.contains("risk_estimate:"));
        // auth file should be high risk
        assert!(output.contains("high=1"));
        // api file should be medium risk
        assert!(output.contains("medium=1"));
        // utils file should be low risk
        assert!(output.contains("low=1"));
    }

    #[test]
    fn test_format_diff_summary_hint() {
        let temp = tempfile::tempdir().unwrap();
        let files = vec![
            make_changed_file("file.ts", crate::git::ChangeType::Added),
        ];

        let output = format_diff_summary(
            temp.path(),
            "main",
            "HEAD",
            &files,
        );

        assert!(output.contains("_hint:"));
        assert!(output.contains("limit/offset"));
    }
}
