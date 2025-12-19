//! Data capture for benchmark snapshots
//!
//! Captures semantic analysis data at each build step, including symbols,
//! call graphs, complexity metrics, and timing information.

use super::types::*;
use crate::analysis::{calculate_cognitive_complexity, max_nesting_depth};
use crate::extract::extract;
use crate::lang::Lang;
use crate::overlay::compute_symbol_hash;
use crate::schema::SemanticSummary;
use crate::server::AstCache;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Captures a complete snapshot at a given build step
pub fn capture_snapshot(
    output_dir: &Path,
    step: usize,
    file_path: &str,
    ast_cache: Option<Arc<AstCache>>,
) -> Result<StepSnapshot, String> {
    let total_files = count_ts_files(output_dir);

    // Capture with cache (incremental)
    let timing_cached = capture_with_cache(output_dir, ast_cache.clone())?;

    // Capture without cache (full parse)
    let timing_uncached = capture_without_cache(output_dir)?;

    // Extract symbols and build call graph
    let (symbols, call_graph, complexity) = analyze_project(output_dir)?;

    Ok(StepSnapshot {
        step,
        file: file_path.to_string(),
        total_files,
        symbols,
        call_graph,
        complexity,
        timing_cached,
        timing_uncached,
    })
}

/// Capture timing with AST cache (incremental parsing)
fn capture_with_cache(
    output_dir: &Path,
    ast_cache: Option<Arc<AstCache>>,
) -> Result<TimingData, String> {
    let start = Instant::now();
    let mut parse_time = 0u64;
    let mut extract_time = 0u64;
    let mut incremental_parses = 0usize;
    let mut full_parses = 0usize;
    let mut cache_hits = 0usize;

    let files = collect_ts_files(output_dir);

    for file_path in &files {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

        let lang = Lang::from_path(file_path).map_err(|e| e.to_string())?;

        // Parse with or without cache
        let parse_start = Instant::now();
        let tree = if let Some(ref cache) = ast_cache {
            let (tree, result) = cache
                .parse_file(file_path, &source, lang)
                .map_err(|e| format!("Cache parse failed: {}", e))?;
            match result {
                crate::server::ParseResult::Cached => cache_hits += 1,
                crate::server::ParseResult::Incremental { .. } => incremental_parses += 1,
                crate::server::ParseResult::Full => full_parses += 1,
            }
            tree
        } else {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.tree_sitter_language())
                .map_err(|e| e.to_string())?;
            parser
                .parse(&source, None)
                .ok_or_else(|| "Parse failed".to_string())?
        };
        parse_time += parse_start.elapsed().as_micros() as u64;

        // Extract
        let extract_start = Instant::now();
        let _ = extract(file_path, &source, &tree, lang);
        extract_time += extract_start.elapsed().as_micros() as u64;
    }

    let total_us = start.elapsed().as_micros() as u64;

    Ok(TimingData {
        total_us,
        parse_us: parse_time,
        extract_us: extract_time,
        graph_us: 0, // Will be calculated separately
        cached: ast_cache.is_some(),
        incremental_parses,
        full_parses,
        cache_hits,
    })
}

/// Capture timing without cache (fresh parse every time)
fn capture_without_cache(output_dir: &Path) -> Result<TimingData, String> {
    let start = Instant::now();
    let mut parse_time = 0u64;
    let mut extract_time = 0u64;
    let mut full_parses = 0usize;

    let files = collect_ts_files(output_dir);

    for file_path in &files {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

        let lang = Lang::from_path(file_path).map_err(|e| e.to_string())?;

        // Always create fresh parser
        let parse_start = Instant::now();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|e| e.to_string())?;
        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| "Parse failed".to_string())?;
        parse_time += parse_start.elapsed().as_micros() as u64;
        full_parses += 1;

        // Extract
        let extract_start = Instant::now();
        let _ = extract(file_path, &source, &tree, lang);
        extract_time += extract_start.elapsed().as_micros() as u64;
    }

    let total_us = start.elapsed().as_micros() as u64;

    Ok(TimingData {
        total_us,
        parse_us: parse_time,
        extract_us: extract_time,
        graph_us: 0,
        cached: false,
        incremental_parses: 0,
        full_parses,
        cache_hits: 0,
    })
}

/// Analyze project and extract symbols, call graph, and complexity
fn analyze_project(
    output_dir: &Path,
) -> Result<
    (
        Vec<SymbolSnapshot>,
        HashMap<String, Vec<String>>,
        ComplexitySnapshot,
    ),
    String,
> {
    let files = collect_ts_files(output_dir);
    let mut all_summaries: Vec<SemanticSummary> = Vec::new();
    let mut symbols: Vec<SymbolSnapshot> = Vec::new();
    let mut complexity_entries: Vec<SymbolComplexityEntry> = Vec::new();

    for file_path in &files {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

        let lang = Lang::from_path(file_path).map_err(|e| e.to_string())?;

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|e| e.to_string())?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| "Parse failed".to_string())?;

        let summary = extract(file_path, &source, &tree, lang).map_err(|e| e.to_string())?;

        // Extract symbol snapshots
        let relative_path = file_path
            .strip_prefix(output_dir)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let module = extract_module_name(&relative_path);

        for sym in &summary.symbols {
            let hash = compute_symbol_hash(sym, &relative_path);
            let cognitive = calculate_cognitive_complexity(&sym.control_flow);
            let max_nest = max_nesting_depth(&sym.control_flow);

            symbols.push(SymbolSnapshot {
                hash: hash.clone(),
                name: sym.name.clone(),
                kind: format!("{:?}", sym.kind).to_lowercase(),
                file: relative_path.clone(),
                lines: format!("{}-{}", sym.start_line, sym.end_line),
                module: module.clone(),
                risk: format!("{:?}", sym.behavioral_risk).to_lowercase(),
                cognitive_complexity: cognitive,
                max_nesting: max_nest,
                calls: sym.calls.iter().map(|c| c.name.clone()).collect(),
            });

            complexity_entries.push(SymbolComplexityEntry {
                hash,
                name: sym.name.clone(),
                cognitive,
                cyclomatic: sym.control_flow.len(),
                max_nesting: max_nest,
            });
        }

        all_summaries.push(summary);
    }

    // Build call graph
    let call_graph = build_call_graph(&all_summaries, output_dir);

    // Calculate complexity metrics
    let total_cognitive: usize = complexity_entries.iter().map(|e| e.cognitive).sum();
    let total_cyclomatic: usize = complexity_entries.iter().map(|e| e.cyclomatic).sum();
    let max_cognitive = complexity_entries
        .iter()
        .map(|e| e.cognitive)
        .max()
        .unwrap_or(0);
    let avg_cognitive = if !complexity_entries.is_empty() {
        total_cognitive as f64 / complexity_entries.len() as f64
    } else {
        0.0
    };

    let high_risk_count = symbols.iter().filter(|s| s.risk == "high").count();

    let complexity = ComplexitySnapshot {
        symbols: complexity_entries,
        total_cognitive,
        total_cyclomatic,
        avg_cognitive,
        max_cognitive,
        high_risk_count,
    };

    Ok((symbols, call_graph, complexity))
}

/// Build call graph from semantic summaries
fn build_call_graph(
    summaries: &[SemanticSummary],
    output_dir: &Path,
) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // Build lookup: function name -> hash
    let mut name_to_hash: HashMap<String, String> = HashMap::new();

    for summary in summaries {
        let relative_path = Path::new(&summary.file)
            .strip_prefix(output_dir)
            .unwrap_or(Path::new(&summary.file))
            .to_string_lossy()
            .to_string();

        for sym in &summary.symbols {
            let hash = compute_symbol_hash(sym, &relative_path);
            name_to_hash.insert(sym.name.clone(), hash);
        }
    }

    // Build edges
    for summary in summaries {
        let relative_path = Path::new(&summary.file)
            .strip_prefix(output_dir)
            .unwrap_or(Path::new(&summary.file))
            .to_string_lossy()
            .to_string();

        for sym in &summary.symbols {
            let caller_hash = compute_symbol_hash(sym, &relative_path);

            let mut callees: Vec<String> = Vec::new();
            for call in &sym.calls {
                if let Some(callee_hash) = name_to_hash.get(&call.name) {
                    callees.push(callee_hash.clone());
                }
            }

            if !callees.is_empty() {
                graph.insert(caller_hash, callees);
            }
        }
    }

    graph
}

/// Extract module name from file path
fn extract_module_name(path: &str) -> String {
    // Extract first directory under src/
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 && parts[0] == "src" {
        parts[1].to_string()
    } else if !parts.is_empty() {
        parts[0].to_string()
    } else {
        "root".to_string()
    }
}

/// Collect all TypeScript files in directory
fn collect_ts_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_ts_files_recursive(dir, &mut files);
    files.sort();
    files
}

fn collect_ts_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_ts_files_recursive(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "ts" || ext == "tsx" {
                    files.push(path);
                }
            }
        }
    }
}

/// Count TypeScript files
fn count_ts_files(dir: &Path) -> usize {
    collect_ts_files(dir).len()
}

/// Calculate graph evolution between steps
pub fn calculate_graph_evolution(
    prev_graph: &HashMap<String, Vec<String>>,
    curr_graph: &HashMap<String, Vec<String>>,
    step: usize,
) -> GraphStep {
    // Count edges and nodes
    let mut edges = 0;
    let mut nodes = std::collections::HashSet::new();
    let mut new_edges = Vec::new();

    for (caller, callees) in curr_graph {
        nodes.insert(caller.clone());
        for callee in callees {
            nodes.insert(callee.clone());
            edges += 1;

            // Check if this is a new edge
            let is_new = match prev_graph.get(caller) {
                Some(prev_callees) => !prev_callees.contains(callee),
                None => true,
            };

            if is_new {
                new_edges.push(GraphEdge {
                    from: caller.clone(),
                    to: callee.clone(),
                });
            }
        }
    }

    GraphStep {
        step,
        edges,
        nodes: nodes.len(),
        new_edges,
        graph: curr_graph.clone(),
    }
}
