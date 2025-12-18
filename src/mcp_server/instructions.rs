//! MCP Server Instructions
//!
//! This module contains the instructions that are sent to AI assistants
//! to help them understand how to use the semfora-engine MCP tools effectively.

/// Instructions for AI assistants on how to use the MCP tools efficiently
pub(super) const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Purpose
Produces highly compressed semantic summaries, enabling efficient code review without reading entire files.

## Unified Search (The Magic)

**One `search()` call runs BOTH symbol AND semantic search by default!**

```
search("auth")
-> Returns SYMBOL MATCHES (exact names) + RELATED CODE (conceptual matches)
```

| Mode | When to Use | Example |
|------|-------------|---------|
| Default (hybrid) | Most searches | `search("auth")` - gets both types |
| `mode: "symbols"` | Know exact name | `search("validateUser", mode: "symbols")` |
| `mode: "semantic"` | Conceptual only | `search("error handling", mode: "semantic")` |
| `mode: "raw"` | Comments/strings | `search("TODO", mode: "raw")` |

**No need to choose** - the default hybrid mode gives you everything!

## Quick Start Workflow

1. `get_context()` -> Git + project info (~200 tokens) - USE FIRST
2. `get_overview()` -> Architecture + modules (~500 tokens) - **NOTE THE MODULE NAMES**
3. `search("your query")` -> Unified search (symbol + semantic)
4. `get_callers(hash)` -> Before modifying, check impact
5. `get_source(hash)` -> Surgical code read for editing

## Codebase Audit Workflow

For comprehensive quality audits:
1. `get_context()` + `get_overview()` - Note module names from output
2. `index()` - Refresh if stale (check `get_context` index_status)
3. `find_duplicates(limit: 30)` - Identify code duplication
4. `validate(module: "<name>")` - Check complexity for each module **from overview**
5. `get_callgraph(summary_only: true)` - Understand coupling
6. For high-complexity symbols: `get_callers(hash)` - Assess refactoring risk before recommending changes

## Token-Efficient Patterns

### DO: Use unified search
```
search("Request", include_source: true)
```
One call returns BOTH symbol matches AND related code + source (~1,200 tokens)

### DON'T: Multiple separate searches
```
search("Request", mode: "symbols")  -> 400 tokens
search("Request", mode: "semantic") -> 800 tokens
TOTAL: ~1,200 tokens + 2 round trips
```
Just use the default hybrid mode instead!

### DO: File overview first
```
get_file(file_path: "types.rs", include_source: false)  -> ~500 tokens
```
Then surgical reads for specific symbols.

### DON'T: Read file in chunks
```
get_source(file, 1-150)     -> 2,000 tokens
get_source(file, 150-300)   -> 2,000 tokens
get_source(file, 300-450)   -> 2,000 tokens
TOTAL: ~6,000 tokens
```

## Actual Token Costs (measured)

| Tool | Tokens | Notes |
|------|--------|-------|
| `get_context` | ~200 | Use FIRST |
| `get_overview` | ~500 | Architecture + optional modules |
| `search` (hybrid) | ~1,200 | Symbol + semantic combined - USE THIS |
| `search` (symbols only) | ~400 | 20 results |
| `search` (semantic only) | ~800 | 20 results with scores |
| `search` (raw) | ~1,000 | 50 regex matches |
| `get_file` | ~500 | File or module symbols |
| `get_source` | ~400/50 lines | Use for final edits |

## The 18 Tools

### Query-Driven (Most Efficient)
- **search**: Unified search - runs BOTH symbol AND semantic by default!
  - Default: hybrid mode (best for most queries)
  - `mode: "symbols"`: exact name match only
  - `mode: "semantic"`: BM25 conceptual only
  - `mode: "raw"`: regex for comments/strings
- **get_symbol**: Symbol details by hash (supports batch with `hashes: [...]`)
- **get_source**: Surgical source code read (by hash or file+lines)
- **get_file**: File symbols OR module symbols (mutually exclusive params)
- **get_callers**: Who calls this function? (impact analysis)

### Repository Analysis
- **get_context**: Quick git/project context (~200 tokens) - USE FIRST
- **get_overview**: Architecture summary with optional `include_modules: true`
- **get_languages**: List supported programming languages
- **analyze**: Unified analysis - auto-detects file, directory, or module
- **analyze_diff**: Semantic diff between branches/commits
- **get_callgraph**: Call graph with optional `export: "sqlite"` for visualization

### Quality & Duplicates
- **validate**: Complexity analysis - REQUIRES one of:
  - `symbol_hash`: Single symbol validation
  - `file_path` (+ optional `line`): File or specific symbol
  - `module`: All symbols in module (use names from `get_overview`)
  - ⚠️ No project-wide scan - must specify scope
- **find_duplicates**: Codebase scan (default) or single symbol check (`symbol_hash`)

### Security
- **security**: CVE vulnerability scanning
  - Default: Scan for CVE patterns
  - `stats_only: true`: Check if patterns are loaded
  - `update: true`: Update patterns from server
  - ⚠️ Requires pre-compiled patterns - use `stats_only` first to check availability

### Index Management
- **index**: Smart refresh by default
  - Default: Check freshness, rebuild only if stale
  - `force: true`: Always regenerate
  - `max_age`: Custom staleness threshold (seconds)

### Testing
- **test**: Unified test runner
  - Default: Run tests with auto-detected framework
  - `detect_only: true`: Only detect framework, don't run

### Server & Commit
- **server_status**: Server mode info with optional `include_layers: true`
- **prep_commit**: Gather commit info (never commits, just prepares)

## AVOID These Patterns

- Using `get_file(module: ...)` for exploration (use `search()` instead)
- Sequential `get_source` on same file (use `get_file` first for overview)
- Using `mode: "symbols"` for conceptual questions (just use default hybrid)
- Multiple separate searches (just use the default hybrid mode)
- Guessing module names - always use names from `get_overview()` output
- Calling `validate()` with no parameters - it requires a scope
- Searching for symbols already returned by `validate` - use the hash directly
- Recommending refactoring without checking `get_callers` first

## Code Review with analyze_diff

When reviewing changes:
1. **High risk**: Use `get_source` for specific sections
2. **Before modifying**: Use `get_callers` to check impact
3. **Quality audit**: Use `validate` for complexity analysis

## After Finding High-Complexity Symbols

When `validate` returns symbols with high cognitive complexity (CC > 15):
1. `get_callers(hash)` - Check how many places depend on it (refactoring risk)
2. `get_source(hash)` - Read the code to understand why it's complex
3. `get_callgraph(symbol: "name")` - See what it calls (dependencies)
4. Only then recommend refactoring - with full impact context

## Expensive Operations (Require Confirmation)

### get_callgraph(export: "sqlite")
Exports call graph to SQLite database for visualization with semfora-graph explorer.

**IMPORTANT**: This is an expensive disk-writing operation.
- ALWAYS ask for user confirmation before running
- Can take several minutes on large codebases (>100k edges)
- Writes directly to disk - default location: `<cache>/call_graph.sqlite`
- Returns ONLY statistics - NEVER returns actual data to avoid token blowout

**When to use**:
- User explicitly requests SQLite export for graph visualization
- User wants to explore call graph in external tools
- User mentions semfora-graph, graph database, or SQLite export

**Example confirmation prompt**:
"I'll export the call graph to SQLite. This may take a few minutes for large codebases.
The file will be written to: <path>
Proceed? (This is an expensive operation)"

## Subagent Strategy for Complex Analysis

When facing large exploration tasks (codebase audits, architecture analysis, multi-module tracing),
**spawn a subagent with semfora-engine tools** instead of using the built-in Explore agent.

### Why Subagents with Semfora?
- Built-in Explore uses Glob/Grep/Read (23+ calls, 56k+ tokens, 1+ minute)
- Semfora subagent uses semantic tools (10-15 calls, 18k tokens, faster)
- **70% token reduction** for the same analysis quality

### When to Spawn a Semfora Subagent
Use the Task tool with `subagent_type: "general-purpose"` when:
1. User asks to "understand", "explore", or "audit" a codebase
2. Task requires analyzing multiple modules or tracing call flows
3. Task would otherwise trigger the Explore agent

### Subagent Invocation Pattern
```
Task(
  subagent_type: "general-purpose",
  prompt: "Use semfora-engine MCP tools to analyze [task].
           Start with get_context, then get_overview, then search/validate as needed.
           Return findings with file paths and line numbers.",
  description: "Analyze codebase with Semfora"
)
```

### DO: Spawn subagent for exploration
```
User: "Help me understand the authentication flow"
-> Spawn general-purpose subagent with semfora tools
-> Returns semantic analysis in ~18k tokens
```

### DON'T: Let Explore agent run
```
User: "Help me understand the authentication flow"
-> Explore agent with Glob/Grep/Read
-> 23 tool calls, 56k tokens, 1+ minute
```

**IMPORTANT**: If semfora-engine tools are available, ALWAYS prefer spawning a subagent
with these tools over using the built-in Explore agent for codebase analysis tasks.

## Remember
- Start with `get_context` (~200 tokens vs reading git status)
- Note module names from `get_overview` - don't guess them
- Use `search()` for everything - it runs BOTH symbol AND semantic by default!
- Check `get_callers` before modifying OR recommending refactoring
- Use `get_file` before multiple source reads
- Use batch operations: `get_symbol(hashes: [...])` instead of multiple calls
- `validate` needs a scope (symbol_hash, file_path, or module) - no project-wide
- Never use direct file reads when MCP tools available
- **Spawn subagents with semfora tools for complex exploration** (not Explore agent)"#;
