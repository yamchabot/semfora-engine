# Semfora Engine Features

This document covers the advanced features of the Semfora semantic analysis engine.

## Incremental Indexing

Semfora uses SHA-based drift detection to intelligently update indexes, avoiding unnecessary full rebuilds.

### How It Works

**Key insight**: Time-based staleness is meaningless. An old project with the same SHA is fresh (nothing changed), while a 5-minute break with half the app rewritten is stale (everything changed).

The engine compares git SHAs to detect what changed:

```
Indexed SHA: abc123 (what we analyzed)
Current SHA: def456 (what's on disk now)
              ↓
         Git diff → Changed files list
```

### Update Strategies

Based on drift magnitude, the engine selects the optimal strategy:

| Files Changed | Strategy | Action |
|---------------|----------|--------|
| 0 | Fresh | No action needed |
| < 10 | Incremental | Reparse only changed files |
| < 30% of repo | Rebase | Reconcile overlay with new base |
| >= 30% of repo | Full Rebuild | Discard and recreate index |

### Performance Targets

- **Single file change**: < 500ms (typically < 50ms with AST caching)
- **Incremental update (< 10 files)**: 10x faster than full rebuild

### Tree-sitter Incremental Parsing

The daemon uses tree-sitter's incremental parsing feature for dramatic performance improvements when files are edited.

**How it works:**
1. Cache the source code and parsed AST for each file
2. When a file changes, compute an `InputEdit` describing what changed
3. Call `tree.edit(&edit)` to adjust node byte ranges
4. Parse with `parser.parse(new_source, Some(&old_tree))`
5. Tree-sitter reuses unchanged subtrees, only reparsing affected regions

**Benchmark Results (December 2025):**

| Scenario | Full Parse | Incremental | Speedup |
|----------|------------|-------------|---------|
| Small file (~700 bytes) | 74-89 µs | 20-25 µs | **3.5x** |
| Large file (~49KB) | 6.7-8.0 ms | 263-314 µs | **25x** |
| Cache hit (no changes) | - | 96-99 ns | **760x** |

The AST cache is especially effective for:
- Rapid save-on-type editors (VSCode, Cursor)
- Large TypeScript/React components
- Files edited multiple times in sequence

**Memory overhead:** ~2-5KB per cached file (source + tree structure)

### CLI Usage

```bash
# Initial index
semfora-engine --dir . --shard

# Incremental update (only changed files)
semfora-engine --dir . --shard --incremental

# Force full rebuild
semfora-engine --dir . --shard  # Without --incremental
```

### MCP Usage

```json
{
  "name": "generate_index",
  "arguments": {
    "path": "/path/to/project",
    "force": false
  }
}
```

The `check_index` tool reports staleness and can auto-refresh:

```json
{
  "name": "check_index",
  "arguments": {
    "path": "/path/to/project",
    "auto_refresh": true
  }
}
```

---

## Layered Index System

Semfora maintains a 4-layer index stack that mirrors git's state model:

```
Layer 3: AI PROPOSED      ← In-memory changes (not yet on disk)
    ↓
Layer 2: WORKING          ← Uncommitted changes (staged + unstaged)
    ↓
Layer 1: BRANCH           ← Commits since diverging from base
    ↓
Layer 0: BASE             ← main/master branch (full sharded index)
```

### Query Resolution

When looking up a symbol, layers are checked top-down (AI → Working → Branch → Base). First match wins. A `Deleted` marker stops the search and returns None.

This means:
- Local changes shadow committed code
- Uncommitted work is always visible
- AI-proposed changes can be previewed before writing to disk

### Layer-Specific Staleness

Each layer has different staleness triggers:

| Layer | Stale When |
|-------|------------|
| Base | HEAD of base branch moved |
| Branch | HEAD moved OR merge-base changed (rebase) |
| Working | Any tracked file modified (mtime check) |
| AI | Never (ephemeral, managed in-memory) |

### Merge-Base Detection

For branch layers, the engine tracks the merge-base SHA. If it changes (indicating a rebase or merge from upstream), the branch layer triggers a rebase operation to reconcile changes.

---

## SHA-Based Drift Detection

The drift detection system (`src/drift.rs`) provides detailed staleness information:

```rust
pub struct DriftStatus {
    pub is_stale: bool,
    pub indexed_sha: Option<String>,
    pub current_sha: Option<String>,
    pub changed_files: Vec<PathBuf>,
    pub drift_percentage: f64,
    pub merge_base_changed: bool,
}
```

### Checking Drift Programmatically

```rust
let detector = DriftDetector::new(repo_root);
let drift = detector.check_drift(LayerKind::Base, Some(&indexed_sha), None)?;

match drift.strategy(total_files) {
    UpdateStrategy::Fresh => println!("Up to date!"),
    UpdateStrategy::Incremental(files) => println!("Update {} files", files.len()),
    UpdateStrategy::Rebase => println!("Rebase needed"),
    UpdateStrategy::FullRebuild => println!("Full rebuild required"),
}
```

---

## Behavioral Risk Assessment

Every symbol is assigned a risk level based on its characteristics:

### Risk Scoring

| Factor | Points |
|--------|--------|
| New import | +1 (capped at 3) |
| State variable | +1 per variable |
| Control flow (if/for/match) | +1 base, +1 if >5, +1 if >15 |
| I/O or network calls | +2 |
| Public API changes | +3 |
| Persistence operations | +3 |

### Risk Levels

| Level | Score | Meaning |
|-------|-------|---------|
| Low | 0-2 | Simple, low-impact code |
| Medium | 3-4 | Moderate complexity or state |
| High | 5+ | Complex logic, I/O, or public API changes |

### Filtering by Risk

```bash
# CLI: Find high-risk symbols
semfora-engine --search-symbols "handle" --risk high

# MCP tool
{ "name": "search_symbols", "arguments": { "query": "handle", "risk": "high" } }
```

---

## Live Index Updates

When running the MCP server or WebSocket daemon, indexes stay fresh automatically:

### FileWatcher

Monitors the filesystem for changes and triggers incremental updates:
- Uses `notify` crate for cross-platform file watching
- Debounces rapid changes (500ms window)
- Updates Working layer in real-time

### GitPoller

Polls for git state changes:
- Detects new commits (Base/Branch layer updates)
- Detects branch switches
- Detects rebases (merge-base changes)

### Event Broadcasting

File changes emit events that clients can subscribe to:

```json
{
  "type": "event",
  "name": "layer_updated:working",
  "payload": {
    "files_changed": 3,
    "symbols_updated": 15
  }
}
```

---

## Test Runner Integration

Semfora includes a built-in test runner that auto-detects frameworks:

### Supported Frameworks

| Framework | Detection |
|-----------|-----------|
| pytest | `pytest.ini`, `pyproject.toml`, `conftest.py` |
| cargo test | `Cargo.toml` |
| npm test | `package.json` with test script |
| vitest | `vitest.config.*` |
| jest | `jest.config.*` |
| go test | `go.mod` |

### CLI Usage

```bash
# Auto-detect and run tests
semfora-engine --run-tests

# Filter tests by pattern
semfora-engine --run-tests --filter "auth"
```

### MCP Usage

```json
{
  "name": "run_tests",
  "arguments": {
    "path": "/path/to/project",
    "filter": "test_auth",
    "verbose": true
  }
}
```

Returns structured results:
```json
{
  "framework": "pytest",
  "passed": 42,
  "failed": 2,
  "skipped": 1,
  "duration_ms": 3500,
  "failures": [...]
}
```

---

## Ripgrep Fallback

When no semantic index exists, symbol search falls back to ripgrep for text-based search:

```json
{
  "name": "raw_search",
  "arguments": {
    "pattern": "authenticate.*user",
    "file_types": ["rs", "ts"],
    "limit": 50
  }
}
```

This ensures search always works, even before indexing.

---

## TOON Output Format

TOON (Token-Oriented Object Notation) is a compressed format optimized for AI token efficiency:

### Compression Ratios

| Content | JSON | TOON | Savings |
|---------|------|------|---------|
| Symbol list | 2,400 tokens | 800 tokens | 67% |
| Call graph | 5,000 tokens | 1,500 tokens | 70% |
| Full repo overview | 15,000 tokens | 4,000 tokens | 73% |

### Format Selection

```bash
# TOON (default)
semfora-engine file.rs --format toon

# JSON
semfora-engine file.rs --format json
```

### Token Analysis

Compare token efficiency:

```bash
semfora-engine file.rs --analyze-tokens full --compare-compact
```

---

## Call Graph Analysis

The engine builds and maintains a call graph showing function relationships:

### Queries

```json
// What does this function call?
{ "name": "get_symbol_callees", "arguments": { "hash": "abc123" } }

// What calls this function?
{ "name": "get_symbol_callers", "arguments": { "hash": "abc123" } }

// Bidirectional graph centered on a symbol
{
  "name": "get_call_graph_for_symbol",
  "arguments": { "hash": "abc123", "depth": 2 }
}
```

### Graph Regeneration

After incremental updates, graphs are automatically regenerated to stay in sync with the symbol index.

---

## Cache Structure

Indexes are stored in `~/.cache/semfora-engine/{repo_hash}/`:

```
~/.cache/semfora-engine/abc123/
├── repo_overview.toon      # High-level architecture summary
├── modules/                # Per-module symbol data
│   ├── api.toon
│   ├── components.toon
│   └── lib.toon
├── symbols/                # Individual symbol details
│   ├── def456.toon
│   └── ghi789.toon
├── symbol_index.jsonl      # Symbol lookup index (JSON Lines)
├── call_graph.json         # Function call relationships
└── layers/                 # Layered index data
    ├── base/
    ├── branch/
    └── working/
```

### Cache Management

```bash
# Show cache info
semfora-engine --cache-info

# Clear cache for current directory
semfora-engine --cache-clear

# Prune old caches
semfora-engine --cache-prune 30  # Remove caches older than 30 days
```

---

## See Also

- [CLI Reference](cli.md) - Complete command-line documentation
- [WebSocket Daemon](websocket-daemon.md) - Real-time updates and multi-client support
- [Main README](../README.md) - Supported languages and quick start
