//! MCP Server Instructions - Complete variant (full documentation)
//!
//! This variant provides comprehensive documentation for all tools.
//! Estimated token cost: ~4000 tokens

/// Instructions for AI assistants on how to use the MCP tools efficiently
pub(super) const MCP_INSTRUCTIONS: &str = r#"MCP Semantic Diff - Code Analysis for AI Review

## Purpose
Token-efficient semantic code analysis. Produces highly compressed summaries enabling efficient code review without reading entire files.

## Entry Point Decision Tree

Choose your entry point based on the request type:

| Request Type | Optimal Entry Point | Skip | Reason |
|-------------|---------------------|------|--------|
| "Audit this codebase" | get_context → get_overview | - | Need full architecture + module names |
| "Find X / where is Y" | get_context → search("X") | get_overview | Search auto-refreshes index |
| "What's in this file" | analyze(file_path) | get_overview | On-demand parsing, no index needed |
| "Review PR/changes" | analyze_diff(base_ref) | get_overview | Diff is index-independent |
| "Fix bug in module X" | get_context → get_overview | - | Need module names for validate |
| "Understand function Z" | search("Z") → get_callers(hash) | get_overview | Direct to impact analysis |
| "Check code quality" | get_overview → validate(module) | - | Need module names first |

## Detailed Workflows

### Workflow 1: Codebase Audit
Use when: User asks to understand, audit, or assess overall codebase quality.

1. `get_context()` → Check index status, branch info (~200 tokens)
2. `get_overview()` → Architecture summary, **SAVE THE MODULE NAMES** (~1-2k tokens)
3. `find_duplicates(limit: 30)` → Identify code duplication clusters
4. `validate(module: "<name_from_overview>")` → Complexity per module
5. `get_callgraph(summary_only: true)` → Coupling analysis (~300 tokens)
6. For high-complexity symbols: `get_callers(hash)` → Check impact BEFORE recommending refactoring

### Workflow 2: Find Code
Use when: User asks where something is, how to find X, or searching for code.

1. `get_context()` → Quick orientation (~200 tokens)
2. `search("query", limit: 10)` → Hybrid mode (symbol + semantic search combined)
3. Use hash from results → `get_symbol(hash)` for details or `get_callers(hash)` for impact

**Key insight**: Skip `get_overview` - search auto-refreshes the index!

### Workflow 3: Code Review
Use when: User asks to review changes, PR review, diff analysis.

1. `analyze_diff(base_ref: "main")` → See semantic changes
   - Use `target_ref: "WORKING"` for uncommitted changes
2. For risky changes: `get_callers(hash)` → Impact radius

**Key insight**: Skip `get_overview` - diff analysis is index-independent!

## Token Efficiency Guide

| Tool | Tokens | When to Use |
|------|--------|-------------|
| `get_context` | ~200 | **Always start here** |
| `get_overview` | ~1-2k | Audits, or when you need module names |
| `search` (hybrid) | ~500-1k | Finding code - **auto-refreshes index** (default 10 results) |
| `search` (symbols only) | ~400 | Know exact function name |
| `search` (semantic only) | ~800 | Conceptual queries |
| `search` (raw) | ~1k | Comments, strings, TODOs |
| `get_file` | ~500 | File or module symbols |
| `get_source` | ~400/50 lines | Final code reads for editing |
| `get_callgraph(summary_only)` | ~300 | Architecture coupling |
| `get_callgraph(full)` | ~2-6k | Only when need edge details |
| `validate` | ~1-2k | Quality analysis |
| `get_callers` | ~500 | Impact analysis |

**Critical insight**: `search` auto-refreshes the index. If you're searching anyway, skip `get_overview`.

## Using Results Efficiently

### Hash Reuse Pattern
Results from tools include symbol hashes. USE THEM:
- `validate` returns symbols with hashes → use in `get_callers(hash)`
- `search` returns hashes → use in `get_symbol(hash)` or `get_source(hash)`
- **DON'T re-search for symbols you already have!**

### Module Names
- `validate(module: "X")` requires a valid module name
- **COPY module names EXACTLY from `get_overview()` output**
- Names vary by project (e.g., `semfora_pm.db` not `database`, `semfora_pm.tui` not `ui`)
- **NEVER guess module names** - they must match exactly

## The 18 Tools - Detailed

### Start Here
- **get_context**: Git + project info (~200 tokens). USE FIRST on every session.
- **get_overview**: Architecture + module list. Use for audits or when you need module names.

### Search & Explore
- **search**: Unified search - runs BOTH symbol AND semantic by default (hybrid mode)
  - Default: hybrid (best for most queries)
  - Default to `limit: 10` unless you need more
  - Variables hidden by default (`symbol_scope: "variables"` or `"both"` to include)
  - `mode: "symbols"`: exact name match only
  - `mode: "semantic"`: BM25 conceptual search only
  - `mode: "raw"`: regex for comments/strings/TODOs
  - **Auto-refreshes index!**
- **get_file**: Symbols in file (use `file_path`) OR module (use `module`). Mutually exclusive.
- **get_symbol**: Full semantic details by hash. Supports batch with `hashes: [...]` (max 20).
- **get_source**: Code extraction by hash or file+lines. Use for final edits.

### Analysis
- **analyze**: Unified analysis - auto-detects file, directory, or module scope.
- **analyze_diff**: Semantic diff between refs. Use `target_ref: "WORKING"` for uncommitted.
- **get_callers**: Who calls this function? **USE BEFORE modifying or recommending changes.**
- **get_callgraph**: Dependency graph. Use `summary_only: true` for ~300 tokens vs ~2-6k full.

### Quality
- **validate**: Complexity analysis. **REQUIRES scope** - one of:
  - `symbol_hash`: Single symbol validation
  - `file_path` (+ optional `line`): File or symbol at location
  - `module`: All symbols in module (use names from `get_overview`)
  - No project-wide scan - must specify scope
- **find_duplicates**: Duplicate detection. Default: full codebase scan. Or pass `symbol_hash` for single check.

### Operations
- **index**: Smart refresh (auto-triggered by other tools). Use `force: true` to regenerate.
- **test**: Run tests with auto-detected framework. Use `detect_only: true` to just detect.
- **lint**: Run linters with auto-detection. Supports Rust (clippy, rustfmt), JS/TS (ESLint, Prettier, Biome, TSC), Python (ruff, black, mypy), Go (golangci-lint, gofmt, go vet). Use `detect_only: true` to just detect, `mode: "fix"` to auto-fix.
- **prep_commit**: Gather commit context for writing commit messages. Never commits.
- **server_status**: Diagnostic info with optional `include_layers: true`.

## AVOID These Patterns

1. **Calling get_overview when just searching** - search refreshes index too
2. **NEVER guess module names** - COPY EXACTLY from `get_overview` (e.g., `semfora_pm.db` not `database`)
3. **Re-searching for symbols already in results** - use the hash directly
4. **Recommending refactoring without get_callers** - always check impact first
5. **Calling validate() with no scope** - must specify symbol_hash, file_path, or module
6. **Multiple get_overview calls** - once per session is enough
7. **Sequential get_source on same file** - use get_file first for overview
8. **Using mode: "symbols" for conceptual questions** - just use default hybrid

## Expensive Operations

### get_callgraph(export: "sqlite")
Exports to SQLite for visualization. **ALWAYS ask confirmation first.**
- Takes minutes on large codebases
- Writes directly to disk

## Remember
- `get_context` first (~200 tokens, always)
- Match entry point to request type (see decision tree)
- `search` auto-refreshes index - often skip `get_overview`
- Save module names from `get_overview` for `validate` calls
- Use hashes from results - don't re-search
- `get_callers` BEFORE recommending ANY refactoring
- `validate` needs scope - no project-wide option"#;
