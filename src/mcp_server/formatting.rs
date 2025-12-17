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
use super::GetSymbolSourceRequest;

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
pub(super) fn format_call_graph_paginated(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
    offset: usize,
    limit: usize,
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
pub(super) fn format_call_graph_summary(
    edges: &[(String, Vec<String>)],
    total_edges: usize,
    filtered_count: usize,
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

/// Format duplicate clusters with pagination info
pub(super) fn format_duplicate_clusters_paginated(
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
                DuplicateKind::Exact => "exact",
                DuplicateKind::Near => "near",
                DuplicateKind::Divergent => "divergent",
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
