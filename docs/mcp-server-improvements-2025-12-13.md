# MCP Server Improvement Recommendations

**Date:** December 13, 2025
**Based on:** Real-world audit of daggerfall-unity codebase (1,076 files, 13,451 symbols)

---

## Executive Summary

After using the semfora-engine MCP server to audit a large Unity game codebase, several usability issues and improvement opportunities were identified. The primary concerns are:

1. **Output truncation** on large result sets with no pagination
2. **Search functionality gaps** (wildcards don't work as expected)
3. **Documentation gaps** around token budgets and risk calibration
4. **Missing convenience functions** for common analysis patterns

---

## Critical Issues

### 1. Output Truncation on Large Result Sets

**Problem:** Both `get_call_graph` and `find_duplicates` exceeded the 25K token limit and were truncated, making the data incomplete and unusable.

**Impact:** High - Users cannot get complete analysis for medium-to-large codebases.

**Test Case:**
```
Repository: daggerfall-unity
Files: 1,076
Call graph edges: 6,913
Result: [OUTPUT TRUNCATED - exceeded 25000 token limit]
```

**Recommended Solution:**

Add pagination and filtering parameters to all list operations:

```rust
// get_call_graph improvements
pub struct GetCallGraphRequest {
    pub path: Option<String>,
    // NEW PARAMETERS:
    pub module: Option<String>,      // Filter to specific module
    pub symbol: Option<String>,      // Filter to calls from/to specific symbol
    pub depth: Option<u32>,          // Limit call depth (1 = direct calls only)
    pub limit: Option<u32>,          // Max edges to return (default: 100)
    pub offset: Option<u32>,         // Pagination offset
    pub summary_only: Option<bool>,  // Return stats only, no edge list
}

// find_duplicates improvements
pub struct FindDuplicatesRequest {
    pub path: Option<String>,
    pub threshold: Option<f64>,
    pub exclude_boilerplate: Option<bool>,
    pub module: Option<String>,      // ALREADY EXISTS - Filter to specific module
    // NEW PARAMETERS:
    pub min_lines: Option<u32>,      // Ignore functions smaller than N lines
    pub limit: Option<u32>,          // Max clusters to return (default: 50)
    pub offset: Option<u32>,         // Pagination offset
    pub sort_by: Option<String>,     // "size" | "similarity" | "count"
}
```

---

### 2. Search Wildcards Don't Work

**Problem:** `search_symbols(query: "*", risk: "high")` returned 0 results despite there being 1,024 high-risk files.

**Impact:** Medium - Users cannot easily find all symbols matching a filter criterion.

**Test Case:**
```
search_symbols(query="*", risk="high", limit=30)
Result: 0 results
Expected: Up to 30 high-risk symbols
```

**Recommended Solutions:**

Option A: Fix wildcard support
```rust
// In search_symbols implementation
if query == "*" || query.is_empty() {
    // Return all symbols (with other filters applied)
}
```

Option B: Add dedicated filter functions
```rust
// New tool: list_symbols_by_risk
pub struct ListSymbolsByRiskRequest {
    pub path: Option<String>,
    pub risk: String,           // "high" | "medium" | "low"
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
```

Option C: Document supported query syntax
```markdown
## search_symbols Query Syntax

- Simple text: `login` - matches symbols containing "login"
- Exact match: `"LoginHandler"` - matches exact symbol name
- Prefix: `get*` - NOT SUPPORTED, use `get` instead
- Wildcard: `*` - NOT SUPPORTED, use list_symbols instead

To list all symbols with a filter, use `list_symbols(module, risk="high")`.
```

---

### 3. Module Path Inconsistency

**Problem:** Module names have inconsistent formatting - some use absolute paths, some relative:

```
# Relative (good):
test-repos.daggerfall-unity.Assets.Scripts.Game

# Absolute (bad - leaks system info):
.home.kadajett.Dev.Semfora_org.semfora-engine.test-repos.daggerfall-unity.Assets.Scripts.Game
```

**Impact:** Low-Medium - Makes module filtering unreliable and leaks filesystem info.

**Recommended Solution:**

Normalize all paths relative to repository root during indexing:

```rust
fn normalize_module_path(absolute_path: &Path, repo_root: &Path) -> String {
    let relative = absolute_path
        .strip_prefix(repo_root)
        .unwrap_or(absolute_path);

    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join(".")
}
```

---

## Documentation Improvements

### 4. Clarify Compression Ratio Meaning

**Problem:** The output showed `-28.0%` compression, which is confusing. Does negative mean the semantic data is larger than source? Is that bad?

**Current Output:**
```
Compression: -28.0%
```

**Recommended Documentation Addition:**

```markdown
## Understanding Compression Ratio

The compression ratio indicates how the semantic summary size compares to raw source code:

| Ratio | Meaning | Typical For |
|-------|---------|-------------|
| +80% | Summary is 80% smaller than source | Large files with few symbols |
| +50% | Summary is half the size | Average codebase |
| 0% | Same size | Dense code |
| -28% | Summary is 28% larger | Complex codebases with many symbols, calls, state |
| -50% | Summary is 50% larger | Very dense game/enterprise code |

**Note:** Negative compression is common for:
- Game codebases (many state changes, complex control flow)
- Enterprise applications (many integrations, I/O operations)
- Code with extensive cross-references

The semantic data is still valuable because it's *structured* and *queryable*,
even when larger than raw source.
```

---

### 5. Document Risk Level Criteria

**Problem:** 95% of files being "high risk" isn't useful for prioritization. Users don't know what triggers each risk level.

**Current Output:**
```
risk_breakdown: "high:1024,medium:10,low:42"
```

**Recommended Documentation Addition:**

```markdown
## Risk Level Criteria

Risk levels are calculated based on behavioral complexity:

### High Risk
Assigned when ANY of these are true:
- File I/O operations (read, write, delete)
- Network operations (HTTP, sockets)
- Database operations (queries, transactions)
- State mutations (global state, singletons)
- Complex control flow (>5 branches, nested loops)
- External process execution
- Serialization/deserialization
- >15 function calls in a single function

### Medium Risk
Assigned when:
- Some state changes but no I/O
- Moderate complexity (3-5 branches)
- 5-15 function calls
- Uses mutable references

### Low Risk
Assigned when:
- Pure functions (no side effects)
- Simple control flow (<3 branches)
- Configuration or constants
- Type definitions only
- <5 function calls

### Calibration Notes
- Game codebases typically show 80-95% high risk (expected)
- CLI tools typically show 40-60% high risk
- Libraries typically show 20-40% high risk

If your codebase shows >90% high risk, consider using the `search_symbols(risk="high")`
with additional filters like `module` to narrow down to the most critical areas.
```

---

### 6. Accurate Token Budget Documentation

**Problem:** Current docs underestimate token usage for large repos.

**Current Documentation:**
```markdown
**Token budget per query:**
- search_symbols: ~400 tokens (20 results)
- list_symbols: ~800 tokens (50 results)
- get_symbol: ~350 tokens
- get_symbol_source: ~400 tokens (50 lines)
```

**Recommended Update:**

```markdown
## Token Budget Guidelines

### Small Repos (<100 files)
| Tool | Typical Tokens | Notes |
|------|----------------|-------|
| get_repo_overview | 500-1,000 | Always fits |
| get_call_graph | 1,000-5,000 | Usually fits |
| find_duplicates | 500-2,000 | Usually fits |
| search_symbols (20) | 400-600 | Predictable |

### Medium Repos (100-500 files)
| Tool | Typical Tokens | Notes |
|------|----------------|-------|
| get_repo_overview | 1,000-3,000 | Usually fits |
| get_call_graph | 5,000-15,000 | May need filtering |
| find_duplicates | 2,000-10,000 | May need filtering |
| search_symbols (20) | 400-800 | Predictable |

### Large Repos (500+ files)
| Tool | Typical Tokens | Notes |
|------|----------------|-------|
| get_repo_overview | 2,000-5,000 | Usually fits |
| get_call_graph | 15,000-50,000+ | **USE FILTERING** |
| find_duplicates | 10,000-50,000+ | **USE FILTERING** |
| search_symbols (20) | 400-1,000 | Predictable |

### When Output is Truncated

If you see `[OUTPUT TRUNCATED]`, use these strategies:

1. **For call graphs:** Add `module` filter or `depth=1`
2. **For duplicates:** Increase `threshold` to 0.95+ or add `module` filter
3. **For symbol lists:** Reduce `limit` or add `kind` filter
```

---

## New Functionality Proposals

### 7. Add `get_call_graph_summary`

**Purpose:** Get high-level call graph statistics without the full edge list.

**Proposed API:**
```rust
pub struct GetCallGraphSummaryRequest {
    pub path: Option<String>,
    pub module: Option<String>,
}

pub struct CallGraphSummary {
    pub total_edges: u32,
    pub total_symbols: u32,
    pub avg_calls_per_function: f32,
    pub max_calls_in_function: u32,
    pub top_callers: Vec<SymbolCallCount>,    // Functions that make the most calls
    pub top_callees: Vec<SymbolCallCount>,    // Functions called the most
    pub orphan_functions: u32,                 // Functions with no callers
    pub leaf_functions: u32,                   // Functions that call nothing
    pub circular_dependency_count: u32,
}

pub struct SymbolCallCount {
    pub symbol: String,
    pub hash: String,
    pub count: u32,
    pub file: String,
}
```

**Example Output:**
```yaml
_type: call_graph_summary
total_edges: 6913
total_symbols: 13451
avg_calls_per_function: 5.2
max_calls_in_function: 47
top_callers:
  - symbol: "LoadGame"
    hash: "35df80451a7a9298"
    count: 47
    file: "SaveLoadManager.cs"
  - symbol: "Setup"
    hash: "c28a641f515b31a2"
    count: 38
    file: "DaggerfallUnitySettingsWindow.cs"
top_callees:
  - symbol: "ext:DaggerfallUI.Instance.PlayOneShot"
    count: 89
  - symbol: "ext:TextManager.Instance.GetLocalizedText"
    count: 156
orphan_functions: 234
leaf_functions: 1847
circular_dependency_count: 12
```

---

### 8. Add `get_hotspots`

**Purpose:** Quickly identify the most problematic areas of a codebase.

**Proposed API:**
```rust
pub struct GetHotspotsRequest {
    pub path: Option<String>,
    pub metric: String,  // "risk" | "complexity" | "connectivity" | "churn" | "duplicates"
    pub limit: Option<u32>,  // Default: 20
}

pub struct Hotspot {
    pub symbol: String,
    pub hash: String,
    pub file: String,
    pub lines: (u32, u32),
    pub score: f32,
    pub reasons: Vec<String>,
}
```

**Example Output:**
```yaml
_type: hotspots
metric: complexity
showing: 10
hotspots:
  - symbol: "LoadGame"
    hash: "35df80451a7a9298"
    file: "SaveLoadManager.cs"
    lines: [412, 589]
    score: 94.5
    reasons:
      - "47 external calls"
      - "12 state mutations"
      - "8 nested control structures"
      - "File I/O operations"
  - symbol: "CreateFoe"
    hash: "1a8208c1526687a9"
    file: "CreateFoe.cs"
    lines: [1, 245]
    score: 87.2
    reasons:
      - "Complex quest integration"
      - "Entity spawning logic"
      - "State synchronization"
```

---

### 9. Add `get_architecture_issues`

**Purpose:** Automated detection of common architectural problems.

**Proposed API:**
```rust
pub struct GetArchitectureIssuesRequest {
    pub path: Option<String>,
    pub checks: Option<Vec<String>>,  // Filter to specific checks
}

pub struct ArchitectureIssues {
    pub circular_dependencies: Vec<CircularDep>,
    pub god_classes: Vec<GodClass>,
    pub duplicate_code_debt: DuplicateDebt,
    pub orphan_code: Vec<OrphanSymbol>,
    pub high_coupling: Vec<CouplingIssue>,
    pub missing_abstractions: Vec<AbstractionOpportunity>,
}
```

**Example Output:**
```yaml
_type: architecture_issues
circular_dependencies:
  - cycle: ["GameManager", "PlayerEntity", "EntityEffectManager", "GameManager"]
    files: ["GameManager.cs", "PlayerEntity.cs", "EntityEffectManager.cs"]
    severity: high

god_classes:
  - symbol: "TalkManager"
    file: "TalkManager.cs"
    lines: 3847
    methods: 156
    responsibilities:
      - "NPC dialogue"
      - "Quest integration"
      - "Rumor management"
      - "Building directory"
      - "Topic management"
    recommendation: "Consider splitting into TalkManager, RumorManager, TopicManager"

duplicate_code_debt:
  clusters: 449
  total_duplicate_functions: 1297
  estimated_lines_saveable: 4200
  top_opportunities:
    - pattern: "File reader constructors"
      occurrences: 31
      files: ["ImgFile.cs", "FactionFile.cs", "MagicItemsFile.cs", ...]
      recommendation: "Extract to BaseFileReader class"

orphan_code:
  count: 234
  top_examples:
    - symbol: "DebugTeleport"
      file: "DebugCommands.cs"
      reason: "No callers found"

high_coupling:
  - symbol: "DaggerfallUI"
    dependents: 347
    severity: critical
    recommendation: "Consider facade pattern or event system"
```

---

### 10. Enhanced Duplicate Detection Filtering

**Current Issues:**
- `exclude_boilerplate` didn't seem to filter effectively (449 clusters still found)
- ~~No way to focus on specific modules~~ **FIXED:** `module` filter already exists
- No minimum size threshold
- No pagination for large result sets

**Proposed Improvements:**

```rust
pub struct FindDuplicatesRequest {
    pub path: Option<String>,
    pub threshold: Option<f64>,           // Default: 0.90
    pub exclude_boilerplate: Option<bool>, // Default: true
    pub module: Option<String>,           // ALREADY EXISTS - Filter to specific module

    // NEW PARAMETERS:
    pub min_lines: Option<u32>,           // Ignore functions < N lines (default: 3)
    pub min_statements: Option<u32>,      // Ignore functions < N statements
    pub exclude_patterns: Option<Vec<String>>,  // e.g., ["ToString", "GetHashCode"]
    pub include_cross_module: Option<bool>,     // Only show duplicates across modules
    pub limit: Option<u32>,               // Max clusters (default: 50)
    pub offset: Option<u32>,              // Pagination
    pub sort_by: Option<String>,          // "similarity" | "size" | "count" | "impact"
}
```

**New Boilerplate Patterns to Exclude:**

```rust
const BOILERPLATE_PATTERNS: &[&str] = &[
    // Common overrides
    "ToString",
    "GetHashCode",
    "Equals",
    "CompareTo",

    // Unity lifecycle
    "Awake",
    "Start",
    "Update",
    "FixedUpdate",
    "LateUpdate",
    "OnEnable",
    "OnDisable",
    "OnDestroy",

    // Property accessors (if trivial)
    "get_*",
    "set_*",

    // Event handlers (if just delegation)
    "*_OnClick",
    "*_OnChanged",

    // Builder pattern
    "With*",
    "Set*",  // Only if single-line
];
```

---

## MCP Server Instructions Update

**Proposed Addition to Server Instructions:**

```markdown
## Handling Large Codebases (>500 files)

For repositories with many files, follow this workflow to avoid truncation:

### Step 1: Get Overview (Always Safe)
```
get_repo_overview(path)
```
This returns a compact summary that always fits in context.

### Step 2: Explore Incrementally

**Instead of:**
```
get_call_graph(path)  // May exceed token limit!
```

**Use:**
```
get_call_graph_summary(path)  // Get stats first
search_symbols(query="Manager", limit=20)  // Find specific symbols
get_symbol(hash)  // Deep dive on specific functions
```

### Step 3: Filter Large Operations

**For call graphs:**
```
get_call_graph(path, module="Game.Entities", depth=1)
```

**For duplicates:**
```
find_duplicates(path, threshold=0.95, min_lines=10, limit=30)
```

### Step 4: Use Summary Modes

When you need statistics but not full data:
```
get_call_graph_summary(path)  // Counts and top items only
get_architecture_issues(path)  // Automated issue detection
get_hotspots(path, metric="complexity", limit=10)  // Focus areas
```

## When Output is Truncated

If you see `[OUTPUT TRUNCATED - exceeded 25000 token limit]`:

1. **Don't retry the same query** - it will truncate again
2. **Add filters** to narrow results:
   - `module` - limit to specific module
   - `limit` - reduce result count
   - `threshold` - increase similarity threshold (for duplicates)
   - `depth` - reduce call depth (for call graphs)
3. **Use summary endpoints** when full data isn't needed
4. **Paginate** using `offset` if you need complete data
```

---

## Current Workarounds

Until these improvements are implemented, users can use these workarounds:

### Listing All Symbols with a Filter

**Problem:** `search_symbols(query="*", risk="high")` returns 0 results.

**Workaround:** Use `list_symbols` with module filter:
```
list_symbols(module="known-module", risk="high")
```

Or use empty query (works because `"".contains("")` matches everything):
```
search_symbols(query="", risk="high", limit=50)
```

### Finding High-Risk Symbols Without Module Name

**Problem:** Need to find all high-risk symbols but don't know module names.

**Workaround:** First list modules, then query each:
```
1. list_modules(path)  → Get module names
2. For each module: list_symbols(module, risk="high")
```

### Handling Truncated Results

**Problem:** `get_call_graph` or `find_duplicates` returns truncated output.

**Workaround:** Use the existing `module` filter on `find_duplicates`:
```
find_duplicates(path, module="specific-module", threshold=0.95)
```

For `get_call_graph`, no workaround exists until pagination is added.

---

## Implementation Priority

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| **P0** | Add `limit`/`offset` to get_call_graph | Low | High |
| **P0** | Add `limit`/`offset` to find_duplicates | Low | High |
| **P0** | Fix module path normalization | Low | Medium |
| **P1** | Add `get_call_graph_summary` | Medium | High |
| **P1** | Document risk criteria | Low | Medium |
| **P1** | Document token budgets accurately | Low | Medium |
| **P1** | Fix/document wildcard search | Low | Medium |
| **P2** | Add `get_hotspots` | Medium | High |
| **P2** | Add `get_architecture_issues` | High | High |
| **P2** | Enhanced boilerplate detection | Medium | Medium |
| **P3** | Add `min_lines` to find_duplicates | Low | Low |
| **P3** | Clarify compression ratio in docs | Low | Low |

---

## Appendix: Test Case Details

### Repository Used for Testing
- **Name:** daggerfall-unity
- **Location:** `/home/kadajett/Dev/Semfora_org/semfora-engine/test-repos/daggerfall-unity`
- **Files:** 1,076
- **Symbols:** 13,451
- **Modules:** 226
- **Call Graph Edges:** 6,913
- **Duplicate Clusters:** 449

### Tools Tested
- `generate_index` - Worked correctly
- `get_repo_overview` - Worked correctly
- `get_call_graph` - Truncated at 25K tokens
- `find_duplicates` - Truncated at 25K tokens
- `search_symbols` - Wildcard query returned 0 results
- `list_modules` - Returned inconsistent path formats

### Token Usage Observed
| Tool | Expected | Actual |
|------|----------|--------|
| get_repo_overview | ~2,000 | ~1,500 |
| get_call_graph | ~5,000 | 25,000+ (truncated) |
| find_duplicates | ~2,000 | 25,000+ (truncated) |
| list_modules | ~500 | ~3,000 |
