# MCP Workflows Guide

Common workflows for using semfora-engine MCP tools effectively.

## Quick Decision Tree

| Goal | Workflow |
|------|----------|
| Understand codebase | Codebase Audit |
| Find specific code | Code Search |
| Review PR/changes | Code Review |
| Assess refactoring impact | Impact Analysis |
| Analyze large file | Large File Analysis |
| Check code quality | Quality Validation |

---

## Workflow 1: Codebase Audit

**Goal:** Understand architecture, find issues, identify improvement areas.

**Token Budget:** ~10-15k tokens

### Steps

```
1. get_context()
   → Verify index status, get repo basics

2. get_overview()
   → Get module structure, SAVE MODULE NAMES

3. find_duplicates(limit: 30)
   → Identify duplication opportunities

4. validate(module: "<from_overview>")
   → Check complexity per module

5. For high-complexity symbols:
   get_callers(hash)
   → Assess impact BEFORE recommending changes
```

### Example

```json
// Step 1
{ "tool": "get_context" }

// Step 2
{ "tool": "get_overview" }
// Output: modules: ["api", "components.ui", "services.auth"]

// Step 3
{ "tool": "find_duplicates", "limit": 30 }

// Step 4 - use exact module name from step 2
{ "tool": "validate", "module": "components.ui" }

// Step 5 - for high complexity symbol
{ "tool": "get_callers", "symbol_hash": "abc123:def456", "depth": 2 }
```

### Key Rules
- **COPY module names exactly** from get_overview
- **Always call get_callers** before recommending refactoring
- Use pagination for large results

---

## Workflow 2: Code Search

**Goal:** Find specific functions, patterns, or implementations.

**Token Budget:** ~2-5k tokens

### Steps

```
1. get_context()
   → Quick orientation

2. search("query")
   → Find matching symbols
   → Note: auto-refreshes index, skip get_overview!

3. Use results:
   get_symbol(hash) → Details
   get_source(hash) → Code
   get_callers(hash) → Usage
```

### Example

```json
// Find error handling
{ "tool": "get_context" }
{ "tool": "search", "query": "handleError" }

// Get source for a result
{ "tool": "get_source", "symbol_hash": "abc123:def456" }
```

### Key Rules
- **Skip get_overview** - search auto-refreshes index
- Use hash from search results, don't re-search
- Batch symbol lookups with `hashes` array (up to 20)

---

## Workflow 3: Code Review

**Goal:** Review changes in a PR or between commits.

**Token Budget:** ~3-8k tokens

### Steps

```
1. analyze_diff(base_ref: "main", summary_only: true)
   → Quick overview (~300 tokens)

2. If large PR (>20 files):
   analyze_diff(base_ref: "main", limit: 20, offset: 0)
   → Paginate through files

3. For risky changes:
   get_callers(hash)
   → Verify impact of changes
```

### Example

```json
// Step 1: Get summary first
{ "tool": "analyze_diff", "base_ref": "main", "summary_only": true }

// Step 2: Review in batches if needed
{ "tool": "analyze_diff", "base_ref": "main", "limit": 20, "offset": 0 }
{ "tool": "analyze_diff", "base_ref": "main", "limit": 20, "offset": 20 }

// Step 3: Check impact of risky changes
{ "tool": "get_callers", "symbol_hash": "abc123:def456" }
```

### Uncommitted Changes

```json
// Review working directory changes
{ "tool": "analyze_diff", "base_ref": "HEAD", "target_ref": "WORKING" }
```

### Key Rules
- **Always start with summary_only** for large diffs
- **Paginate** - don't request all files at once
- Check get_callers for modified public functions

---

## Workflow 4: Impact Analysis

**Goal:** Understand the impact of changing a function or class.

**Token Budget:** ~2-4k tokens

### Steps

```
1. search("functionName")
   → Find the symbol

2. get_callers(hash, depth: 3)
   → See all callers up to 3 levels deep

3. get_callgraph(symbol_hash: hash, summary_only: true)
   → Understand coupling
```

### Example

```json
// Find the function
{ "tool": "search", "query": "calculateTotal" }

// Get complete caller chain
{ "tool": "get_callers", "symbol_hash": "abc123:def456", "depth": 3 }

// Check coupling
{ "tool": "get_callgraph", "symbol_hash": "abc123:def456", "summary_only": true }
```

### Key Rules
- **Never recommend refactoring without get_callers**
- Higher depth = more complete but more tokens
- Use summary_only for initial assessment

---

## Workflow 5: Large File Analysis

**Goal:** Analyze files >2000 lines without blowing context.

**Token Budget:** ~1-3k tokens

### Steps

```
1. analyze(path)
   → For very large files, returns large_file_notice

2. get_file(file_path)
   → Get symbol list with line ranges

3. analyze(path, start_line: N, end_line: M)
   → Focus on specific section

   OR

   search("specific function")
   → Find specific symbol
   get_source(hash)
   → Get just that code
```

### Example

```json
// Step 1: Try analysis (will return notice for large files)
{ "tool": "analyze", "path": "src/components/RunPanel.tsx" }

// Step 2: Get symbol map
{ "tool": "get_file", "file_path": "src/components/RunPanel.tsx" }

// Step 3a: Focus on specific lines
{
  "tool": "analyze",
  "path": "src/components/RunPanel.tsx",
  "start_line": 100,
  "end_line": 300
}

// Step 3b: Or find specific symbol
{ "tool": "search", "query": "handleSubmit RunPanel" }
{ "tool": "get_source", "symbol_hash": "abc123:def456" }
```

### Output Modes

```json
// Quick overview
{ "tool": "analyze", "path": "...", "output_mode": "summary" }

// Just symbol list
{ "tool": "analyze", "path": "...", "output_mode": "symbols_only" }
```

### Key Rules
- **NEVER use file Read on files >2000 lines**
- Use get_file for navigation, focus mode for analysis
- search + get_source for surgical code retrieval

---

## Workflow 6: Quality Validation

**Goal:** Check complexity, find quality issues.

**Token Budget:** ~3-6k tokens

### Steps

```
1. get_overview()
   → Get module names

2. validate(module: "<name>")
   → Check complexity per module

3. For issues found:
   get_source(hash)
   → See the problematic code
```

### Example

```json
{ "tool": "get_overview" }
// modules: ["api", "services.database", "utils"]

{ "tool": "validate", "module": "services.database", "limit": 50 }

// For high complexity function
{ "tool": "get_source", "symbol_hash": "abc123:def456" }
```

### Key Rules
- **validate requires scope** - provide module, file_path, or symbol_hash
- Copy module names exactly from get_overview
- Use limit to control result size

---

## Anti-Patterns to Avoid

| Don't Do This | Do This Instead |
|---------------|-----------------|
| `get_overview` then `search` | Just `search` (auto-refreshes) |
| Guess module names | Copy from `get_overview` |
| Re-search same symbol | Use hash from first search |
| Recommend refactor without callers | Always check `get_callers` |
| Read large files directly | Use `analyze` + focus mode |
| Request 100+ items at once | Use pagination (limit/offset) |
| Retry truncated query unchanged | Reduce limit or add filters |
| Load tools sequentially | Batch MCPSearch calls |

---

## Tool Loading Optimization

**Pre-load these 4 tools at session start:**
- get_context
- search
- get_source
- get_callers

**Load on demand:**
- validate, find_duplicates (for audits)
- analyze_diff (for reviews)
- get_callgraph (for dependency analysis)
