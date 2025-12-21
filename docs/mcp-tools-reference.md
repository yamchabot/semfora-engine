# MCP Tools Reference

Complete reference for all semfora-engine MCP tools.

## Quick Reference Table

| Tool | Purpose | Token Cost | Use When |
|------|---------|------------|----------|
| `get_context` | Git/project context | ~200 | Always first |
| `get_overview` | Repository overview | ~1-2k | Audits, module discovery |
| `search` | Find code | ~500-1k | Searching (auto-refreshes index) |
| `analyze` | Semantic analysis | ~500 | Single file analysis |
| `analyze_diff` | Git diff review | ~300-5k | Code reviews, PRs |
| `get_file` | Symbol listing | ~300 | Large file navigation |
| `get_symbol` | Symbol details | ~200 | Getting specific symbols |
| `get_source` | Source code | ~varies | Reading code |
| `get_callers` | Reverse call graph | ~500 | Impact analysis |
| `get_callgraph` | Call graph | ~300-5k | Dependency analysis |
| `validate` | Quality metrics | ~1-2k | Complexity checks |
| `find_duplicates` | Duplicate detection | ~1-2k | Duplication audit |
| `index` | Refresh index | ~100 | When stale |
| `test` | Run tests | ~varies | Test execution |
| `security` | CVE scanning | ~1-2k | Security audits |
| `prep_commit` | Commit prep | ~500 | Before committing |

---

## Tool Details

### get_context

Get quick git and project context. **Use this FIRST** in every workflow.

**Parameters:**
- `path` (optional): Repository path (defaults to current directory)

**Output:** ~200 tokens
- Repository name, branch, last commit
- Index status (fresh, stale, missing)
- Project type detection

**Example:**
```json
{ "path": "/path/to/repo" }
```

---

### get_overview

Get repository architecture overview from the index.

**Parameters:**
- `path` (optional): Repository path
- `max_modules` (optional): Limit modules returned (default: all)

**Output:** ~1-2k tokens
- Module structure with symbol counts
- Language breakdown
- High-level architecture

**Use for:** Discovering module names for subsequent calls

---

### search

Hybrid search across symbols and code.

**Parameters:**
- `query` (required): Search query
- `mode` (optional): "hybrid" (default), "symbol", "semantic", "raw"
- `limit` (optional): Max results (default: 20)
- `path` (optional): Scope to directory

**Output:** ~500-1k tokens
- Matching symbols with file, line, kind
- Symbol hashes for follow-up calls

**Note:** Search auto-refreshes the index - skip `get_overview` when searching.

---

### analyze

Semantic analysis of file, directory, or module.

**Parameters:**
- `path` (optional): File or directory path
- `module` (optional): Module name (alternative to path)
- `format` (optional): "toon" (default) or "json"
- `start_line` (optional): Focus mode start (for large files)
- `end_line` (optional): Focus mode end (for large files)
- `output_mode` (optional): "full", "summary", or "symbols_only"

**Output:** ~500 tokens (file), varies for directory
- Symbols, calls, dependencies
- Risk assessment
- For large files: navigation hints

**Large File Handling:**
- Files >3000 lines or >500KB return a `large_file_notice`
- Use `start_line`/`end_line` for focused analysis
- Use `output_mode="symbols_only"` for quick overview

---

### analyze_diff

Analyze changes between git refs.

**Parameters:**
- `base_ref` (required): Base branch/commit (e.g., "main")
- `target_ref` (optional): Target (defaults to "HEAD", use "WORKING" for uncommitted)
- `working_dir` (optional): Repository path
- `limit` (optional): Files per page (default: 20, max: 100)
- `offset` (optional): Pagination offset
- `summary_only` (optional): Return only statistics (~300 tokens)

**Output:** ~300 tokens (summary), ~2-5k (full)
- Changed files with semantic diffs
- Risk assessment per file
- New/modified/deleted symbols

**Pagination Pattern:**
1. First: `analyze_diff(base_ref: "main", summary_only: true)`
2. Then: `analyze_diff(base_ref: "main", limit: 20, offset: 0)`
3. Continue with offset until `next_offset` is absent

---

### get_file

List all symbols in a file with line ranges.

**Parameters:**
- `file_path` (required): Path to file

**Output:** ~300 tokens
- Symbol list with names, kinds, line ranges
- Use for navigating large files

---

### get_symbol

Get detailed symbol information.

**Parameters:**
- `symbol_hash` (optional): Symbol hash from search results
- `hashes` (optional): Batch of up to 20 hashes
- `file_path` + `line` (optional): Look up by location

**Output:** ~200 tokens per symbol
- Full semantic details
- Dependencies, calls, complexity

---

### get_source

Extract source code.

**Parameters:**
- `symbol_hash` (optional): Get source for symbol
- `hashes` (optional): Batch of up to 20 hashes
- `file_path` + `start_line` + `end_line` (optional): Line range

**Output:** Varies with size
- Raw source code with context

---

### get_callers

Find what calls a symbol (reverse call graph).

**Parameters:**
- `symbol_hash` (required): Target symbol hash
- `depth` (optional): How deep to trace (default: 1, max: 5)
- `limit` (optional): Max callers (default: 20)

**Output:** ~500 tokens
- List of calling symbols with context
- **Use BEFORE recommending refactoring**

---

### get_callgraph

Get call graph for the repository.

**Parameters:**
- `path` (optional): Repository path
- `symbol_hash` (optional): Focus on specific symbol
- `summary_only` (optional): Just statistics (~300 tokens)
- `limit` (optional): Edges per page (default: 500, max: 2000)
- `offset` (optional): Pagination offset

**Output:** ~300-5k tokens
- Call relationships
- Coupling metrics

---

### validate

Quality validation with complexity metrics.

**Parameters:**
- `module` (optional): Module name (from get_overview)
- `file_path` (optional): Specific file
- `symbol_hash` (optional): Specific symbol
- `limit` (optional): Max results (default: 50)

**Requires one of:** `module`, `file_path`, or `symbol_hash`

**Output:** ~1-2k tokens
- Complexity scores
- Nesting depth
- Line counts

---

### find_duplicates

Detect code duplication.

**Parameters:**
- `path` (optional): Repository path
- `threshold` (optional): Similarity % (default: 80)
- `limit` (optional): Max clusters (default: 50)
- `offset` (optional): Pagination offset

**Output:** ~1-2k tokens
- Duplicate clusters
- Similarity percentages
- File locations

---

### index

Manage the semantic index.

**Parameters:**
- `operation` (optional): "refresh" (default), "check", "clear"
- `path` (optional): Repository path

**Output:** ~100 tokens
- Index status
- Files processed

---

### test

Run or discover tests.

**Parameters:**
- `path` (optional): Test path or pattern
- `discover_only` (optional): Just list tests

**Output:** Varies
- Test results or discovery

---

### security

Scan for CVE patterns.

**Parameters:**
- `path` (optional): Repository path
- `severity` (optional): Minimum severity

**Output:** ~1-2k tokens
- Security findings
- Severity levels

---

### prep_commit

Prepare commit with semantic analysis.

**Parameters:**
- `working_dir` (optional): Repository path

**Output:** ~500 tokens
- Staged changes analysis
- Suggested commit message
- Risk assessment

---

## Token Budget Guidelines

| Context | Target | Action if Exceeded |
|---------|--------|-------------------|
| Per tool call | <5k | Use summary_only, reduce limit |
| Per workflow | <15k | Paginate, filter by module |
| Full audit | <50k | Split into multiple queries |

## Error Recovery

| Error | Recovery |
|-------|----------|
| "Index stale" | Call `index()` then retry |
| "Module not found" | Call `get_overview`, copy name exactly |
| "Output truncated" | Add filters, reduce limit, paginate |
| "File too large" | Use `analyze(path, start_line, end_line)` |
