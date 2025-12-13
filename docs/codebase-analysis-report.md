# Semfora Engine Codebase Analysis Report

**Generated**: December 13, 2025
**Codebase**: semfora-engine
**Analysis Tool**: semfora-engine v0.1.0 (self-analysis)

---

## Executive Summary

| Metric | Value |
|--------|-------|
| **Total Files Analyzed** | 196 |
| **Total Modules** | 32 |
| **Total Symbols** | 1,571 (index) / 1,480 (analysis) |
| **Total Lines of Code** | 41,429 |
| **Compression Ratio** | 62.9% |
| **Indexing Time** | 1.267 seconds |
| **Duplicate Clusters** | 66 clusters (140 duplicate functions) |
| **Cognitive Complexity Avg** | 3.3 |

---

## 1. Indexing Performance

| Metric | Value |
|--------|-------|
| Files Processed | 190 |
| Source Size | 11.5 MB |
| Index Time | 1.267 seconds |
| Throughput | ~150 files/second |
| Compression | 62.9% token reduction |

The sharded index system provides efficient caching with module-level granularity for incremental updates.

---

## 2. Cognitive Complexity Analysis

### Top 15 Most Complex Functions

| Rank | Function | Cognitive | Nesting | LOC | Fan-Out | File |
|------|----------|-----------|---------|-----|---------|------|
| 1 | `build_call_graph_from_summaries` | 100 | 5 | 155 | 24 | cache.rs |
| 2 | `build_call_graph` | 87 | 5 | 97 | 20 | shard.rs |
| 3 | `load_module_summaries` | 66 | 6 | 105 | 27 | cache.rs |
| 4 | `collect_symbol_candidates` | 66 | 6 | 113 | 16 | detectors |
| 5 | `handle_query` | 61 | 2 | 356 | 0 | socket_server |
| 6 | `from_summaries` | 55 | 2 | 172 | 13 | cache.rs |
| 7 | `collect_files_recursive` | 54 | 7 | 64 | 18 | mcp_server |
| 8 | `start_with_cache` | 51 | 7 | 145 | 0 | server |
| 9 | `extract_candidate_from_decl` | 50 | 5 | 130 | 0 | detectors |
| 10 | `collect_files_recursive` | 48 | 7 | 60 | 16 | socket_server |
| 11 | `extract` | 44 | 2 | 80 | 41 | extract.rs |
| 12 | `run_shard` | 44 | 3 | 203 | 0 | main.rs |
| 13 | `load_layer` | 43 | 2 | 90 | 21 | cache.rs |
| 14 | `search_symbols` | 42 | 3 | 85 | 19 | overlay.rs |
| 15 | `run` | 40 | 3 | 126 | 0 | main.rs |

**Complexity Scale:**
- 0-5: Simple
- 6-10: Moderate
- 11-20: Complex
- 21+: Very Complex

### Refactoring Candidates

Functions with cognitive complexity > 50 should be considered for refactoring:
1. **`build_call_graph_from_summaries`** (100) - Extract helper functions for symbol lookup and edge building
2. **`build_call_graph`** (87) - Similar to above, split graph construction logic
3. **`load_module_summaries`** (66) - Separate file I/O from parsing logic
4. **`collect_symbol_candidates`** (66) - Extract visitor pattern or iterative approach

---

## 3. Lines of Code Analysis

### Highest LOC Functions

| Rank | Function | LOC | File |
|------|----------|-----|------|
| 1 | `get_templates` | 6,642 | benchmark_builder/templates.rs |
| 2 | `handle_query` | 356 | socket_server |
| 3 | `run_shard` | 203 | main.rs |
| 4 | `from_summaries` | 172 | cache.rs |
| 5 | `build_call_graph_from_summaries` | 155 | cache.rs |
| 6 | `start_with_cache` | 145 | server |
| 7 | `extract_candidate_from_decl` | 130 | detectors |
| 8 | `run` | 126 | main.rs |
| 9 | `collect_symbol_candidates` | 113 | detectors |
| 10 | `load_module_summaries` | 105 | cache.rs |

**Note:** `get_templates` at 6,642 LOC contains embedded template strings for the benchmark builder - this is by design, not a refactoring candidate.

### Module Metrics by LOC

| Module | Symbols | Total LOC | Avg CC | Max CC |
|--------|---------|-----------|--------|--------|
| cache | 144 | 3,755 | 3.7 | 100 |
| detectors | 113 | 2,500 | 3.8 | 31 |
| mcp_server | 82 | 2,192 | 4.8 | 34 |
| detectors.javascript | 100 | 2,007 | 4.0 | 32 |
| socket_server | 68 | 1,731 | 4.2 | 61 |
| scripts | 46 | 1,641 | 5.3 | 22 |
| root | 33 | 1,510 | 10.0 | 48 |
| benches | 48 | 1,477 | 5.5 | 24 |
| detectors.javascript.core | 37 | 1,231 | 8.3 | 66 |
| toon | 26 | 1,220 | 8.5 | 34 |
| shard | 31 | 1,023 | 10.6 | 87 |

---

## 4. Duplicate Function Detection

**Configuration:**
- Threshold: 90%
- Boilerplate Excluded: Yes
- Total Signatures Analyzed: 1,295
- Duplicate Clusters Found: 66
- Total Duplicate Functions: 140

### Exact Duplicates (100% Match) - HIGH PRIORITY

These are identical implementations that should be consolidated:

| Function | Locations | Action |
|----------|-----------|--------|
| `truncate_to_char_boundary` | `common.rs`, `toon.rs`, `extract.rs` | Consolidate to single utility |
| `extract_filename_stem` | `python.rs`, `javascript/core.rs` | Move to shared module |
| `parse_source` | `javascript/core.rs`, `generic.rs` | Extract to common parser |
| `collect_files` | `main.rs`, `mcp_server/helpers.rs` | Deduplicate |
| `default` (Default impl) | 5 locations | Consider derive macro |

### Near Duplicates (90-97% Match)

| Cluster | Primary | Duplicates | Similarity |
|---------|---------|------------|------------|
| 1 | `parse_and_extract_string` | `parse_and_extract` | 95% |
| 2 | `get_changed_files` | `get_commit_changed_files` | 95% |
| 3 | `test_load_layer_corrupted_symbols` | 5 similar test functions | 90-94% |
| 4 | `call_graph_path` | `import_graph_path`, `module_graph_path` | 90% |
| 5 | `layer_symbols_path` | `layer_deleted_path`, `layer_moves_path` | 90% |
| 6 | `test_compute_edit_insert` | 3 similar edit tests | 92% |

### Divergent Duplicates (80-89% Match)

These share common patterns but have evolved differently:

| Pattern | Count | Examples |
|---------|-------|----------|
| Framework detection (`is_*`) | 10 | `is_entry_point`, `is_component`, `is_service` |
| Git operations (`get_*`) | 6 | `get_current_branch`, `get_merge_base`, etc. |
| Test assertions | 15+ | Similar test structure with different data |
| Layer operations | 6 | `with_boilerplate_config`, `with_limit`, etc. |

### Full Duplicate Cluster List (66 Clusters)

<details>
<summary>Click to expand all 66 clusters</summary>

#### Cluster 1: AnimatedEdge Components
- **Primary**: `AnimatedEdge` (benchmark-visualizer)
- **Duplicates**: `StaticEdge` (87%), `NewEdge` (87%)

#### Cluster 2: Script Utilities
- **Primary**: `run_cmd` (realworld-test.py)
- **Duplicates**: `clear_cache` (80%)

#### Cluster 3: Print Functions
- **Primary**: `print_status` (realworld-test.py)
- **Duplicates**: `print_progress` (87%)

#### Cluster 4: Benchmark Functions
- **Primary**: `benchmark_get_overview`
- **Duplicates**: `benchmark_get_call_graph` (88%)

#### Cluster 5: Filename Extraction (EXACT)
- **Primary**: `extract_filename_stem` (python.rs)
- **Duplicates**: `extract_filename_stem` (javascript/core.rs) - 100%

#### Cluster 6: Score Calculation
- **Primary**: `calculate_basic_score` (python.rs)
- **Duplicates**: `calculate_symbol_score` (go.rs) - 90%

#### Cluster 7-8: HCL Tests
- 4 similar test functions for HCL parsing

#### Cluster 9: String Truncation (EXACT)
- **Primary**: `truncate_to_char_boundary` (common.rs)
- **Duplicates**: Same function in `toon.rs`, `extract.rs` - 100%

#### Cluster 10: Source Parsing (EXACT)
- **Primary**: `parse_source` (javascript/core.rs)
- **Duplicates**: `parse_source` (generic.rs) - 100%

#### Cluster 11-12: JavaScript Tests
- Call attribution tests with minor variations

#### Cluster 13: Framework Detection Pattern (10 duplicates)
- `is_entry_point`, `is_middleware_file`, `is_vue_sfc`, `is_component` (Vue/Angular), `is_service`, `is_module`, `is_directive`, `is_pipe`, `is_composable`, `is_pinia_store`
- All follow same pattern: `source.contains("pattern")`

#### Cluster 14: Feature Detection
- `detect_from_source` with 4 similar functions

#### Cluster 15: Vue SFC Tests
- 4 test functions for Vue SFC extraction (93% similar)

#### Cluster 16: Export Detection
- `c_is_exported` / `kotlin_is_exported` (90%)

#### Cluster 17-19: Default/New Implementations
- Multiple simple constructors with similar patterns

#### Cluster 20-22: Search Tests
- Test functions for file patterns and language filters

#### Cluster 23: File Collection (EXACT + NEAR)
- `collect_files` (main.rs) = `collect_files` (helpers.rs) - 100%
- `collect_source_files` (indexer.rs) - 93%

#### Cluster 24: Parse Functions
- `parse_and_extract_string` / `parse_and_extract` (95%)

#### Cluster 25-30: Cache Path Functions
- Multiple `*_path` functions returning cache paths

#### Cluster 31-34: Git Operations
- `get_current_branch`, `get_merge_base`, `get_parent_commit`, etc.

#### Cluster 35-38: Test Functions
- Multiple test clusters with similar assertion patterns

#### Cluster 39: Default Trait (EXACT)
- `default()` implementation in 5 files - 100%

#### Cluster 40-66: Remaining Clusters
- Mix of test functions, overlay operations, and utility functions

</details>

---

## 5. Call Graph Analysis

### Entry Points (32 symbols)
Functions that are called from external sources or serve as module entry points.

### Leaf Functions (121 symbols)
Functions that don't call any other internal functions - potential candidates for utility extraction.

### Most Called Functions (Hotspots)

| Function | Callers | Notes |
|----------|---------|-------|
| `Ok` | 106 | Rust Result wrapper |
| `Some` | 105 | Rust Option wrapper |
| `Vec::new` | 77 | Collection initialization |
| `PathBuf::from` | 65 | Path construction |
| `LayeredIndex::new` | 42 | Index creation |
| `make_test_symbol` | 42 | Test helper |
| `Default::default` | 41 | Default trait |
| `TempDir::new` | 38 | Test directories |

### High Coupling (Most Outgoing Calls)

| Symbol | Outgoing Calls | Risk |
|--------|----------------|------|
| `CacheMeta` | 83 | High - consider splitting |
| `McpDiffServer` | 80 | High - facade pattern |
| `main` | 68 | Expected for entry point |
| `DetectionResult` | 48 | Moderate |
| `BenchmarkResult` | 48 | Moderate |

### Circular Dependencies Detected

5 circular dependency chains found in the call graph:

```
6b99d11b7375e677 → 82354bc187b12b66 → 6b99d11b7375e677
b880e2883646d4ea → 6b99d11b7375e677 → b880e2883646d4ea
6b99d11b7375e677 → 29205f009c7a6e4a → 6b99d11b7375e677
6b99d11b7375e677 → 5c084015448bd950 → 6b99d11b7375e677
6b99d11b7375e677 → 2381062eef1335bb → 6b99d11b7375e677
```

**Note**: These are in Python scripts (`scripts/`) and represent intentional recursive patterns, not problematic Rust dependencies.

---

## 6. Potentially Unused Code

Based on call graph analysis, the following patterns suggest potential dead code:

### Leaf Functions Not Called from Entry Points

The analysis shows 121 leaf functions. Functions that are:
1. Not test functions (`test_*`)
2. Not trait implementations
3. Not called by any other function

Should be reviewed for removal. Run `--get-call-graph` and cross-reference with entry points to identify specific candidates.

### Unused Exports

The `detectors/` module contains several language-specific functions that may not be invoked for all file types. This is expected behavior for a multi-language analyzer.

---

## 7. Recommendations

### Immediate Actions (High Priority)

1. **Consolidate Exact Duplicates**
   - Move `truncate_to_char_boundary` to a shared `utils` module
   - Unify `extract_filename_stem` implementations
   - Deduplicate `collect_files` between main.rs and helpers.rs

2. **Refactor High-Complexity Functions**
   - `build_call_graph_from_summaries` (CC: 100) - Split into smaller functions
   - `build_call_graph` (CC: 87) - Extract graph building logic
   - `load_module_summaries` (CC: 66) - Separate I/O from parsing

### Medium Priority

3. **Extract Common Patterns**
   - Create trait for `is_*` framework detection functions
   - Consolidate git operation functions into trait with shared implementation
   - Unify cache path methods

4. **Test Deduplication**
   - Consider parameterized tests for similar test clusters
   - Use test fixtures for common setup patterns

### Future Improvements

5. **Reduce High Coupling**
   - Consider splitting `CacheMeta` responsibilities
   - Apply facade pattern more consistently

6. **Documentation**
   - Add module-level documentation for complex modules
   - Document circular dependency reasons in scripts

---

## 8. Appendix: Raw Metrics

### Index Statistics
```
Files analyzed: 196
Modules: 32
Symbols: 1,571
Compression: 62.9%
Index time: 1.267s
```

### Language Distribution
| Language | Files | Symbols |
|----------|-------|---------|
| Rust | 150+ | 1,200+ |
| TypeScript/TSX | 15+ | 150+ |
| Python | 10+ | 100+ |
| Shell | 5+ | 50+ |

### Risk Distribution
- High Risk: ~50 symbols
- Medium Risk: ~200 symbols
- Low Risk: ~1,300 symbols

---

*Report generated by semfora-engine's self-analysis capabilities.*
