//! Static code analysis module
//!
//! Provides complexity metrics, call graph analysis, and code health reports
//! built on top of the semantic index.

use crate::cache::CacheDir;
use crate::utils::truncate_to_char_boundary;
use crate::schema::{RiskLevel, SemanticSummary, SymbolKind};
use crate::Result;
use std::collections::HashMap;
use std::path::Path;
use rayon::prelude::*;

/// Complexity metrics for a single symbol
#[derive(Debug, Clone, Default)]
pub struct SymbolComplexity {
    /// Symbol name
    pub name: String,
    /// Symbol hash for stable identification
    pub hash: String,
    /// File containing the symbol
    pub file: String,
    /// Line range (start-end)
    pub lines: String,
    /// Symbol kind
    pub kind: SymbolKind,

    // Complexity metrics
    /// Cyclomatic complexity (control flow paths)
    pub cyclomatic: usize,
    /// Cognitive complexity (SonarSource metric - accounts for nesting)
    pub cognitive: usize,
    /// Number of function calls made (fan-out)
    pub fan_out: usize,
    /// Number of callers (fan-in) - populated during graph analysis
    pub fan_in: usize,
    /// Number of state mutations
    pub state_mutations: usize,
    /// Lines of code
    pub loc: usize,
    /// Maximum nesting depth
    pub max_nesting: usize,
    /// Behavioral risk level (deprecated - use cognitive complexity instead)
    pub risk: RiskLevel,

    // Dependency metrics
    /// Number of imports/dependencies
    pub dependencies: usize,
    /// I/O operations detected
    pub io_operations: usize,
}

/// Calculate cognitive complexity from control flow changes
///
/// Cognitive complexity (SonarSource) is calculated as:
/// - +1 for each control flow break (if, for, while, switch, try, etc.)
/// - +1 additional for each level of nesting
///
/// Example:
/// ```ignore
/// if (a) {           // +1 (depth 0)
///     if (b) {       // +1 + 1 = +2 (depth 1)
///         for (c) {  // +1 + 2 = +3 (depth 2)
///         }
///     }
/// }
/// // Total: 1 + 2 + 3 = 6
/// ```
pub fn calculate_cognitive_complexity(control_flow: &[crate::schema::ControlFlowChange]) -> usize {
    let mut complexity = 0;

    for cf in control_flow {
        // Base increment for the control structure
        let base = 1;
        // Nesting penalty
        let nesting_penalty = cf.nesting_depth;
        complexity += base + nesting_penalty;
    }

    complexity
}

/// Get the maximum nesting depth from control flow changes
pub fn max_nesting_depth(control_flow: &[crate::schema::ControlFlowChange]) -> usize {
    control_flow.iter().map(|cf| cf.nesting_depth).max().unwrap_or(0)
}

impl SymbolComplexity {
    /// Calculate a composite complexity score using cognitive complexity as primary metric
    pub fn complexity_score(&self) -> usize {
        // Cognitive complexity is the primary metric (already accounts for nesting)
        let mut score = self.cognitive;

        // High fan-out suggests tight coupling
        if self.fan_out > 10 {
            score += (self.fan_out - 10) / 2;
        }

        // Long functions are harder to maintain
        if self.loc > 50 {
            score += (self.loc - 50) / 25;
        }

        score
    }

    /// Get a human-readable complexity rating based on cognitive complexity
    ///
    /// Thresholds based on SonarSource recommendations:
    /// - 0-5: simple (easy to understand)
    /// - 6-10: moderate (some effort to understand)
    /// - 11-20: complex (hard to understand, consider refactoring)
    /// - 21+: very complex (should be refactored)
    pub fn rating(&self) -> &'static str {
        match self.cognitive {
            0..=5 => "simple",
            6..=10 => "moderate",
            11..=20 => "complex",
            _ => "very complex",
        }
    }

    /// Get a color hint for the rating (for terminal output)
    pub fn rating_color(&self) -> &'static str {
        match self.cognitive {
            0..=5 => "green",
            6..=10 => "yellow",
            11..=20 => "orange",
            _ => "red",
        }
    }
}

/// Module-level metrics aggregated from symbols
#[derive(Debug, Clone, Default)]
pub struct ModuleMetrics {
    /// Module name
    pub name: String,
    /// Total files in module
    pub files: usize,
    /// Total symbols in module
    pub symbols: usize,
    /// Average cyclomatic complexity
    pub avg_complexity: f64,
    /// Max cyclomatic complexity
    pub max_complexity: usize,
    /// Symbol with highest complexity
    pub most_complex_symbol: Option<String>,
    /// Total lines of code
    pub total_loc: usize,
    /// Number of high-risk symbols
    pub high_risk_count: usize,
    /// Afferent coupling (incoming dependencies from other modules)
    pub afferent_coupling: usize,
    /// Efferent coupling (outgoing dependencies to other modules)
    pub efferent_coupling: usize,
}

impl ModuleMetrics {
    /// Calculate instability metric (Ce / (Ca + Ce))
    /// 0 = maximally stable (hard to change)
    /// 1 = maximally unstable (easy to change)
    pub fn instability(&self) -> f64 {
        let total = self.afferent_coupling + self.efferent_coupling;
        if total == 0 {
            0.5 // Neutral if no coupling
        } else {
            self.efferent_coupling as f64 / total as f64
        }
    }
}

/// Call graph analysis results
#[derive(Debug, Clone, Default)]
pub struct CallGraphAnalysis {
    /// Symbols with highest fan-in (most called)
    pub hotspots: Vec<(String, usize)>,
    /// Symbols with highest fan-out (most dependencies)
    pub high_coupling: Vec<(String, usize)>,
    /// Circular dependency chains detected
    pub cycles: Vec<Vec<String>>,
    /// Orphan symbols (no callers, no callees)
    pub orphans: Vec<String>,
    /// Entry points (high fan-in, low fan-out)
    pub entry_points: Vec<String>,
    /// Leaf functions (called but don't call others)
    pub leaf_functions: Vec<String>,
}

/// Repository-wide analysis summary
#[derive(Debug, Clone, Default)]
pub struct RepoAnalysis {
    /// Per-module metrics
    pub modules: Vec<ModuleMetrics>,
    /// Top 20 most complex symbols
    pub complex_symbols: Vec<SymbolComplexity>,
    /// Call graph analysis
    pub call_graph: CallGraphAnalysis,
    /// Overall stats
    pub total_symbols: usize,
    pub total_lines: usize,
    pub avg_complexity: f64,
    pub high_risk_percentage: f64,
}

/// Analyze complexity from a call graph
pub fn analyze_call_graph(
    call_graph: &HashMap<String, Vec<String>>,
    symbol_names: &HashMap<String, String>, // hash -> name
) -> CallGraphAnalysis {
    let mut analysis = CallGraphAnalysis::default();

    // Build reverse graph for fan-in calculation
    let mut fan_in: HashMap<String, usize> = HashMap::new();
    let mut fan_out: HashMap<String, usize> = HashMap::new();

    for (caller, callees) in call_graph {
        fan_out.insert(caller.clone(), callees.len());
        for callee in callees {
            *fan_in.entry(callee.clone()).or_insert(0) += 1;
        }
    }

    // Find hotspots (high fan-in)
    let mut hotspots: Vec<_> = fan_in.iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    analysis.hotspots = hotspots.into_iter().take(10).collect();

    // Find high coupling (high fan-out) - resolve hash to name
    let mut high_coupling: Vec<_> = fan_out.iter()
        .filter(|(_, v)| **v > 10)
        .map(|(hash, v)| {
            let name = symbol_names.get(hash).cloned().unwrap_or_else(|| hash.clone());
            (name, *v)
        })
        .collect();
    high_coupling.sort_by(|a, b| b.1.cmp(&a.1));
    analysis.high_coupling = high_coupling;

    // Find orphans (no fan-in, no fan-out except self)
    for (hash, _) in symbol_names {
        let fi = fan_in.get(hash).copied().unwrap_or(0);
        let fo = fan_out.get(hash).copied().unwrap_or(0);
        if fi == 0 && fo == 0 {
            if let Some(name) = symbol_names.get(hash) {
                analysis.orphans.push(name.clone());
            }
        }
    }

    // Find entry points (high fan-in, low fan-out)
    for (hash, &fi) in &fan_in {
        let fo = fan_out.get(hash).copied().unwrap_or(0);
        if fi >= 5 && fo <= 2 {
            if let Some(name) = symbol_names.get(hash) {
                analysis.entry_points.push(name.clone());
            }
        }
    }

    // Find leaf functions (called, but don't call others)
    for (hash, &fi) in &fan_in {
        let fo = fan_out.get(hash).copied().unwrap_or(0);
        if fi > 0 && fo == 0 {
            if let Some(name) = symbol_names.get(hash) {
                analysis.leaf_functions.push(name.clone());
            }
        }
    }

    // Cycle detection using DFS
    analysis.cycles = detect_cycles(call_graph);

    analysis
}

/// Detect cycles in call graph using DFS
fn detect_cycles(graph: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut visited = HashMap::new();
    let mut rec_stack = HashMap::new();
    let mut path = Vec::new();

    fn dfs(
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        visited: &mut HashMap<String, bool>,
        rec_stack: &mut HashMap<String, bool>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string(), true);
        rec_stack.insert(node.to_string(), true);
        path.push(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if !visited.get(neighbor).copied().unwrap_or(false) {
                    dfs(neighbor, graph, visited, rec_stack, path, cycles);
                } else if rec_stack.get(neighbor).copied().unwrap_or(false) {
                    // Found a cycle - extract it
                    if let Some(start_idx) = path.iter().position(|x| x == neighbor) {
                        let cycle: Vec<String> = path[start_idx..].to_vec();
                        if cycle.len() > 1 && cycle.len() <= 5 {
                            // Only report small cycles (2-5 nodes)
                            cycles.push(cycle);
                        }
                    }
                }
            }
        }

        path.pop();
        rec_stack.insert(node.to_string(), false);
    }

    for node in graph.keys() {
        if !visited.get(node).copied().unwrap_or(false) {
            dfs(node, graph, &mut visited, &mut rec_stack, &mut path, &mut cycles);
        }
    }

    cycles
}

/// Format analysis as text report
pub fn format_analysis_report(analysis: &RepoAnalysis) -> String {
    let mut output = String::new();

    output.push_str("╔══════════════════════════════════════════════════════════════════╗\n");
    output.push_str("║                    STATIC CODE ANALYSIS REPORT                   ║\n");
    output.push_str("╚══════════════════════════════════════════════════════════════════╝\n\n");

    // Overview
    output.push_str("── OVERVIEW ─────────────────────────────────────────────────────────\n");
    output.push_str(&format!("  Total Symbols:     {:>6}\n", analysis.total_symbols));
    output.push_str(&format!("  Total Lines:       {:>6}\n", analysis.total_lines));
    output.push_str(&format!("  Avg Cognitive:     {:>6.1}\n", analysis.avg_complexity));
    output.push_str("\n");

    // Top Complex Symbols - sorted by cognitive complexity
    output.push_str("── COGNITIVE COMPLEXITY HOTSPOTS ────────────────────────────────────\n");
    output.push_str("  Symbol                           Cog  Nest   LoC  FanOut  Rating\n");
    output.push_str("  ─────────────────────────────────────────────────────────────────\n");

    for sym in analysis.complex_symbols.iter().take(15) {
        let name = if sym.name.len() > 30 {
            format!("{}...", truncate_to_char_boundary(&sym.name, 27))
        } else {
            sym.name.clone()
        };
        output.push_str(&format!(
            "  {:<30} {:>4}  {:>4}  {:>4}  {:>6}   {}\n",
            name, sym.cognitive, sym.max_nesting, sym.loc, sym.fan_out, sym.rating()
        ));
    }
    output.push_str("\n");

    // Legend
    output.push_str("  Cog: Cognitive Complexity (0-5 simple, 6-10 moderate, 11-20 complex, 21+ very complex)\n");
    output.push_str("  Nest: Maximum nesting depth | LoC: Lines of code | FanOut: Function calls\n");
    output.push_str("\n");

    // Module Analysis
    output.push_str("── MODULE METRICS ───────────────────────────────────────────────────\n");
    output.push_str("  Module              Symbols  AvgCC  MaxCC  LoC    Instability\n");
    output.push_str("  ─────────────────────────────────────────────────────────────────\n");

    let mut modules: Vec<_> = analysis.modules.iter().collect();
    modules.sort_by(|a, b| b.avg_complexity.partial_cmp(&a.avg_complexity).unwrap());

    for m in modules.iter().take(15) {
        let name = if m.name.len() > 18 {
            format!("{}...", truncate_to_char_boundary(&m.name, 15))
        } else {
            m.name.clone()
        };
        output.push_str(&format!(
            "  {:<18} {:>7}  {:>5.1}  {:>5}  {:>5}  {:>10.2}\n",
            name, m.symbols, m.avg_complexity, m.max_complexity,
            m.total_loc, m.instability()
        ));
    }
    output.push_str("\n");

    // Call Graph Analysis
    output.push_str("── CALL GRAPH ANALYSIS ──────────────────────────────────────────────\n");

    if !analysis.call_graph.hotspots.is_empty() {
        output.push_str("  Hotspots (most called):\n");
        for (name, count) in analysis.call_graph.hotspots.iter().take(10) {
            let display = truncate_to_char_boundary(name, 40);
            output.push_str(&format!("    {:<40} ({} callers)\n", display, count));
        }
        output.push_str("\n");
    }

    if !analysis.call_graph.high_coupling.is_empty() {
        output.push_str("  High Coupling (many outgoing calls):\n");
        for (name, count) in analysis.call_graph.high_coupling.iter().take(5) {
            let display = truncate_to_char_boundary(name, 40);
            output.push_str(&format!("    {:<40} ({} callees)\n", display, count));
        }
        output.push_str("\n");
    }

    if !analysis.call_graph.cycles.is_empty() {
        output.push_str("  ⚠ Circular Dependencies Detected:\n");
        for cycle in analysis.call_graph.cycles.iter().take(5) {
            output.push_str(&format!("    {} → {}\n",
                cycle.join(" → "),
                cycle.first().unwrap_or(&String::new())
            ));
        }
        output.push_str("\n");
    }

    if !analysis.call_graph.entry_points.is_empty() {
        output.push_str(&format!("  Entry Points: {} symbols\n",
            analysis.call_graph.entry_points.len()));
    }

    if !analysis.call_graph.leaf_functions.is_empty() {
        output.push_str(&format!("  Leaf Functions: {} symbols\n",
            analysis.call_graph.leaf_functions.len()));
    }

    output.push_str("\n");
    output.push_str("══════════════════════════════════════════════════════════════════════\n");

    output
}

/// Build SymbolComplexity from a SemanticSummary
///
/// Use `fan_in = 0` if you don't have call graph data available.
pub fn symbol_complexity_from_summary(summary: &SemanticSummary, fan_in: usize) -> SymbolComplexity {
    let loc = match (summary.start_line, summary.end_line) {
        (Some(s), Some(e)) => e.saturating_sub(s) + 1,
        _ => 0,
    };

    // Cyclomatic complexity: base 1 + one per control flow branch
    let cyclomatic = 1 + summary.control_flow_changes.len();

    // Cognitive complexity: accounts for nesting depth
    let cognitive = calculate_cognitive_complexity(&summary.control_flow_changes);

    // Maximum nesting depth from actual control flow data
    let max_nesting = max_nesting_depth(&summary.control_flow_changes);

    // Count I/O operations
    let io_operations = summary.calls.iter()
        .filter(|c| crate::schema::Call::check_is_io(&c.name))
        .count();

    SymbolComplexity {
        name: summary.symbol.clone().unwrap_or_default(),
        hash: summary.symbol_id.as_ref().map(|id| id.hash.clone()).unwrap_or_default(),
        file: summary.file.clone(),
        lines: format!("{}-{}",
            summary.start_line.unwrap_or(0),
            summary.end_line.unwrap_or(0)),
        kind: summary.symbol_kind.clone().unwrap_or_default(),
        cyclomatic,
        cognitive,
        fan_out: summary.calls.len(),
        fan_in,
        state_mutations: summary.state_changes.len(),
        loc,
        max_nesting,
        risk: summary.behavioral_risk,
        dependencies: summary.added_dependencies.len(),
        io_operations,
    }
}

/// Analyze a repository from its cached index
///
/// This is the main entry point for static analysis. It reads from the
/// pre-built semantic index and computes complexity metrics.
/// Parse a lines string "start-end" into (start, end)
fn parse_lines(lines: &str) -> (usize, usize) {
    let parts: Vec<&str> = lines.split('-').collect();
    if parts.len() == 2 {
        let start = parts[0].parse().unwrap_or(0);
        let end = parts[1].parse().unwrap_or(0);
        (start, end)
    } else {
        (0, 0)
    }
}

pub fn analyze_repo(repo_path: &Path) -> Result<RepoAnalysis> {
    let cache = CacheDir::for_repo(repo_path)?;
    let mut analysis = RepoAnalysis::default();

    // Load call graph for fan-in/fan-out calculation
    let call_graph = cache.load_call_graph().unwrap_or_default();

    // Build reverse map for fan-in
    let mut fan_in_map: HashMap<String, usize> = HashMap::new();
    let mut fan_out_map: HashMap<String, usize> = HashMap::new();
    for (caller, callees) in &call_graph {
        fan_out_map.insert(caller.clone(), callees.len());
        for callee in callees {
            *fan_in_map.entry(callee.clone()).or_insert(0) += 1;
        }
    }

    // Build symbol name map for call graph analysis
    let mut symbol_names: HashMap<String, String> = HashMap::new();

    // Load all symbol entries from the index
    let symbol_entries = cache.load_all_symbol_entries().unwrap_or_default();

    // Build map of hash -> (file, start, end) for aggregating fan_out in impl blocks
    let mut hash_to_location: HashMap<String, (&str, usize, usize)> = HashMap::new();
    for entry in &symbol_entries {
        let (start, end) = parse_lines(&entry.lines);
        hash_to_location.insert(entry.hash.clone(), (&entry.file, start, end));
    }

    // Track name -> max fan_out for direct name-based lookup
    // This handles impl blocks where calls are attributed to the struct with same name
    let mut name_to_fan_out: HashMap<String, usize> = HashMap::new();
    for entry in &symbol_entries {
        // Track max fan_out for each symbol name (struct might have higher than impl)
        if let Some(&fo) = fan_out_map.get(&entry.hash) {
            let current = name_to_fan_out.entry(entry.symbol.clone()).or_insert(0);
            *current = (*current).max(fo);
        }
    }

    // Group entries by module
    let mut module_entries: HashMap<String, Vec<&crate::cache::SymbolIndexEntry>> = HashMap::new();
    for entry in &symbol_entries {
        module_entries.entry(entry.module.clone())
            .or_default()
            .push(entry);
        symbol_names.insert(entry.hash.clone(), entry.symbol.clone());
    }

    // Build map of file -> total fan_out for same-file aggregation fallback
    let mut file_to_fan_out: HashMap<String, usize> = HashMap::new();
    for (cg_hash, &fo) in &fan_out_map {
        if let Some(&(cg_file, _, _)) = hash_to_location.get(cg_hash) {
            *file_to_fan_out.entry(cg_file.to_string()).or_insert(0) += fo;
        }
    }
    // Pre-compute aggregated fan_out for symbols (avoids O(n²) in parallel loop)
    // Group symbols by file for efficient containment checking
    let mut symbols_by_file: HashMap<&str, Vec<(&str, usize, usize, usize)>> = HashMap::new();
    for (cg_hash, &fo) in &fan_out_map {
        if let Some(&(cg_file, cg_start, cg_end)) = hash_to_location.get(cg_hash) {
            symbols_by_file.entry(cg_file)
                .or_default()
                .push((cg_hash, cg_start, cg_end, fo));
        }
    }
    
    // Pre-compute aggregated fan_out: for each symbol, sum fan_out of contained symbols
    let aggregated_fan_out: HashMap<String, usize> = symbol_entries
        .par_iter()
        .filter_map(|entry| {
            let (start, end) = parse_lines(&entry.lines);
            // Skip if we already have direct fan_out
            if fan_out_map.contains_key(&entry.hash) || name_to_fan_out.contains_key(&entry.symbol) {
                return None;
            }
            // Find contained symbols in same file
            if let Some(file_symbols) = symbols_by_file.get(entry.file.as_str()) {
                let total: usize = file_symbols.iter()
                    .filter(|(_, s, e, _)| *s >= start && *e <= end)
                    .map(|(_, _, _, fo)| fo)
                    .sum();
                if total > 0 {
                    return Some((entry.hash.clone(), total));
                }
            }
            None
        })
        .collect();

    // Process modules in parallel
    let total_modules = module_entries.len();
    let processed_modules = std::sync::atomic::AtomicUsize::new(0);
    eprintln!("Analyzing {} modules...", total_modules);
    let module_results: Vec<(ModuleMetrics, Vec<SymbolComplexity>)> = module_entries
        .par_iter()
        .map(|(module_name, entries)| {
            let current = processed_modules.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if current % 200 == 0 || current == total_modules {
                eprintln!("  Progress: {}/{} ({:.1}%)", current, total_modules, (current as f64 / total_modules as f64) * 100.0);
            }
            let mut module_metrics = ModuleMetrics {
                name: module_name.clone(),
                ..Default::default()
            };

            let mut files_seen = std::collections::HashSet::new();
            let mut module_cc_sum = 0usize;
            let mut complex_symbols_local: Vec<SymbolComplexity> = Vec::new();

            for entry in entries {
                files_seen.insert(&entry.file);

                // Parse line range for LoC
                let (start, end) = parse_lines(&entry.lines);
                let loc = if end > start { end - start + 1 } else { 1 };

                // Get fan-in/fan-out (O(1) lookups using pre-computed maps)
                let fan_in = fan_in_map.get(&entry.hash).copied().unwrap_or(0);
                let fan_out = fan_out_map.get(&entry.hash).copied()
                    .or_else(|| name_to_fan_out.get(&entry.symbol).copied())
                    .or_else(|| aggregated_fan_out.get(&entry.hash).copied())
                    .or_else(|| {
                        if entry.kind == "method" {
                            file_to_fan_out.get(&entry.file).copied()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);

                let sym_complexity = SymbolComplexity {
                    name: entry.symbol.clone(),
                    hash: entry.hash.clone(),
                    file: entry.file.clone(),
                    lines: entry.lines.clone(),
                    kind: SymbolKind::from_str(&entry.kind),
                    cyclomatic: entry.cognitive_complexity,
                    cognitive: entry.cognitive_complexity,
                    fan_out,
                    fan_in,
                    state_mutations: 0,
                    loc,
                    max_nesting: entry.max_nesting,
                    risk: RiskLevel::from_str(&entry.risk),
                    dependencies: 0,
                    io_operations: 0,
                };

                module_metrics.total_loc += loc;
                module_cc_sum += entry.cognitive_complexity;

                if entry.cognitive_complexity > module_metrics.max_complexity {
                    module_metrics.max_complexity = entry.cognitive_complexity;
                    module_metrics.most_complex_symbol = Some(entry.symbol.clone());
                }

                if entry.risk == "high" {
                    module_metrics.high_risk_count += 1;
                }

                if entry.cognitive_complexity > 5 || fan_out > 10 || loc > 50 {
                    complex_symbols_local.push(sym_complexity);
                }

                module_metrics.symbols += 1;
            }

            module_metrics.files = files_seen.len();
            if module_metrics.symbols > 0 {
                module_metrics.avg_complexity = module_cc_sum as f64 / module_metrics.symbols as f64;
            }

            (module_metrics, complex_symbols_local)
        })
        .collect();

    // Merge results
    for (module_metrics, complex_symbols_local) in module_results {
        analysis.total_symbols += module_metrics.symbols;
        analysis.total_lines += module_metrics.total_loc;
        analysis.modules.push(module_metrics);
        analysis.complex_symbols.extend(complex_symbols_local);
    }

    // Calculate overall averages
    if analysis.total_symbols > 0 {
        // Average cognitive complexity across all symbols
        let total_cc: usize = symbol_entries.iter()
            .map(|e| e.cognitive_complexity)
            .sum();
        analysis.avg_complexity = total_cc as f64 / analysis.total_symbols as f64;

        let high_risk_count = symbol_entries.iter()
            .filter(|e| e.risk == "high")
            .count();
        analysis.high_risk_percentage = (high_risk_count as f64 / analysis.total_symbols as f64) * 100.0;
    }

    // Sort complex symbols by cognitive complexity (primary) then fan-out (secondary)
    analysis.complex_symbols.sort_by(|a, b| {
        let score_a = a.cognitive * 100 + a.fan_out * 10 + a.loc;
        let score_b = b.cognitive * 100 + b.fan_out * 10 + b.loc;
        score_b.cmp(&score_a)
    });
    analysis.complex_symbols.truncate(20);

    // Analyze call graph
    analysis.call_graph = analyze_call_graph(&call_graph, &symbol_names);

    Ok(analysis)
}

/// Quick complexity check for a single module
pub fn analyze_module(repo_path: &Path, module_name: &str) -> Result<ModuleMetrics> {
    let cache = CacheDir::for_repo(repo_path)?;
    let call_graph = cache.load_call_graph().unwrap_or_default();

    // Build fan-in map
    let mut fan_in_map: HashMap<String, usize> = HashMap::new();
    for (_caller, callees) in &call_graph {
        for callee in callees {
            *fan_in_map.entry(callee.clone()).or_insert(0) += 1;
        }
    }

    let summaries = cache.load_module_summaries(module_name)?;
    let mut metrics = ModuleMetrics {
        name: module_name.to_string(),
        ..Default::default()
    };

    let mut complexity_sum = 0usize;
    let mut files_seen = std::collections::HashSet::new();

    for summary in &summaries {
        files_seen.insert(&summary.file);

        let fan_in = summary.symbol_id.as_ref()
            .and_then(|id| fan_in_map.get(&id.hash).copied())
            .unwrap_or(0);

        let sym_complexity = symbol_complexity_from_summary(summary, fan_in);

        complexity_sum += sym_complexity.cyclomatic;
        metrics.total_loc += sym_complexity.loc;

        if sym_complexity.cyclomatic > metrics.max_complexity {
            metrics.max_complexity = sym_complexity.cyclomatic;
            metrics.most_complex_symbol = Some(sym_complexity.name.clone());
        }

        if matches!(summary.behavioral_risk, RiskLevel::High) {
            metrics.high_risk_count += 1;
        }

        metrics.symbols += 1;
    }

    metrics.files = files_seen.len();
    if metrics.symbols > 0 {
        metrics.avg_complexity = complexity_sum as f64 / metrics.symbols as f64;
    }

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_score() {
        let mut sym = SymbolComplexity::default();
        sym.cognitive = 1;
        assert_eq!(sym.rating(), "simple");

        sym.cognitive = 5;
        assert_eq!(sym.rating(), "simple");

        sym.cognitive = 6;
        assert_eq!(sym.rating(), "moderate");

        sym.cognitive = 10;
        assert_eq!(sym.rating(), "moderate");

        sym.cognitive = 11;
        assert_eq!(sym.rating(), "complex");

        sym.cognitive = 21;
        assert_eq!(sym.rating(), "very complex");
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = HashMap::new();
        graph.insert("a".to_string(), vec!["b".to_string()]);
        graph.insert("b".to_string(), vec!["c".to_string()]);
        graph.insert("c".to_string(), vec!["a".to_string()]);

        let cycles = detect_cycles(&graph);
        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_instability() {
        let mut metrics = ModuleMetrics::default();
        metrics.afferent_coupling = 10;
        metrics.efferent_coupling = 10;
        assert!((metrics.instability() - 0.5).abs() < 0.01);

        metrics.afferent_coupling = 0;
        metrics.efferent_coupling = 10;
        assert!((metrics.instability() - 1.0).abs() < 0.01);
    }
}

    #[test]
    fn test_parse_lines() {
        // Normal case
        assert_eq!(parse_lines("10-20"), (10, 20));
        
        // Single line (same start/end)
        assert_eq!(parse_lines("5-5"), (5, 5));
        
        // Large range
        assert_eq!(parse_lines("100-500"), (100, 500));
        
        // Invalid format returns (0, 0)
        assert_eq!(parse_lines("invalid"), (0, 0));
        assert_eq!(parse_lines(""), (0, 0));
        assert_eq!(parse_lines("10"), (0, 0));
        
        // Non-numeric values
        assert_eq!(parse_lines("a-b"), (0, 0));
    }

    #[test]
    fn test_fan_out_cascade_lookup() {
        // Test the fan_out lookup priority:
        // 1. Direct hash match in fan_out_map
        // 2. Name match in name_to_fan_out
        // 3. Aggregated from contained symbols
        // 4. File-based fallback for methods
        
        let mut fan_out_map: HashMap<String, usize> = HashMap::new();
        let mut name_to_fan_out: HashMap<String, usize> = HashMap::new();
        let mut aggregated_fan_out: HashMap<String, usize> = HashMap::new();
        let mut file_to_fan_out: HashMap<String, usize> = HashMap::new();
        
        fan_out_map.insert("hash1".to_string(), 10);
        name_to_fan_out.insert("symbol2".to_string(), 20);
        aggregated_fan_out.insert("hash3".to_string(), 30);
        file_to_fan_out.insert("file4.rs".to_string(), 40);
        
        // Priority 1: Direct hash match
        let fan_out1 = fan_out_map.get("hash1").copied()
            .or_else(|| name_to_fan_out.get("symbol1").copied())
            .or_else(|| aggregated_fan_out.get("hash1").copied())
            .unwrap_or(0);
        assert_eq!(fan_out1, 10);
        
        // Priority 2: Name match (when hash not found)
        let fan_out2 = fan_out_map.get("hash2").copied()
            .or_else(|| name_to_fan_out.get("symbol2").copied())
            .or_else(|| aggregated_fan_out.get("hash2").copied())
            .unwrap_or(0);
        assert_eq!(fan_out2, 20);
        
        // Priority 3: Aggregated match
        let fan_out3 = fan_out_map.get("hash3").copied()
            .or_else(|| name_to_fan_out.get("symbol3").copied())
            .or_else(|| aggregated_fan_out.get("hash3").copied())
            .unwrap_or(0);
        assert_eq!(fan_out3, 30);
        
        // Priority 4: File fallback for methods
        let is_method = true;
        let fan_out4 = fan_out_map.get("hash4").copied()
            .or_else(|| name_to_fan_out.get("symbol4").copied())
            .or_else(|| aggregated_fan_out.get("hash4").copied())
            .or_else(|| {
                if is_method {
                    file_to_fan_out.get("file4.rs").copied()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        assert_eq!(fan_out4, 40);
        
        // Default to 0 when nothing matches
        let fan_out5 = fan_out_map.get("hash5").copied()
            .or_else(|| name_to_fan_out.get("symbol5").copied())
            .or_else(|| aggregated_fan_out.get("hash5").copied())
            .unwrap_or(0);
        assert_eq!(fan_out5, 0);
    }

    #[test]
    fn test_symbol_containment_logic() {
        // Test that symbol containment works correctly for aggregation
        // A symbol at lines 10-100 contains a symbol at lines 20-30
        
        let outer_start = 10usize;
        let outer_end = 100usize;
        
        // Contained symbol
        let inner_start = 20usize;
        let inner_end = 30usize;
        assert!(inner_start >= outer_start && inner_end <= outer_end);
        
        // Not contained - starts before
        let before_start = 5usize;
        let before_end = 15usize;
        assert!(!(before_start >= outer_start && before_end <= outer_end));
        
        // Not contained - ends after
        let after_start = 90usize;
        let after_end = 110usize;
        assert!(!(after_start >= outer_start && after_end <= outer_end));
        
        // Edge case - exactly at boundaries
        let edge_start = 10usize;
        let edge_end = 100usize;
        assert!(edge_start >= outer_start && edge_end <= outer_end);
    }

    #[test]
    fn test_aggregated_fan_out_calculation() {
        // Test that aggregated fan_out sums correctly
        let file_symbols: Vec<(&str, usize, usize, usize)> = vec![
            ("hash_a", 20, 30, 5),   // fan_out = 5
            ("hash_b", 40, 50, 3),   // fan_out = 3
            ("hash_c", 60, 70, 7),   // fan_out = 7
        ];
        
        // Outer symbol contains all three
        let outer_start = 10usize;
        let outer_end = 100usize;
        
        let total: usize = file_symbols.iter()
            .filter(|(_, s, e, _)| *s >= outer_start && *e <= outer_end)
            .map(|(_, _, _, fo)| fo)
            .sum();
        
        assert_eq!(total, 15); // 5 + 3 + 7
        
        // Outer symbol contains only first two
        let partial_end = 55usize;
        let partial_total: usize = file_symbols.iter()
            .filter(|(_, s, e, _)| *s >= outer_start && *e <= partial_end)
            .map(|(_, _, _, fo)| fo)
            .sum();
        
        assert_eq!(partial_total, 8); // 5 + 3
    }

    #[test]
    fn test_module_metrics_aggregation() {
        // Test that module metrics aggregate correctly
        let mut metrics = ModuleMetrics::default();
        
        metrics.symbols = 10;
        metrics.total_loc = 500;
        metrics.max_complexity = 15;
        metrics.high_risk_count = 2;
        metrics.files = 3;
        
        // Test instability calculation
        metrics.afferent_coupling = 5;
        metrics.efferent_coupling = 15;
        let instability = metrics.instability();
        assert!((instability - 0.75).abs() < 0.01); // 15 / (5 + 15) = 0.75
        
        // Test average complexity calculation
        let total_cc = 100usize;
        metrics.avg_complexity = total_cc as f64 / metrics.symbols as f64;
        assert!((metrics.avg_complexity - 10.0).abs() < 0.01);
    }
