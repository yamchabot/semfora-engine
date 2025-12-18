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

## Workflows

### Codebase Audit
1. `get_context()` → index status
2. `get_overview()` → **save module names**
3. `find_duplicates(limit: 30)` → duplication
4. `validate(module: "<from_overview>")` → complexity
5. For high-complexity: `get_callers(hash)` → impact BEFORE recommending changes

### Find Code
1. `get_context()` → orientation
2. `search("query")` → hybrid search
3. Use hash → `get_symbol(hash)` or `get_callers(hash)`
**Skip get_overview** - search auto-refreshes!

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

## Critical Rules

1. **get_context first** (~200 tokens, always)
2. **Match entry point to request type** (table above)
3. **search auto-refreshes** - skip get_overview when searching
4. **COPY module names EXACTLY from get_overview** - names vary by project (e.g., `semfora_pm.db` not `database`)
5. **Use hashes from results** - don't re-search
6. **get_callers BEFORE recommending refactoring**
7. **validate needs scope** (symbol_hash, file_path, or module)

## Tools Quick Reference

**Start:** get_context, get_overview
**Search:** search (hybrid default), get_file, get_symbol, get_source
**Analysis:** analyze, analyze_diff, get_callers, get_callgraph
**Quality:** validate (requires scope!), find_duplicates
**Ops:** index, test, security, prep_commit

## AVOID

- get_overview when just searching (search refreshes index)
- **NEVER guess module names** - copy EXACTLY from get_overview (e.g., `semfora_pm.db` not `database`)
- Re-searching symbols you already have (use hash)
- Recommending refactoring without get_callers
- validate() with no scope"#;
