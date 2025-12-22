# Trace Utility Plan (CLI + MCP)

## Goal
Provide a shared trace utility that returns full usage stacks for any symbol type (function, variable, component, module/file) with edge kinds (call/read/write/readwrite/escape_*), while remaining scalable for very large repos (up to 1TB).

## Core Requirements
- Input: symbol hash or name, optional kind filter.
- Scope: repo/module/file; depth/limit/offset for pagination.
- Direction: incoming, outgoing, or both.
- Include escape refs only when requested.
- Output: nodes + edges with edge_kind and metadata (file, lines, kind) using JSON/TOON/TEXT.

## Performance & Scalability
- Do NOT preload full hash->name maps.
- Use on-demand resolution with a small LRU cache:
  - Resolve by hash via symbol shard (`cache.symbol_path(hash)`) first.
  - Fallback to streaming scan of `symbol_index.jsonl` if needed.
  - Prefer SQLite symbol index if present for large repos.
- Use strict caps (depth/limit/scope) and short-circuit traversal early.
- Allow unresolved hashes in output when resolution is too expensive.

## Implementation Steps
1) Shared core module
   - New `trace.rs` (or `analysis/trace.rs`) with `trace_symbol(...)`.
   - Input: target hash/name, kind filter, scope, depth, limit/offset, include_escape_refs, direction.
   - Output: `TraceResult { nodes, edges, stats }`.

2) Callgraph integration
   - Reuse cached callgraph and `CallGraphEdge::decode` to read edge_kind.
   - Filter by scope and direction without loading unrelated edges when possible.

3) On-demand symbol resolution
   - Implement `resolve_symbol(hash) -> SymbolIndexEntry?` with LRU cache.
   - If not found in shard, scan `symbol_index.jsonl` (streaming) or query SQLite index.

4) CLI wiring
   - Add `semfora trace` command with args matching the core API.
   - Respect include_escape_refs default false.

5) MCP wiring
   - Add MCP `trace` tool with same args and output format.

6) Output formatting
   - JSON/TOON: include edge_kind and hashes; optional name/file/lines.
   - Text: tree-like or tabular output with edge_kind tags.

## Open Questions
- Should we create a dedicated SQLite symbol index for large repos by default?
- How should we treat ambiguous name matches (multiple hashes)?
- Should module/file trace expand to contained symbols automatically or be explicit?
