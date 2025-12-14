# Agent Workflow Improvements Plan

## Overview

This plan outlines improvements to the semfora-engine MCP server to optimize the agent workflow from first prompt to useful results, minimizing context usage while maximizing actionable information.

## Design Principles

1. **Semfora is read-only** - indexing and analysis only, no code editing
2. **Token efficiency** - minimize context consumption at every step
3. **Progressive disclosure** - lightweight context first, details on demand
4. **Quality validation** - leverage existing complexity/duplicate detection
5. **No curated lists** - use algorithmic approaches (BM25) over maintained word lists

---

## Phase 1: Quick Context Tool

### New Tool: `get_context`

**Purpose**: Provide immediate git and project context without full index dump.

**Target token cost**: ~200 tokens

**Response structure**:
```yaml
_type: context
repo_name: "semfora-engine"
branch: "main"
remote: "github.com/Semfora-AI/semfora-engine"
last_commit:
  hash: "04aa817"
  message: "feat: parallelize analysis and indexing"
  author: "..."
  date: "2025-12-14"
index_status: "fresh" | "stale"
stale_files: 3  # if stale
project_type: "Rust CLI + Library"
entry_points: ["src/main.rs", "src/lib.rs"]
```

**Implementation**:
- Git info: `git rev-parse`, `git remote`, `git log -1`
- Index status: Compare index timestamp vs file mtimes
- Project type: Already detected in RepoOverview
- Entry points: Already in RepoOverview

**Files to modify**:
- `src/mcp_server/mod.rs` - Add new tool handler
- `src/mcp_server/types.rs` - Add request/response types

---

## Phase 2: Lightweight Repo Summary

### Modified Tool: `get_repo_summary` (or flag on `get_repo_overview`)

**Purpose**: Replace overwhelming 5000+ token overview with focused 500 token summary.

**Changes**:
1. **Auto-exclude test directories** from module listing
   - Skip paths containing `/test-repos/`, `/tests/`, `/__tests__/`
   - Make configurable via `exclude_patterns` parameter

2. **Limit modules to top N by relevance**
   - Default: 20 modules
   - Sort by: symbol count, risk level, or entry point proximity
   - Parameter: `max_modules: Option<usize>`

3. **Include git context** (from Phase 1)

**Response structure**:
```yaml
_type: repo_summary
context:
  repo: "semfora-engine"
  branch: "main"
  last_commit: "04aa817"
framework: "Rust (bin+lib)"
patterns: ["CLI application", "MCP server", "AST analysis"]
modules[20]:  # Top 20 only
  - name: "mcp_server"
    purpose: "MCP protocol handlers"
    files: 3
    symbols: 57
    risk: "low"
  ...
stats:
  total_files: 136
  total_modules: 47  # Actual count (excluding test-repos)
  total_symbols: 1971
entry_points: ["src/main.rs"]
```

**Files to modify**:
- `src/shard.rs` - Add filtering logic to `generate_sharded_index`
- `src/mcp_server/mod.rs` - Add parameters, include git context
- `src/mcp_server/types.rs` - Extend request type

---

## Phase 3: BM25 Semantic Search

### New Tool: `semantic_search`

**Purpose**: Enable loose term queries like "authentication" or "error handling" that find conceptually related code, not just exact symbol name matches.

**Approach**: BM25 (Best Match 25) ranking algorithm
- No curated word lists required
- Indexes terms from: symbol names, comments, string literals, file paths
- Handles partial matches, stemming optional
- Fast: O(query_terms * matching_docs)

**Implementation options**:

#### Option A: Build BM25 index at shard time
- During `generate_index`, extract terms from each symbol
- Store inverted index in `bm25_index.json` alongside other shards
- Query-time: Load index, compute BM25 scores

#### Option B: Use existing `raw_search` with ranking
- Ripgrep already searches content
- Add BM25 scoring layer on top of grep results
- Less accurate but zero index overhead

**Recommended**: Option A for accuracy

**Request**:
```rust
pub struct SemanticSearchRequest {
    pub query: String,           // "authentication", "error handling"
    pub path: Option<String>,
    pub limit: Option<usize>,    // Default 20
    pub include_source: Option<bool>,
}
```

**Response structure**:
```yaml
_type: semantic_search_results
query: "authentication"
results[20]:
  - symbol: "validate_token"
    file: "src/auth/validate.rs"
    lines: "45-78"
    score: 0.89
    snippet: "/// Validates JWT token..."  # If include_source
    context_terms: ["token", "jwt", "session"]
  ...
related_queries: ["login", "session", "token", "credentials"]
```

**Files to create/modify**:
- `src/bm25.rs` (NEW) - BM25 implementation
- `src/shard.rs` - Add BM25 index generation
- `src/mcp_server/mod.rs` - Add tool handler
- `src/mcp_server/types.rs` - Add request/response types

**BM25 Implementation sketch**:
```rust
pub struct Bm25Index {
    // term -> [(doc_id, term_freq)]
    inverted_index: HashMap<String, Vec<(String, u32)>>,
    // doc_id -> doc_length
    doc_lengths: HashMap<String, u32>,
    avg_doc_length: f64,
    total_docs: u32,
}

impl Bm25Index {
    pub fn search(&self, query: &str, k: usize) -> Vec<(String, f64)> {
        // Standard BM25 with k1=1.2, b=0.75
    }
}
```

---

## Phase 4: Code Quality Validation Tool

### New Tool: `validate_symbol`

**Purpose**: Post-analysis quality check for a symbol. Useful for agents to verify code they're reviewing or after the user has made changes.

**Leverages existing functionality**:
- `calculate_complexity()` in `src/detectors/generic.rs:769`
- `find_duplicates` (already fast on massive codebases)
- `get_callers` for impact analysis

**Request**:
```rust
pub struct ValidateSymbolRequest {
    pub symbol_hash: Option<String>,  // Lookup by hash
    pub file_path: Option<String>,    // Or by file + line
    pub line: Option<usize>,
    pub path: Option<String>,         // Repo path
}
```

**Response structure**:
```yaml
_type: validation_result
symbol: "handle_request"
file: "src/server/handler.rs"
lines: "45-120"

complexity:
  cognitive: 12
  cyclomatic: 8
  max_nesting: 4
  risk: "medium"

duplicates:  # From find_duplicates, filtered to this symbol
  - symbol: "process_request"
    file: "src/api/processor.rs"
    similarity: 0.87

callers:  # Impact radius
  direct: 5
  transitive: 12
  high_risk_callers: ["main", "handle_connection"]

suggestions:
  - "Cognitive complexity 12 exceeds recommended threshold of 10"
  - "87% similar to process_request - consider consolidation"
```

**Files to modify**:
- `src/mcp_server/mod.rs` - Add tool handler
- `src/mcp_server/types.rs` - Add request/response types
- `src/risk.rs` - Expose complexity calculation as public API

---

## Phase 5: Automatic Partial Reindexing

### Enhancement: Transparent staleness handling

**Purpose**: Eliminate stale index issues without manual `check_index` calls.

### Decision: Smart Staleness Detection (No Daemon Required)

**Competitor Research Summary** (Dec 2024):
- **Cursor**: Merkle trees + 10-minute sync intervals, integrated in IDE
- **Sourcegraph**: LSIF pre-computed indexes, WebSocket connections
- **rust-analyzer**: Persistent compilation in-process, no daemon
- **Code-Index-MCP**: File watcher via watchdog library within MCP process
- **Claude Context MCP**: Merkle trees, user-initiated, stdio transport

**Key Finding**: No major tool requires a separate daemon. File watching is always integrated.

**Rejected Approaches**:
| Option | Why Rejected |
|--------|--------------|
| Daemon proxy | User friction ("start the daemon"), resource overhead |
| Hybrid daemon/MCP | Over-engineered, two code paths to maintain |
| HTTP transport mode | Adds complexity, most MCP clients expect stdio |

### Final Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    MCP Tool Request                         │
│              (search_symbols, get_repo_overview, etc.)      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Quick Staleness Check (~5ms)                │
│   1. Compare indexed_sha vs current git HEAD                │
│   2. Check for uncommitted changes via git status           │
│   3. Compare index mtime vs source files (sampling)         │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
        [Fresh Index]                  [Stale Index]
              │                               │
              ▼                               ▼
     Execute Query                  ┌─────────────────────────┐
                                    │  Determine Strategy      │
                                    │  - <50 files: partial    │
                                    │  - >50 files: full       │
                                    └─────────────────────────┘
                                              │
                                              ▼
                                    ┌─────────────────────────┐
                                    │  Partial Reindex        │
                                    │  (changed files only)   │
                                    └─────────────────────────┘
                                              │
                                              ▼
                                       Execute Query
                                    + Include freshness note
```

### Leveraging Existing Code

| Existing Code | Location | Reuse For |
|---------------|----------|-----------|
| `DriftDetector::check_drift()` | `src/drift.rs:197` | Git SHA staleness detection |
| `LayerSynchronizer::update_layer()` | `src/server/sync.rs:1` | Incremental update logic |
| `check_cache_staleness_detailed()` | `src/mcp_server/helpers.rs:102` | File mtime comparison |
| `generate_index_internal()` | `src/mcp_server/helpers.rs:351` | Full index generation |
| `UpdateStrategy` enum | `src/drift.rs:197` | Fresh/Incremental/Rebase/FullRebuild |

### New Implementation

**1. `ensure_fresh_index()` in `src/mcp_server/helpers.rs`**
```rust
pub struct FreshnessResult {
    pub cache: CacheDir,
    pub refreshed: bool,
    pub files_updated: usize,
    pub duration_ms: u64,
}

/// Ensures index is fresh before query execution.
/// Called at the start of all query tools.
pub fn ensure_fresh_index(
    repo_path: &Path,
    max_stale_files: usize,  // Default 50
) -> Result<FreshnessResult, String> {
    // 1. Quick staleness check
    // 2. If stale with <=max_stale_files: partial reindex
    // 3. If stale with >max_stale_files: full reindex
    // 4. Return cache + freshness note
}
```

**2. `partial_reindex()` in `src/shard.rs`**
```rust
/// Incrementally update index for changed files only.
/// Preserves unchanged symbols, updates affected module shards.
pub fn partial_reindex(
    cache: &CacheDir,
    changed_files: &[PathBuf],
) -> Result<PartialReindexStats, McpDiffError> {
    // 1. Parse changed files
    // 2. Identify affected modules
    // 3. Load existing module shards
    // 4. Replace/add symbols from changed files
    // 5. Write updated module shards
    // 6. Update repo_overview stats
}
```

**3. `quick_staleness_check()` in `src/cache.rs`**
```rust
pub struct QuickStalenessResult {
    pub is_stale: bool,
    pub indexed_sha: Option<String>,
    pub current_sha: String,
    pub changed_files: Vec<PathBuf>,
}

/// Fast staleness check (~5ms) using git + sampling.
pub fn quick_staleness_check(&self) -> QuickStalenessResult {
    // 1. git rev-parse HEAD vs stored indexed_sha
    // 2. git status --porcelain for uncommitted changes
    // 3. Sample file mtimes (already in check_cache_staleness_detailed)
}
```

### Performance Targets

| Operation | Target | Implementation |
|-----------|--------|----------------|
| Staleness check | <5ms | Git SHA comparison + sampled mtimes |
| Partial reindex (1-10 files) | <100ms | Incremental parsing, shard patching |
| Partial reindex (10-50 files) | <500ms | Parallel analysis via rayon |
| Full reindex trigger | Only if >50 files | Fallback to generate_index_internal |

### User Experience

**Before (current):**
```
Agent: Let me check the index status first...
[calls check_index]
Agent: The index is stale. Let me regenerate it...
[calls generate_index - takes 5s]
Agent: Now I can search...
[calls search_symbols]
```

**After (proposed):**
```
Agent: Let me search for authentication code...
[calls search_symbols - auto-refreshes if needed]
Result includes: "⚡ Index refreshed (3 files updated in 45ms)"
```

### Files to Modify

1. **`src/cache.rs`** - Add `quick_staleness_check()` method
2. **`src/shard.rs`** - Add `partial_reindex()` function
3. **`src/mcp_server/helpers.rs`** - Add `ensure_fresh_index()` wrapper
4. **`src/mcp_server/mod.rs`** - Call `ensure_fresh_index()` in query tools:
   - `search_symbols`
   - `get_repo_overview`
   - `list_symbols`
   - `get_module`
   - `get_symbol`
   - `get_call_graph`

### Open Design Decisions

1. **Opt-out parameter?** Add `auto_refresh: Option<bool>` to disable for performance?
2. **Threshold tuning**: Is 50 files the right cutoff for partial vs full reindex?
3. **Response format**: Include freshness note in TOON or as separate field?

---

## Phase 6: Improved Tool Documentation for AI

### Enhancement: MCP tool descriptions optimized for agent understanding

**Current problem**: Tool descriptions don't tell agents WHEN to use them.

**Proposed improvements**:

| Tool | Current Description | Improved Description |
|------|--------------------|--------------------|
| `check_duplicates` | "Check if a specific function has duplicates..." | "**Use before writing new functions** to avoid duplication. Returns similar existing functions. Also useful for refactoring to find consolidation candidates." |
| `get_callers` | "Get callers of a symbol..." | "**Use before modifying existing code** to understand impact radius. Shows what will break if you change this function." |
| `find_duplicates` | "Find all duplicate function clusters..." | "Find code duplication across the entire codebase. Fast even on massive repos. Use for codebase health audits or before major refactoring." |
| `get_call_graph` | "Get the call graph..." | "Understand code flow and dependencies. **Use with filters** (module, symbol) for targeted analysis. Unfiltered output can be large." |

**Files to modify**:
- `src/mcp_server/mod.rs` - Update `#[tool(description = "...")]` attributes

---

## Implementation Order

| Phase | Effort | Impact | Priority |
|-------|--------|--------|----------|
| 1. `get_context` | Low | High | P0 |
| 2. Repo summary improvements | Low | High | P0 |
| 6. Tool documentation | Low | Medium | P0 |
| 5. Auto partial reindex | Medium | High | P1 |
| 4. `validate_symbol` | Medium | Medium | P1 |
| 3. BM25 semantic search | High | High | P2 |

---

## Success Metrics

1. **Token reduction**: First-contact context from 5000+ to <500 tokens
2. **Staleness elimination**: Zero manual `check_index` calls needed
3. **Quality adoption**: Agents use `validate_symbol` after code analysis
4. **Search relevance**: BM25 finds related code that exact match misses

---

## Open Questions

1. Should `get_context` be a separate tool or merged into `get_repo_summary`?
2. BM25 index size estimate for large repos - acceptable?
3. Should partial reindex be opt-out via parameter?
4. `validate_symbol` - should it auto-run `find_duplicates` or require explicit call?
