# Semfora CLI Reference

The `semfora-engine` CLI is a semantic code analyzer that produces TOON output for AI-assisted code review.

## Quick Start

```bash
# Build
cargo build --release

# Index a project
semfora-engine index generate /path/to/project

# Search for symbols
semfora-engine search "authenticate"

# Analyze uncommitted changes
semfora-engine analyze --uncommitted

# Start MCP server
semfora-engine serve --repo .
```

## Installation

```bash
cargo build --release
# Binary: target/release/semfora-engine
```

## Top-Level Usage

```
semfora-engine [OPTIONS] <COMMAND>

Commands:
  analyze    Analyze files, directories, or git changes  [alias: a]
  search     Search for code (hybrid symbol + semantic)  [alias: s]
  query      Query the semantic index                    [alias: q]
  trace      Trace symbol usage across the call graph
  validate   Run quality audits (complexity, duplicates) [alias: v]
  index      Manage the semantic index
  cache      Manage the cache
  test       Run or detect tests
  lint       Run linters                                 [alias: l]
  commit     Prepare information for a commit message
  setup      Setup semfora-engine and MCP client configuration
  uninstall  Uninstall semfora-engine or MCP configurations
  config     Manage semfora-engine configuration
  benchmark  Run token efficiency benchmark
  serve      Start the MCP server (for AI coding assistants)
  help       Print help

Global Options:
  -f, --format <FORMAT>   Output format: text (default), toon, json
  -v, --verbose           Show verbose output
      --progress          Show progress percentage
  -h, --help              Print help
  -V, --version           Print version
```

---

## `analyze` — Analyze Code

Analyze files, directories, or git changes.

```
semfora-engine analyze [OPTIONS] [PATH]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `[PATH]` | Path to file or directory to analyze |

### Options

| Option | Description |
|--------|-------------|
| `--diff [<REF>]` | Analyze git diff (auto-detects main/master if no ref given) |
| `--uncommitted` | Analyze uncommitted changes (working dir vs HEAD) |
| `--commit <SHA>` | Analyze a specific commit |
| `--all-commits` | Analyze all commits on current branch since base |
| `--base <BRANCH>` | Base branch for diff comparison |
| `--target-ref <REF>` | Target ref (defaults to HEAD; use `WORKING` for uncommitted) |
| `--limit <N>` | Max files to show in diff output (pagination) |
| `--offset <N>` | Offset for diff pagination |
| `--max-depth <N>` | Max directory depth (default: 10) |
| `--ext <EXT>` | Filter by extension (repeatable: `--ext rs --ext ts`) |
| `--allow-tests` | Include test files (excluded by default) |
| `--summary-only` | Show summary statistics only |
| `--start-line <LINE>` | Start line for focused analysis (file mode only) |
| `--end-line <LINE>` | End line for focused analysis (file mode only) |
| `--output-mode <MODE>` | `full` (default), `symbols_only`, or `summary` |
| `--print-ast` | Print parsed AST (debugging) |
| `--analyze-tokens <MODE>` | Token analysis: `full` or `compact` |
| `--compare-compact` | Include compact JSON in token analysis |
| `--shard` | Generate sharded index (legacy flag, prefer `index generate`) |
| `--incremental` | Incremental indexing (legacy flag, prefer `index generate --incremental`) |

### Examples

```bash
# Single file
semfora-engine analyze path/to/file.rs

# Directory
semfora-engine analyze ./src

# Uncommitted changes
semfora-engine analyze --uncommitted

# Diff against main
semfora-engine analyze --diff main

# Diff with summary only
semfora-engine analyze --diff origin/main --summary-only

# Specific commit
semfora-engine analyze --commit abc123

# Focused line range
semfora-engine analyze ./src/big_file.rs --start-line 100 --end-line 250

# JSON output
semfora-engine analyze path/to/file.rs --format json
```

---

## `search` — Search Code

Hybrid search across symbol names and code semantics.

```
semfora-engine search [OPTIONS] <QUERY>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<QUERY>` | Search query (required) |

### Options

| Option | Description |
|--------|-------------|
| `-s, --symbols` | Only show exact symbol name matches |
| `-r, --related` | Only show semantically related code |
| `--raw` | Raw regex search (for comments, strings, patterns) |
| `--kind <KIND>` | Filter by symbol kind (fn, struct, component, etc.) |
| `--module <MODULE>` | Filter by module name |
| `--risk <RISK>` | Filter by risk level: high, medium, low |
| `--include-source` | Include source code snippets in output |
| `--limit <N>` | Max results (default: 20) |
| `--file-types <TYPES>` | File types for raw search (e.g., `rs,ts,py`) |
| `--case-sensitive` | Case-sensitive search |
| `--symbol-scope <SCOPE>` | `functions` (default), `variables`, or `both` |
| `--include-escape-refs` | Include local variables that escape scope |

### Examples

```bash
# Hybrid search (default)
semfora-engine search "authenticate"

# Symbol names only
semfora-engine search "handleRequest" --symbols

# Semantic only
semfora-engine search "user login flow" --related

# Filter by kind and risk
semfora-engine search "process" --kind fn --risk high

# Search in a specific module
semfora-engine search "login" --module auth

# Raw regex search
semfora-engine search "TODO|FIXME" --raw

# Include variables in results
semfora-engine search "config" --symbol-scope both
```

---

## `query` — Query the Index

Query the semantic index for symbols, source, callers, etc.

```
semfora-engine query <SUBCOMMAND>
```

### Subcommands

#### `query overview`

Get repository overview.

```bash
semfora-engine query overview
semfora-engine query overview --modules          # Include full module list
semfora-engine query overview --max-modules 50   # Limit modules shown
```

#### `query module <MODULE>`

Get details for a specific module.

```bash
semfora-engine query module src.commands
semfora-engine query module auth --format json
```

#### `query symbol`

Get a symbol by hash or file+line location.

```bash
semfora-engine query symbol abc123def456
semfora-engine query symbol --file-path ./src/main.rs --line 42
```

#### `query source`

Get source code for a file or symbol(s).

```bash
semfora-engine query source abc123def456
semfora-engine query source --file-path ./src/main.rs --start-line 10 --end-line 50
```

#### `query callers <HASH>`

Find what calls a symbol (reverse call graph).

```bash
semfora-engine query callers abc123def456
semfora-engine query callers abc123def456 --depth 3
```

#### `query callgraph`

Get the repository call graph.

```bash
semfora-engine query callgraph
semfora-engine query callgraph --format json
```

#### `query file <FILE_PATH>`

List all symbols in a file.

```bash
semfora-engine query file ./src/main.rs
semfora-engine query file ./src/commands/index.rs
```

#### `query languages`

List all supported languages.

```bash
semfora-engine query languages
```

---

## `trace` — Trace Symbol Usage

Trace a symbol through the call graph in either direction.

```
semfora-engine trace [OPTIONS] <TARGET>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<TARGET>` | Symbol hash or name to trace |

### Options

| Option | Description |
|--------|-------------|
| `--kind <KIND>` | Target kind (function, variable, component, module, file, etc.) |
| `--direction <DIR>` | `incoming`, `outgoing`, or `both` (default: `both`) |
| `--depth <N>` | Max depth to traverse (default: 2) |
| `--limit <N>` | Max edges to return (default: 200) |
| `--offset <N>` | Pagination offset |
| `--include-escape-refs` | Include local variables that escape scope |
| `--include-external` | Include external nodes (ext:*) |
| `--path <PATH>` | Repository path |

### Examples

```bash
# Trace all directions
semfora-engine trace abc123def456

# Incoming only (who calls this?)
semfora-engine trace abc123def456 --direction incoming

# Outgoing only (what does this call?)
semfora-engine trace abc123def456 --direction outgoing --depth 3

# Trace by name
semfora-engine trace "authenticate"
```

---

## `validate` — Quality Audits

Run complexity and duplicate code analysis.

```
semfora-engine validate [OPTIONS] [TARGET]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `[TARGET]` | File path, module name, or symbol hash (auto-detected) |

### Options

| Option | Description |
|--------|-------------|
| `--path <PATH>` | Repository path |
| `--symbol-hash <HASH>` | Symbol hash for single-symbol validation |
| `--file-path <FILE>` | File path for file-level validation |
| `--line <LINE>` | Line number (requires `--file-path`) |
| `--module <MODULE>` | Module name for module-level validation |
| `--include-source` | Include source snippet in output |
| `--duplicates` | Find duplicate code patterns |
| `--threshold <N>` | Similarity threshold (default: 0.90) |
| `--include-boilerplate` | Include boilerplate in duplicate detection |
| `--kind <KIND>` | Filter by symbol kind |
| `--symbol-scope <SCOPE>` | `functions` (default), `variables`, or `both` |
| `--limit <N>` | Max clusters (default: 50) |
| `--offset <N>` | Pagination offset |
| `--min-lines <N>` | Min function lines to include (default: 3) |
| `--sort-by <FIELD>` | Sort by: `similarity` (default), `size`, or `count` |

### Examples

```bash
# Validate a module (get module name from query overview first)
semfora-engine validate src.commands

# Validate a specific file
semfora-engine validate --file-path ./src/main.rs

# Find duplicates
semfora-engine validate --duplicates

# Lower threshold to find more similar code
semfora-engine validate --duplicates --threshold 0.75

# Validate a specific symbol
semfora-engine validate --symbol-hash abc123
```

---

## `index` — Manage the Index

```
semfora-engine index <SUBCOMMAND>
```

### `index generate [PATH]`

Generate or refresh the semantic index.

```bash
# Index current directory
semfora-engine index generate .

# Index a specific path
semfora-engine index generate /path/to/project

# With progress output
semfora-engine index generate . --progress

# Force full re-index
semfora-engine index generate . --force

# Incremental (only changed files)
semfora-engine index generate . --incremental

# Filter by extension
semfora-engine index generate . --ext rs --ext ts

# Limit depth
semfora-engine index generate . --max-depth 5
```

### `index check`

Check if the index is fresh or stale.

```bash
semfora-engine index check
```

### `index export`

Export the index to SQLite.

```bash
semfora-engine index export
semfora-engine index export --output ./my_index.db
```

---

## `cache` — Manage the Cache

```
semfora-engine cache <SUBCOMMAND>
```

```bash
# Show cache info
semfora-engine cache info

# Clear cache for current directory
semfora-engine cache clear

# Prune caches older than 30 days
semfora-engine cache prune 30
```

---

## `serve` — Start the MCP Server

Start the MCP server for AI coding assistants. Communicates via stdio.

```
semfora-engine serve [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `-r, --repo <PATH>` | Repository path to serve (default: current directory) |
| `--no-watch` | Disable file watcher for live index updates |
| `--no-git-poll` | Disable git polling for branch/commit changes |

### Examples

```bash
# Serve current directory
semfora-engine serve

# Serve a specific repo
semfora-engine serve --repo /path/to/project

# Without file watching (useful for CI/testing)
semfora-engine serve --repo . --no-watch --no-git-poll
```

### MCP Client Configuration

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "semfora-engine": {
      "type": "stdio",
      "command": "/path/to/semfora-engine",
      "args": ["serve", "--repo", "/path/to/your/project"]
    }
  }
}
```

**Cursor / VS Code / Other MCP clients:**

```json
{
  "mcpServers": {
    "semfora-engine": {
      "command": "/path/to/semfora-engine",
      "args": ["serve", "--repo", "/path/to/your/project"]
    }
  }
}
```

---

## `lint` — Run Linters

Auto-detects and runs available linters for the project.

```bash
# Detect available linters without running
semfora-engine lint --detect-only

# Run linters
semfora-engine lint

# Run in fix mode
semfora-engine lint --mode fix --safe-only

# Run a specific linter
semfora-engine lint --linter clippy

# Typecheck only
semfora-engine lint --mode typecheck
```

---

## `test` — Run Tests

```bash
# Run tests
semfora-engine test

# Discover tests without running
semfora-engine test --discover-only

# Run tests in a specific path
semfora-engine test ./tests/integration
```

---

## `commit` — Prepare Commit Context

Gather semantic context for writing a commit message.

```bash
semfora-engine commit
```

---

## Output Formats

All commands support `--format`:

| Format | Description |
|--------|-------------|
| `text` | Human-readable with visual formatting (default for terminal) |
| `toon` | TOON format — token-efficient for AI consumption |
| `json` | Standard JSON |

```bash
semfora-engine query overview --format json
semfora-engine search "authenticate" --format toon
```

---

## Test File Exclusion

By default, test files are excluded. Use `--allow-tests` to include them.

| Language | Excluded Patterns |
|----------|-------------------|
| Rust | `*_test.rs`, `tests/**` |
| TypeScript/JS | `*.test.ts`, `*.spec.ts`, `__tests__/**` |
| Python | `test_*.py`, `*_test.py`, `tests/**` |
| Go | `*_test.go` |
| Java | `*Test.java`, `*Tests.java` |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Logging verbosity (e.g., `RUST_LOG=semfora_engine=debug`) |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | File not found or IO error |
| 2 | Unsupported language |
| 3 | Parse failure |
| 4 | Semantic extraction or query error |
| 5 | Git error (not a git repo, etc.) |

---

## Typical Workflows

### Full Project Analysis

```bash
# 1. Generate index
cd my-project
semfora-engine index generate . --progress

# 2. Get architecture overview
semfora-engine query overview

# 3. Search for specific functionality
semfora-engine search "authenticate" --kind fn

# 4. Trace a symbol
semfora-engine trace <hash> --direction incoming

# 5. Validate code quality
semfora-engine validate src.api

# 6. Check for duplicates
semfora-engine validate --duplicates
```

### Code Review

```bash
# Analyze PR changes
semfora-engine analyze --diff origin/main

# Focus on specific file types
semfora-engine analyze --diff origin/main --ext ts --ext tsx

# Summary only (fast overview)
semfora-engine analyze --diff origin/main --summary-only
```

## See Also

- [Quick Start](quickstart.md) — Get up and running in 5 minutes
- [Features](features.md) — Incremental indexing, layered indexes, risk assessment
- [MCP Tools Reference](mcp-tools-reference.md) — All 18 MCP tools with parameters
- [WebSocket Daemon](websocket-daemon.md) — Real-time updates via WebSocket
