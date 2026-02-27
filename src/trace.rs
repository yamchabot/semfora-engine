//! Trace utility for usage traversal across call graphs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::cache::{CacheDir, SymbolIndexEntry};
use crate::commands::toon_parser::read_cached_file;
use crate::schema::{CallGraphEdge, RefKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceDirection {
    Incoming,
    Outgoing,
    Both,
}

#[derive(Debug, Clone)]
pub struct TraceOptions {
    pub target: String,
    pub target_kind: Option<String>,
    pub depth: usize,
    pub limit: usize,
    pub offset: usize,
    pub include_escape_refs: bool,
    pub include_external: bool,
    pub direction: TraceDirection,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceNode {
    pub hash: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub file: Option<String>,
    pub lines: Option<String>,
    pub is_escape_local: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub depth_reached: usize,
    pub roots_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceResult {
    pub roots: Vec<String>,
    pub nodes: Vec<TraceNode>,
    pub edges: Vec<TraceEdge>,
    pub stats: TraceStats,
}

struct SymbolResolver {
    resolved: HashMap<String, SymbolIndexEntry>,
    misses: HashSet<String>,
}

impl SymbolResolver {
    fn new() -> Self {
        Self {
            resolved: HashMap::new(),
            misses: HashSet::new(),
        }
    }

    fn resolve(&mut self, cache: &CacheDir, hash: &str) -> Option<SymbolIndexEntry> {
        if let Some(entry) = self.resolved.get(hash) {
            return Some(entry.clone());
        }
        if self.misses.contains(hash) {
            return None;
        }

        if let Some(entry) = resolve_from_symbol_shard(cache, hash) {
            self.resolved.insert(hash.to_string(), entry.clone());
            return Some(entry);
        }

        if let Some(entry) = resolve_from_symbol_index(cache, hash) {
            self.resolved.insert(hash.to_string(), entry.clone());
            return Some(entry);
        }

        self.misses.insert(hash.to_string());
        None
    }
}

pub fn trace(cache: &CacheDir, options: TraceOptions) -> crate::error::Result<TraceResult> {
    let call_graph = cache.load_call_graph()?;
    if call_graph.is_empty() {
        return Err(crate::McpDiffError::FileNotFound {
            path: "Call graph not found or empty. Run `semfora index generate` first.".to_string(),
        });
    }

    let roots = resolve_trace_roots(cache, &options)?;
    let roots_truncated = roots.truncated;

    let mut reverse_graph = HashMap::new();
    if matches!(
        options.direction,
        TraceDirection::Incoming | TraceDirection::Both
    ) {
        reverse_graph = build_reverse_graph(
            &call_graph,
            options.include_escape_refs,
            options.include_external,
        );
    }

    let mut edges: Vec<TraceEdge> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut nodes: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();

    for root in &roots.hashes {
        nodes.insert(root.clone());
        queue.push_back((root.clone(), 0));
    }

    let mut depth_reached = 0;

    while let Some((current, depth)) = queue.pop_front() {
        if depth > options.depth {
            continue;
        }
        depth_reached = depth_reached.max(depth);

        if !visited.insert(current.clone()) {
            continue;
        }

        if matches!(
            options.direction,
            TraceDirection::Outgoing | TraceDirection::Both
        ) {
            if let Some(callees) = call_graph.get(&current) {
                for callee in callees {
                    let edge = CallGraphEdge::decode(callee);
                    if edge.edge_kind.is_escape_ref() && !options.include_escape_refs {
                        continue;
                    }
                    if !options.include_external && edge.callee.starts_with("ext:") {
                        continue;
                    }
                    edges.push(TraceEdge {
                        from: current.clone(),
                        to: edge.callee.clone(),
                        kind: edge.edge_kind.as_edge_kind().to_string(),
                    });
                    if nodes.insert(edge.callee.clone()) {
                        queue.push_back((edge.callee, depth + 1));
                    }
                }
            }
        }

        if matches!(
            options.direction,
            TraceDirection::Incoming | TraceDirection::Both
        ) {
            if let Some(callers) = reverse_graph.get(&current) {
                for (caller, edge_kind) in callers {
                    edges.push(TraceEdge {
                        from: caller.clone(),
                        to: current.clone(),
                        kind: edge_kind.as_edge_kind().to_string(),
                    });
                    if nodes.insert(caller.clone()) {
                        queue.push_back((caller.clone(), depth + 1));
                    }
                }
            }
        }

        if edges.len() >= options.limit.saturating_add(options.offset) {
            break;
        }
    }

    let edges_len = edges.len();
    let start = options.offset.min(edges_len);
    let end = (start + options.limit).min(edges_len);
    let paged_edges = edges[start..end].to_vec();

    let mut resolver = SymbolResolver::new();
    let mut node_list: Vec<TraceNode> = Vec::new();
    for hash in nodes.iter() {
        if hash.starts_with("ext:") {
            if options.include_external {
                node_list.push(TraceNode {
                    hash: hash.clone(),
                    name: Some(hash.trim_start_matches("ext:").to_string()),
                    kind: Some("external".to_string()),
                    file: None,
                    lines: None,
                    is_escape_local: false,
                });
            }
            continue;
        }

        let entry = resolver.resolve(cache, hash);
        node_list.push(TraceNode {
            hash: hash.clone(),
            name: entry.as_ref().map(|e| e.symbol.clone()),
            kind: entry.as_ref().map(|e| e.kind.clone()),
            file: entry.as_ref().map(|e| e.file.clone()),
            lines: entry.as_ref().map(|e| e.lines.clone()),
            is_escape_local: entry.as_ref().map(|e| e.is_escape_local).unwrap_or(false),
        });
    }

    let stats = TraceStats {
        total_nodes: node_list.len(),
        total_edges: paged_edges.len(),
        depth_reached,
        roots_truncated,
    };

    Ok(TraceResult {
        roots: roots.hashes,
        nodes: node_list,
        edges: paged_edges,
        stats,
    })
}

struct RootResult {
    hashes: Vec<String>,
    truncated: bool,
}

fn resolve_trace_roots(
    cache: &CacheDir,
    options: &TraceOptions,
) -> crate::error::Result<RootResult> {
    let target = options.target.trim();
    let target_kind = options.target_kind.as_ref().map(|k| k.to_lowercase());

    if looks_like_hash(target) {
        return Ok(RootResult {
            hashes: vec![normalize_edge_hash(target)],
            truncated: false,
        });
    }

    if matches!(target_kind.as_deref(), Some("module")) {
        let entries = cache.list_module_symbols(target, None, None, 200)?;
        return Ok(RootResult {
            hashes: entries.into_iter().map(|e| e.hash).collect(),
            truncated: false,
        });
    }

    if matches!(target_kind.as_deref(), Some("file")) {
        let entries = list_symbols_in_file(cache, target, 200)?;
        return Ok(RootResult {
            hashes: entries.into_iter().map(|e| e.hash).collect(),
            truncated: false,
        });
    }

    let mut exact: Vec<String> = Vec::new();
    let mut partial: Vec<String> = Vec::new();
    let max_roots = 200usize;

    let index_path = cache.symbol_index_path();
    if index_path.exists() {
        let file = fs::File::open(index_path)?;
        let reader = BufReader::new(file);
        let target_lower = target.to_lowercase();
        let kind_filter = target_kind.as_deref().filter(|k| *k != "symbol");

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }
            let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if let Some(kind) = kind_filter {
                if entry.kind != kind {
                    continue;
                }
            }

            let name_lower = entry.symbol.to_lowercase();
            if name_lower == target_lower {
                exact.push(entry.hash);
            } else if name_lower.contains(&target_lower) {
                partial.push(entry.hash);
            }

            if exact.len() >= max_roots {
                break;
            }
        }
    }

    let hashes = if !exact.is_empty() { exact } else { partial };
    Ok(RootResult {
        truncated: hashes.len() > max_roots,
        hashes: hashes.into_iter().take(max_roots).collect(),
    })
}

fn resolve_from_symbol_shard(cache: &CacheDir, hash: &str) -> Option<SymbolIndexEntry> {
    let path = cache.symbol_path(hash);
    if !path.exists() {
        return None;
    }
    let cached = read_cached_file(&path).ok()?;
    let json = cached.json;

    symbol_from_json(&json, "")
}

fn resolve_from_symbol_index(cache: &CacheDir, hash: &str) -> Option<SymbolIndexEntry> {
    let index_path = cache.symbol_index_path();
    if !index_path.exists() {
        return None;
    }
    let file = fs::File::open(index_path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: SymbolIndexEntry = serde_json::from_str(&line).ok()?;
        if entry.hash == hash {
            return Some(entry);
        }
    }
    None
}

fn list_symbols_in_file(
    cache: &CacheDir,
    file_path: &str,
    limit: usize,
) -> crate::error::Result<Vec<SymbolIndexEntry>> {
    let index_path = cache.symbol_index_path();
    if !index_path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&index_path)?;
    let reader = BufReader::new(file);
    let target = file_path.trim_start_matches("./");
    let mut results = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let entry_file = entry.file.trim_start_matches("./");
        if entry_file == target || entry_file.ends_with(target) || target.ends_with(entry_file) {
            results.push(entry);
            if results.len() >= limit {
                break;
            }
        }
    }

    Ok(results)
}

fn symbol_from_json(sym: &serde_json::Value, module_name: &str) -> Option<SymbolIndexEntry> {
    let symbol = sym
        .get("symbol")
        .or_else(|| sym.get("s"))
        .or_else(|| sym.get("name"))
        .and_then(|s| s.as_str())?
        .to_string();
    let hash = sym
        .get("hash")
        .or_else(|| sym.get("h"))?
        .as_str()?
        .to_string();
    let kind = sym
        .get("kind")
        .or_else(|| sym.get("k"))
        .and_then(|k| k.as_str())
        .unwrap_or("?")
        .to_string();
    let file = sym
        .get("file")
        .or_else(|| sym.get("f"))
        .and_then(|f| f.as_str())
        .unwrap_or("?")
        .to_string();
    let lines = sym
        .get("lines")
        .or_else(|| sym.get("l"))
        .and_then(|l| l.as_str())
        .unwrap_or("?")
        .to_string();
    let risk = sym
        .get("risk")
        .or_else(|| sym.get("r"))
        .and_then(|r| r.as_str())
        .unwrap_or("low")
        .to_string();
    let cognitive_complexity = sym.get("cc").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
    let max_nesting = sym.get("nest").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
    let is_escape_local = sym
        .get("is_escape_local")
        .or_else(|| sym.get("el"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let framework_entry_point = sym
        .get("framework_entry_point")
        .or_else(|| sym.get("fep"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let is_exported = sym
        .get("is_exported")
        .or_else(|| sym.get("exp"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let decorators = sym
        .get("decorators")
        .or_else(|| sym.get("dec"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let arity = sym
        .get("arity")
        .or_else(|| sym.get("ar"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    Some(SymbolIndexEntry {
        symbol,
        hash,
        semantic_hash: String::new(),
        kind,
        module: module_name.to_string(),
        file,
        lines,
        risk,
        cognitive_complexity,
        max_nesting,
        is_escape_local,
        framework_entry_point,
        is_exported,
        decorators,
        arity,
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
    })
}

fn looks_like_hash(value: &str) -> bool {
    value.contains(':') && value.chars().all(|c| c.is_ascii_hexdigit() || c == ':')
}

fn normalize_edge_hash(sym: &str) -> String {
    CallGraphEdge::decode(sym).callee
}

fn build_reverse_graph(
    call_graph: &HashMap<String, Vec<String>>,
    include_escape_refs: bool,
    include_external: bool,
) -> HashMap<String, Vec<(String, RefKind)>> {
    let mut reverse = HashMap::new();

    for (caller, callees) in call_graph {
        for callee in callees {
            let edge = CallGraphEdge::decode(callee);
            if edge.edge_kind.is_escape_ref() && !include_escape_refs {
                continue;
            }
            if !include_external && edge.callee.starts_with("ext:") {
                continue;
            }
            reverse
                .entry(edge.callee.clone())
                .or_insert_with(Vec::new)
                .push((caller.clone(), edge.edge_kind));
        }
    }

    reverse
}
