//! MCP Server Instructions - Fast variant (decision tree focused)
//!
//! This variant prioritizes speed to the correct tool with a prominent decision tree.
//! Estimated token cost: ~2000 tokens

/// Instructions for AI assistants on how to use the MCP tools efficiently
pub(super) const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Entry Point by Request Type

| Request | Entry Point | Why |
|---------|-------------|-----|
| "Audit/understand codebase" | get_context → get_overview | Need architecture + module names |
| "Find X / where is Y" | get_context → search("X") | Search auto-refreshes index |
| "Analyze this file" | analyze(path) | On-demand parsing |
| "Review changes/PR" | analyze_diff(base_ref) | Independent of index |
| "What calls this?" | search → get_callers(hash) | Direct to impact |
| "Check quality" | get_overview → validate(module) | Need module names first |

## Large File Strategy (>500 lines)

| File Size | Strategy | Why |
|-----------|----------|-----|
| <500 lines | get_source(file, start, end) | Direct read is fine |
| 500-2000 lines | get_file(file_path) → get_source(hash) | Get symbol map first |
| >2000 lines | analyze(path) → search → get_source(hash) | Never read directly |

**For files >2000 lines:** Use analyze(path) for ~500 token summary, search for specific symbols, get_source(hash) for surgical reads. **NEVER use file Read tool on large files.**

## Workflows

### Codebase Audit
1. `get_context()` → index status
2. `get_overview()` → **save module names**
3. `find_duplicates(limit: 30)` → duplication
4. `validate(module: "<from_overview>")` → complexity
5. For high-complexity: `get_callers(hash)` → impact BEFORE recommending changes

### Find Code
1. `get_context()` → orientation
2. `search("query", limit: 10)` → hybrid search (default)
3. Use hash → `get_symbol(hash)` or `get_callers(hash)`
**Skip get_overview** - search auto-refreshes!
**Variables are hidden by default** - use `symbol_scope: "variables"` or `"both"` if needed.

### Code Review
1. `analyze_diff(base_ref: "main")` → changes
   - For large PRs (50+ files): use `summary_only: true` first (~300 tokens)
   - Then paginate: `limit: 20, offset: 0` to review in batches
2. For risky: `get_callers(hash)` → impact
**Skip get_overview** - diff independent!

## Token Costs

| Tool | Tokens | Notes |
|------|--------|-------|
| get_context | ~200 | Always first |
| get_overview | ~1-2k | Only for audits/module names |
| search | ~500-1k | Auto-refreshes index |
| analyze_diff(summary_only) | ~300 | Quick PR overview |
| analyze_diff(paginated) | ~2-5k/page | Use limit/offset for large diffs |
| get_callgraph(summary) | ~300 | Coupling overview |
| validate | ~1-2k | Requires scope |
| get_callers | ~500 | Before any changes |
| analyze | ~500 | Semantic summary for any file |

## Pagination Rules

| Tool | Paginate When | Default Limit |
|------|--------------|---------------|
| analyze_diff | >20 files | 20 |
| get_callgraph | >500 edges | 500 |
| find_duplicates | >30 clusters | 30 |

**Pattern:** First call with `limit: 20, offset: 0`. Check for `next_offset:` in response. Continue with next offset.

## Tool Loading

**BATCH MCPSearch calls in parallel** - don't call sequentially.

**Pre-load on session start (4 tools):**
- get_context, search, get_source, get_callers

## Critical Rules

1. **get_context first** (~200 tokens, always)
2. **Match entry point to request type** (table above)
3. **Prefer hybrid search** - use default `search()` unless you only need exact name or pure semantic
4. **Default to 10 search results** for hybrid searches unless you need more
5. **Variables hidden by default** - use `symbol_scope: "variables"` or `"both"` when needed
6. **search auto-refreshes** - skip get_overview when searching
7. **COPY module names EXACTLY from get_overview** - names vary by project (e.g., `semfora_pm.db` not `database`)
8. **Use hashes from results** - don't re-search
9. **get_callers BEFORE recommending refactoring**
10. **validate needs scope** (symbol_hash, file_path, or module)
11. **PAGINATE large results** - use limit/offset when >20 items

## Error Recovery

| Error | Recovery |
|-------|----------|
| "Index stale" | index() then retry |
| "Module not found" | get_overview, copy name exactly |
| "Output truncated" | Add filters, reduce limit |
| "File too large" | Use analyze(path) instead of Read |

## Tools Quick Reference

**Start:** get_context, get_overview
**Search:** search (hybrid default), get_file, get_symbol, get_source
**Analysis:** analyze, analyze_diff, get_callers, get_callgraph
**Quality:** validate (requires scope!), find_duplicates
**Ops:** index, test, lint, prep_commit

## AVOID

- get_overview when just searching (search refreshes index)
- **NEVER guess module names** - copy EXACTLY from get_overview (e.g., `semfora_pm.db` not `database`)
- Re-searching symbols you already have (use hash)
- Recommending refactoring without get_callers
- validate() with no scope
- **NEVER use Read on files >2000 lines** - use analyze(path) + search
- Sequential MCPSearch calls - batch them in parallel
- Retrying truncated queries without reducing scope"#;
