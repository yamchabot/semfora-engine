//! MCP Server Instructions - Compact variant (token efficiency)
//!
//! This variant prioritizes minimal token usage with maximum information density.
//! Estimated token cost: ~500 tokens

/// Instructions for AI assistants on how to use the MCP tools efficiently
pub(super) const MCP_INSTRUCTIONS: &str = r#"semfora-engine MCP - Semantic Code Analysis

## Entry Points
| Request | Path |
|---------|------|
| Audit | get_context → get_overview → validate(module) |
| Find | get_context → search(limit: 10) |
| File | analyze(path) |
| Diff | analyze_diff(base) |
| Impact | search → get_callers(hash) |

## Token Cost
get_context:200 | get_overview:1-2k | search:500-1k | validate:1-2k | get_callers:500

## Rules
- get_context first
- COPY module names EXACTLY from get_overview (e.g., `semfora_pm.db` not `database`)
- prefer hybrid search (default), limit 10
- variables hidden by default (`symbol_scope: "variables"` or `"both"` to include)
- search auto-refreshes index
- Use hashes, don't re-search
- get_callers before refactoring
- validate needs: symbol_hash OR file_path OR module

## Tools
Start: get_context, get_overview
Search: search, get_file, get_symbol, get_source
Analysis: analyze, analyze_diff, get_callers, get_callgraph
Quality: validate, find_duplicates
Ops: index, test, lint, prep_commit"#;
