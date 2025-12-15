//! MCP Server Instructions
//!
//! This module contains the instructions that are sent to AI assistants
//! to help them understand how to use the semfora-engine MCP tools effectively.

/// Instructions for AI assistants on how to use the MCP tools efficiently
pub(super) const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Purpose
Produces highly compressed semantic summaries, enabling efficient code review without reading entire files.

## CRITICAL: Choose the Right Search Tool

| Question Type | Tool | Example |
|---------------|------|---------|
| Know the name | `search_symbols` | "find validateUser function" |
| Conceptual/exploratory | `semantic_search` | "how does authentication work?" |
| Text patterns | `raw_search` | find `#[tool(` decorators, comments |

**WRONG**: Using `search_symbols("auth")` for "how does auth work?"
**RIGHT**: Using `semantic_search("authentication workflow")`

## Quick Start Workflow

1. `get_context()` -> Git + project info (~200 tokens) - USE FIRST
2. `get_repo_overview()` -> Architecture (~500 tokens)
3. Choose search method:
   - Exploratory? -> `semantic_search("your concept")`
   - Know name? -> `search_and_get_symbols("name", include_source: true)`
4. `get_callers(hash)` -> Before modifying, check impact
5. `get_symbol_source(hash)` -> Surgical code read for editing

## Token-Efficient Patterns

### DO: Combined queries
```
search_and_get_symbols("Request", include_source: true, limit: 10)
```
One call returns search results + full details + source (~1,500 tokens)

### DON'T: Sequential calls
```
search_symbols("Request")     -> 400 tokens
get_symbol(hash1)             -> 350 tokens
get_symbol(hash2)             -> 350 tokens
get_symbol_source(hash1)      -> 400 tokens
get_symbol_source(hash2)      -> 400 tokens
TOTAL: ~1,900 tokens + 5 round trips
```

### DO: File overview first
```
get_file_symbols("types.rs", include_source: false)  -> ~500 tokens
```
Then surgical reads for specific symbols.

### DON'T: Read file in chunks
```
get_symbol_source(file, 1-150)     -> 2,000 tokens
get_symbol_source(file, 150-300)   -> 2,000 tokens
get_symbol_source(file, 300-450)   -> 2,000 tokens
TOTAL: ~6,000 tokens
```

## Actual Token Costs (measured)

| Tool | Tokens | Notes |
|------|--------|-------|
| `get_context` | ~200 | Use FIRST |
| `get_repo_overview` | ~500 | Architecture |
| `search_symbols` | ~400 | 20 results |
| `semantic_search` | ~800 | 20 results with scores |
| `search_and_get_symbols` | ~1,500 | 10 results with source |
| `get_file_symbols` | ~500 | File overview |
| `list_symbols` | ~5,000 | EXPENSIVE - prefer search_symbols |
| `list_modules` | ~2,000 | Can be large |
| `get_symbol_source` | ~400/50 lines | Use for final edits |
| `raw_search` | ~1,000 | 50 matches |

## Tools by Category

### Query-Driven (Most Efficient)
- **search_symbols**: Find by name pattern (wildcards supported)
- **semantic_search**: Find by concept (BM25 ranking)
- **search_and_get_symbols**: Combined search + details in ONE call
- **get_file_symbols**: All symbols in a file (overview)
- **get_symbol** / **get_symbols**: Detailed info by hash
- **get_symbol_source**: Surgical source code read
- **get_callers**: Who calls this function? (impact analysis)

### Repository Analysis
- **get_context**: Quick git/project context (~200 tokens) - USE FIRST
- **get_repo_overview**: Architecture summary
- **analyze_diff**: Semantic diff between branches/commits
- **get_call_graph**: Function call relationships (use filters!)
- **find_duplicates**: Codebase-wide duplicate detection
- **validate_symbol**: Quality audit (complexity + duplicates + callers)

### Index Management
- **generate_index**: Create/regenerate sharded index
- **check_index**: Check staleness, auto-refresh option
- **list_modules**: Available modules (can be large)

### On-Demand Analysis
- **analyze_file**: Single file analysis
- **analyze_directory**: Directory analysis
- **raw_search**: Ripgrep fallback (comments, strings, decorators)

### Testing
- **run_tests**: Auto-detect and run tests
- **detect_tests**: Find test frameworks

## AVOID These Patterns

- `list_symbols` / `list_modules` for exploration (use search instead)
- `get_module` (extremely expensive - use list_symbols + get_symbol)
- Sequential `get_symbol_source` on same file (use get_file_symbols first)
- `search_symbols` for conceptual questions (use semantic_search)
- Multiple search -> get_symbol -> get_source chains (use search_and_get_symbols)

## Code Review with analyze_diff

When reviewing changes:
1. **High risk**: Use `get_symbol_source` for specific sections
2. **Before modifying**: Use `get_callers` to check impact
3. **Quality audit**: Use `validate_symbol` for complexity analysis

## Expensive Operations (Require Confirmation)

### export_call_graph_sqlite
Exports call graph to SQLite database for visualization with semfora-graph explorer.

**IMPORTANT**: This is an expensive disk-writing operation.
- ALWAYS ask for user confirmation before running
- Can take several minutes on large codebases (>100k edges)
- Writes directly to disk - default location: `<cache>/call_graph.sqlite`
- Returns ONLY statistics - NEVER returns actual data to avoid token blowout
- Note: semfora-graph may not be installed on the user's system

**When to use**:
- User explicitly requests SQLite export for graph visualization
- User wants to explore call graph in external tools
- User mentions semfora-graph, graph database, or SQLite export

**Example confirmation prompt**:
"I'll export the call graph to SQLite. This may take a few minutes for large codebases.
The file will be written to: <path>
Proceed? (This is an expensive operation)"

## Remember
- Start with `get_context` (~200 tokens vs reading git status)
- Use `semantic_search` for "how does X work?" questions
- Use `search_and_get_symbols` to combine operations
- Check `get_callers` before modifying functions
- Use `get_file_symbols` before multiple source reads
- Never use direct file reads when MCP tools available"#;
