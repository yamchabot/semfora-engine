//! Search command handler - Hybrid search that runs both symbol and semantic search
//!
//! This module implements the "magic" search that runs BOTH symbol matching AND
//! semantic search by default, presenting results in categorized sections.

use crate::cache::CacheDir;
use crate::cli::{OutputFormat, SearchArgs, SearchMode};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::ripgrep::{RipgrepSearcher, SearchOptions};
use crate::truncate_to_char_boundary;
use std::collections::HashSet;

/// Run the search command with hybrid search by default
pub fn run_search(args: &SearchArgs, ctx: &CommandContext) -> Result<String> {
    let mode = args.search_mode();

    match mode {
        SearchMode::Hybrid => run_hybrid_search(args, ctx),
        SearchMode::SymbolsOnly => run_symbol_search(args, ctx),
        SearchMode::SemanticOnly => run_semantic_search(args, ctx),
        SearchMode::Raw => run_raw_search(args, ctx),
    }
}

/// Hybrid search: runs both symbol and semantic search, presents combined results
fn run_hybrid_search(args: &SearchArgs, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    let mut output = String::new();

    // Try to get symbol matches
    let symbol_results = get_symbol_matches(&cache, args);

    // Try to get semantic matches
    let semantic_results = get_semantic_matches(&cache, args);

    let symbol_count = symbol_results
        .as_ref()
        .map(|r| r.results.len())
        .unwrap_or(0);
    let related_count = semantic_results
        .as_ref()
        .map(|r| r.results.len())
        .unwrap_or(0);

    // Generate contextual hint for AI based on what was found
    let hint: Option<&str> = match (symbol_count > 0, related_count > 0) {
        (true, true) => None, // Both found results - no hint needed
        (true, false) => Some("Exact symbol matches found. No semantic matches - try get_symbol(hash) for details."),
        (false, true) => Some("No exact symbol name matches, but BM25 found related code. Use hashes from related_code for get_symbol/get_source."),
        (false, false) => Some("No results. Try: 1) get_overview to find module names, 2) get_file(module) to explore, 3) simpler single-word queries."),
    };

    // Build JSON with dynamic ordering - put results first, zeros last
    let empty_symbols: Vec<SymbolEntry> = vec![];
    let empty_semantic: Vec<SemanticEntry> = vec![];
    let empty_suggestions: Vec<String> = vec![];
    let symbol_matches = symbol_results
        .as_ref()
        .map(|r| &r.results)
        .unwrap_or(&empty_symbols);
    let related_code = semantic_results
        .as_ref()
        .map(|r| &r.results)
        .unwrap_or(&empty_semantic);
    let suggestions = semantic_results
        .as_ref()
        .map(|r| &r.suggestions)
        .unwrap_or(&empty_suggestions);

    // Dynamic field ordering: show non-empty results first
    let json_value = if symbol_count > 0 || related_count == 0 {
        // Symbol results first (has results, or both empty)
        let mut obj = serde_json::json!({
            "_type": "hybrid_search",
            "query": args.query,
            "symbol_matches": symbol_matches,
            "symbol_count": symbol_count,
            "related_code": related_code,
            "related_count": related_count,
            "suggested_queries": suggestions,
        });
        if let Some(h) = hint {
            obj["hint"] = serde_json::json!(h);
        }
        obj
    } else {
        // Related code first (has results while symbols empty)
        let mut obj = serde_json::json!({
            "_type": "hybrid_search",
            "query": args.query,
            "related_code": related_code,
            "related_count": related_count,
            "symbol_matches": symbol_matches,
            "symbol_count": symbol_count,
            "suggested_queries": suggestions,
        });
        if let Some(h) = hint {
            obj["hint"] = serde_json::json!(h);
        }
        obj
    };

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("query: \"{}\"\n\n", args.query));

            // Symbol matches section
            output.push_str("═══════════════════════════════════════════\n");
            if let Some(ref results) = symbol_results {
                output.push_str(&format!("SYMBOL MATCHES ({})\n", results.results.len()));
                output.push_str("───────────────────────────────────────────\n");
                if results.results.is_empty() {
                    output.push_str("(no exact symbol matches)\n");
                } else {
                    for entry in &results.results {
                        output.push_str(&format!(
                            "• {} ({})      {}:{}\n",
                            entry.symbol, entry.kind, entry.file, entry.lines
                        ));
                        if ctx.verbose {
                            output.push_str(&format!("  hash: {}\n", entry.hash));
                            output.push_str(&format!(
                                "  module: {} | risk: {}\n",
                                entry.module, entry.risk
                            ));
                        }
                    }
                }
            } else {
                output.push_str("SYMBOL MATCHES\n");
                output.push_str("───────────────────────────────────────────\n");
                output.push_str("(no index - run `semfora index generate` first)\n");
            }

            output.push_str("\n═══════════════════════════════════════════\n");

            // Semantic/related code section
            if let Some(ref results) = semantic_results {
                output.push_str(&format!("RELATED CODE ({})\n", results.results.len()));
                output.push_str("───────────────────────────────────────────\n");
                if results.results.is_empty() {
                    output.push_str("(no semantically related code found)\n");
                } else {
                    for entry in &results.results {
                        let desc = if entry.matched_terms.is_empty() {
                            String::new()
                        } else {
                            format!(" - matches: {}", entry.matched_terms.join(", "))
                        };
                        output.push_str(&format!(
                            "• {}{}      [score: {:.2}]\n",
                            entry.symbol, desc, entry.score
                        ));
                        output.push_str(&format!(
                            "  {}:{} ({})\n",
                            entry.file, entry.lines, entry.kind
                        ));
                    }
                }

                // Suggestions
                if !results.suggestions.is_empty() {
                    output.push_str("\n───────────────────────────────────────────\n");
                    output.push_str("suggested_queries:\n");
                    for suggestion in &results.suggestions {
                        output.push_str(&format!("  • \"{}\"\n", suggestion));
                    }
                }
            } else {
                output.push_str("RELATED CODE\n");
                output.push_str("───────────────────────────────────────────\n");
                output.push_str("(no BM25 index - run `semfora index generate` first)\n");
            }

            // Include source snippets if requested
            if args.include_source {
                output.push_str("\n═══════════════════════════════════════════\n");
                output.push_str("SOURCE SNIPPETS\n");
                output.push_str("───────────────────────────────────────────\n");

                // Show source for top symbol matches
                if let Some(ref results) = symbol_results {
                    for entry in results.results.iter().take(3) {
                        if let Some(source) =
                            get_source_snippet(&cache, &entry.file, &entry.lines, 2)
                        {
                            output.push_str(&format!("\n## {} ({})\n", entry.symbol, entry.kind));
                            output.push_str(&format!("# {}:{}\n", entry.file, entry.lines));
                            output.push_str(&source);
                            output.push('\n');
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Symbol-only search (exact name matching)
fn run_symbol_search(args: &SearchArgs, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    // Use fallback-aware search
    let search_result = cache.search_symbols_with_fallback(
        &args.query,
        args.module.as_deref(),
        args.kind.as_deref(),
        args.risk.as_deref(),
        args.limit,
    )?;

    let mut output = String::new();

    if search_result.fallback_used {
        // Ripgrep fallback results
        let ripgrep_results = search_result.ripgrep_results.unwrap_or_default();

        let json_value = serde_json::json!({
            "_type": "symbol_search",
            "query": args.query,
            "results": ripgrep_results,
            "count": ripgrep_results.len(),
            "fallback": true,
            "note": "Using ripgrep fallback (no semantic index). Run `semfora index generate` to create index."
        });

        match ctx.format {
            OutputFormat::Json => {
                output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
            }
            OutputFormat::Toon => {
                output = super::encode_toon(&json_value);
            }
            OutputFormat::Text => {
                output.push_str("_note: Using ripgrep fallback (no semantic index)\n");
                output.push_str(&format!("query: \"{}\"\n", args.query));
                output.push_str(&format!("results[{}]:\n", ripgrep_results.len()));
                for entry in &ripgrep_results {
                    let content_preview = if entry.content.len() > 60 {
                        format!("{}...", truncate_to_char_boundary(&entry.content, 60))
                    } else {
                        entry.content.clone()
                    };
                    output.push_str(&format!(
                        "  {}:{}:{}: {}\n",
                        entry.file,
                        entry.line,
                        entry.column,
                        content_preview.trim()
                    ));
                }
            }
        }
    } else {
        // Normal indexed search results
        let mut results = search_result.indexed_results.unwrap_or_default();
        if !args.include_escape_refs {
            results.retain(|entry| !entry.is_escape_local);
        }

        let json_value = serde_json::json!({
            "_type": "symbol_search",
            "query": args.query,
            "results": results,
            "count": results.len()
        });

        match ctx.format {
            OutputFormat::Json => {
                output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
            }
            OutputFormat::Toon => {
                output = super::encode_toon(&json_value);
            }
            OutputFormat::Text => {
                output.push_str(&format!("query: \"{}\"\n", args.query));
                output.push_str(&format!("results[{}]:\n", results.len()));
                for entry in &results {
                    output.push_str(&format!(
                        "  {} ({}) - {} [{}] {}:{}\n",
                        entry.symbol, entry.kind, entry.module, entry.risk, entry.file, entry.lines
                    ));
                }
            }
        }
    }

    Ok(output)
}

/// Semantic-only search (BM25 natural language matching)
fn run_semantic_search(args: &SearchArgs, ctx: &CommandContext) -> Result<String> {
    use crate::bm25::Bm25Index;

    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.has_bm25_index() {
        return Err(McpDiffError::FileNotFound {
            path: "BM25 index not found. Run `semfora index generate` first.".to_string(),
        });
    }

    let bm25_path = cache.bm25_index_path();
    let index = Bm25Index::load(&bm25_path).map_err(|e| McpDiffError::GitError {
        message: format!("Failed to load BM25 index: {}", e),
    })?;

    let mut results = index.search(&args.query, args.limit * 2);

    // Apply filters
    if let Some(ref kind_filter) = args.kind {
        let kind_lower = kind_filter.to_lowercase();
        results.retain(|r| r.kind.to_lowercase() == kind_lower);
    }
    if let Some(ref module_filter) = args.module {
        let module_lower = module_filter.to_lowercase();
        results.retain(|r| r.module.to_lowercase() == module_lower);
    }
    if !args.include_escape_refs {
        let escape_hashes = load_escape_local_hashes(&cache);
        results.retain(|r| !escape_hashes.contains(&r.hash));
    }

    results.truncate(args.limit);

    let mut output = String::new();

    let suggestions = index.suggest_related_terms(&args.query, 5);

    let json_value = serde_json::json!({
        "_type": "semantic_search",
        "query": args.query,
        "results": results.iter().map(|r| serde_json::json!({
            "symbol": r.symbol,
            "kind": r.kind,
            "hash": r.hash,
            "file": r.file,
            "lines": r.lines,
            "module": r.module,
            "risk": r.risk,
            "score": r.score,
            "matched_terms": r.matched_terms
        })).collect::<Vec<_>>(),
        "count": results.len(),
        "related_terms": suggestions
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("query: \"{}\"\n", args.query));
            output.push_str(&format!("results[{}]:\n", results.len()));

            for result in &results {
                output.push_str(&format!("\n## {} ({})\n", result.symbol, result.kind));
                output.push_str(&format!("hash: {}\n", result.hash));
                output.push_str(&format!("file: {}\n", result.file));
                output.push_str(&format!("lines: {}\n", result.lines));
                output.push_str(&format!("module: {}\n", result.module));
                output.push_str(&format!("risk: {}\n", result.risk));
                output.push_str(&format!("score: {:.3}\n", result.score));
                output.push_str(&format!(
                    "matched_terms: {}\n",
                    result.matched_terms.join(", ")
                ));

                if args.include_source {
                    if let Some(source) = get_source_snippet(&cache, &result.file, &result.lines, 2)
                    {
                        output.push_str("__source__:\n");
                        output.push_str(&source);
                    }
                }
            }

            if !suggestions.is_empty() {
                output.push_str(&format!(
                    "\n---\nrelated_terms: {}\n",
                    suggestions.join(", ")
                ));
            }
        }
    }

    Ok(output)
}

/// Raw regex search using ripgrep
fn run_raw_search(args: &SearchArgs, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let mut options = SearchOptions::new(&args.query)
        .with_limit(args.limit)
        .with_merge_threshold(args.merge_threshold);

    if !args.case_sensitive {
        options = options.case_insensitive();
    }

    if let Some(ref types) = args.file_types {
        let file_types: Vec<String> = types.split(',').map(|s| s.trim().to_string()).collect();
        options = options.with_file_types(file_types);
    }

    let searcher = RipgrepSearcher::new();
    let mut output = String::new();

    if args.merge_threshold > 0 {
        match searcher.search_merged(&repo_dir, &options) {
            Ok(blocks) => {
                let json_value = serde_json::json!({
                    "_type": "raw_search",
                    "pattern": args.query,
                    "blocks": blocks.iter().map(|b| serde_json::json!({
                        "file": b.file.strip_prefix(&repo_dir).unwrap_or(&b.file).to_string_lossy(),
                        "start_line": b.start_line,
                        "end_line": b.end_line,
                        "lines": b.lines.iter().map(|l| serde_json::json!({
                            "line": l.line,
                            "content": l.content,
                            "is_match": l.is_match
                        })).collect::<Vec<_>>()
                    })).collect::<Vec<_>>(),
                    "count": blocks.len()
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str(&format!("pattern: \"{}\"\n", args.query));
                        output.push_str(&format!("blocks[{}]:\n", blocks.len()));
                        for block in &blocks {
                            let relative_file = block
                                .file
                                .strip_prefix(&repo_dir)
                                .unwrap_or(&block.file)
                                .to_string_lossy();
                            output.push_str(&format!(
                                "\n--- {}:{}-{} ---\n",
                                relative_file, block.start_line, block.end_line
                            ));
                            for line in &block.lines {
                                let prefix = if line.is_match { ">" } else { " " };
                                output.push_str(&format!(
                                    "{} {:>4} | {}\n",
                                    prefix, line.line, line.content
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                return Err(McpDiffError::GitError {
                    message: format!("Search failed: {}", e),
                })
            }
        }
    } else {
        match searcher.search(&repo_dir, &options) {
            Ok(matches) => {
                let json_value = serde_json::json!({
                    "_type": "raw_search",
                    "pattern": args.query,
                    "matches": matches.iter().map(|m| serde_json::json!({
                        "file": m.file.strip_prefix(&repo_dir).unwrap_or(&m.file).to_string_lossy(),
                        "line": m.line,
                        "column": m.column,
                        "content": m.content.trim()
                    })).collect::<Vec<_>>(),
                    "count": matches.len()
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str(&format!("pattern: \"{}\"\n", args.query));
                        output.push_str(&format!("matches[{}]:\n", matches.len()));
                        for m in &matches {
                            let relative_file = m
                                .file
                                .strip_prefix(&repo_dir)
                                .unwrap_or(&m.file)
                                .to_string_lossy();
                            output.push_str(&format!(
                                "  {}:{}:{}: {}\n",
                                relative_file,
                                m.line,
                                m.column,
                                m.content.trim()
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(McpDiffError::GitError {
                    message: format!("Search failed: {}", e),
                })
            }
        }
    }

    Ok(output)
}

// ============================================
// Helper Types and Functions
// ============================================

/// Symbol search result entry
#[derive(Debug, Clone, serde::Serialize)]
struct SymbolEntry {
    symbol: String,
    kind: String,
    hash: String,
    file: String,
    lines: String,
    module: String,
    risk: String,
}

/// Symbol search results
struct SymbolSearchResults {
    results: Vec<SymbolEntry>,
}

/// Semantic search result entry
#[derive(Debug, Clone, serde::Serialize)]
struct SemanticEntry {
    symbol: String,
    kind: String,
    hash: String,
    file: String,
    lines: String,
    module: String,
    risk: String,
    score: f32,
    matched_terms: Vec<String>,
}

/// Semantic search results with suggestions
struct SemanticSearchResults {
    results: Vec<SemanticEntry>,
    suggestions: Vec<String>,
}

/// Get symbol matches from the index
fn get_symbol_matches(cache: &CacheDir, args: &SearchArgs) -> Option<SymbolSearchResults> {
    let search_result = cache
        .search_symbols_with_fallback(
            &args.query,
            args.module.as_deref(),
            args.kind.as_deref(),
            args.risk.as_deref(),
            args.limit / 2, // Half limit for hybrid
        )
        .ok()?;

    if search_result.fallback_used {
        // Convert ripgrep results to symbol entries (limited info)
        let ripgrep = search_result.ripgrep_results.unwrap_or_default();
        let results: Vec<SymbolEntry> = ripgrep
            .iter()
            .take(args.limit / 2)
            .map(|r| SymbolEntry {
                symbol: extract_symbol_name(&r.content),
                kind: "unknown".to_string(),
                hash: String::new(),
                file: r.file.clone(),
                lines: r.line.to_string(),
                module: "unknown".to_string(),
                risk: "unknown".to_string(),
            })
            .collect();
        Some(SymbolSearchResults { results })
    } else {
        let mut indexed = search_result.indexed_results.unwrap_or_default();
        if !args.include_escape_refs {
            indexed.retain(|entry| !entry.is_escape_local);
        }
        let results: Vec<SymbolEntry> = indexed
            .iter()
            .map(|e| SymbolEntry {
                symbol: e.symbol.clone(),
                kind: e.kind.clone(),
                hash: e.hash.clone(),
                file: e.file.clone(),
                lines: e.lines.clone(),
                module: e.module.clone(),
                risk: e.risk.clone(),
            })
            .collect();
        Some(SymbolSearchResults { results })
    }
}

/// Get semantic matches from the BM25 index
fn get_semantic_matches(cache: &CacheDir, args: &SearchArgs) -> Option<SemanticSearchResults> {
    use crate::bm25::Bm25Index;

    if !cache.has_bm25_index() {
        return None;
    }

    let bm25_path = cache.bm25_index_path();
    let index = Bm25Index::load(&bm25_path).ok()?;

    let mut results = index.search(&args.query, args.limit);

    // Apply filters
    if let Some(ref kind_filter) = args.kind {
        let kind_lower = kind_filter.to_lowercase();
        results.retain(|r| r.kind.to_lowercase() == kind_lower);
    }
    if let Some(ref module_filter) = args.module {
        let module_lower = module_filter.to_lowercase();
        results.retain(|r| r.module.to_lowercase() == module_lower);
    }
    if !args.include_escape_refs {
        let escape_hashes = load_escape_local_hashes(cache);
        results.retain(|r| !escape_hashes.contains(&r.hash));
    }

    results.truncate(args.limit / 2); // Half limit for hybrid

    let suggestions = index.suggest_related_terms(&args.query, 3);

    let semantic_results: Vec<SemanticEntry> = results
        .iter()
        .map(|r| SemanticEntry {
            symbol: r.symbol.clone(),
            kind: r.kind.clone(),
            hash: r.hash.clone(),
            file: r.file.clone(),
            lines: r.lines.clone(),
            module: r.module.clone(),
            risk: r.risk.clone(),
            score: r.score as f32,
            matched_terms: r.matched_terms.clone(),
        })
        .collect();

    Some(SemanticSearchResults {
        results: semantic_results,
        suggestions,
    })
}

fn load_escape_local_hashes(cache: &CacheDir) -> HashSet<String> {
    cache
        .load_all_symbol_entries()
        .map(|entries| {
            entries
                .into_iter()
                .filter(|e| e.is_escape_local)
                .map(|e| e.hash)
                .collect()
        })
        .unwrap_or_default()
}

/// Extract a likely symbol name from a line of code
fn extract_symbol_name(content: &str) -> String {
    // Try to extract function/class/const name from code line
    let content = content.trim();

    // Common patterns: fn name, function name, class Name, const NAME
    let patterns = [
        r"(?:fn|function|def|func)\s+(\w+)",
        r"(?:class|struct|interface|enum|type)\s+(\w+)",
        r"(?:const|let|var)\s+(\w+)",
    ];

    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(cap) = re.captures(content) {
                if let Some(name) = cap.get(1) {
                    return name.as_str().to_string();
                }
            }
        }
    }

    // Fallback: first word-like token
    content
        .split_whitespace()
        .find(|w| w.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .unwrap_or("unknown")
        .to_string()
}

/// Get source code snippet for a symbol
fn get_source_snippet(cache: &CacheDir, file: &str, lines: &str, context: usize) -> Option<String> {
    // Parse line range
    let parts: Vec<&str> = lines.split('-').collect();
    let start: usize = parts.first()?.parse().ok()?;
    let end: usize = parts.get(1).unwrap_or(&parts[0]).parse().ok()?;

    // Read file
    let file_path = cache.root.parent()?.join(file);
    let content = std::fs::read_to_string(&file_path).ok()?;
    let all_lines: Vec<&str> = content.lines().collect();

    // Calculate range with context
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
