# Quick Start Guide

Get up and running with Semfora Engine in 5 minutes.

## Prerequisites

- [Rust toolchain](https://rustup.rs) (1.70+)
- A C compiler (`gcc`, `clang`, or [Zig](https://ziglang.org) as a drop-in)
- Git

## Installation

```bash
# Clone the repository
git clone https://github.com/Semfora-AI/semfora-engine.git
cd semfora-engine

# Build release binaries
cargo build --release

# (Optional) Add to PATH
export PATH="$PATH:$(pwd)/target/release"
```

The build produces these binaries in `target/release/`:

| Binary | Purpose |
|--------|---------|
| `semfora-engine` | Main CLI: analysis, indexing, querying, and MCP server |
| `semfora-daemon` | WebSocket daemon for real-time index updates |
| `semfora-benchmark-builder` | Benchmark tooling |
| `semfora-security-compiler` | Security pattern compiler |

> **Note:** There is no separate `semfora-engine-server` binary. The MCP server
> is built into `semfora-engine` via the `serve` subcommand.

## Step 1: Index a Repository

Navigate to any git repository and create a semantic index:

```bash
cd /path/to/your/project

# Generate index (writes to ~/.cache/semfora/)
semfora-engine index generate .

# With progress output
semfora-engine index generate . --progress

# Incremental: only re-index changed files
semfora-engine index generate . --incremental
```

This creates a semantic index with:
- Repository overview
- Per-module symbol data
- Call graph relationships
- BM25 full-text index for hybrid search

## Step 2: Search for Code

```bash
# Hybrid search (symbol + semantic, default)
semfora-engine search "authenticate"

# Symbol name matches only
semfora-engine search "handleRequest" --symbols

# Semantic / related code only
semfora-engine search "user authentication" --related

# Filter by symbol kind
semfora-engine search "handle" --kind fn

# Filter by risk level
semfora-engine search "process" --risk high

# Filter by module
semfora-engine search "login" --module auth
```

## Step 3: Query the Index

```bash
# High-level architecture summary
semfora-engine query overview

# Get a specific module's details
semfora-engine query module <module_name>

# Get a specific symbol by hash
semfora-engine query symbol <hash>

# Get source code for a symbol
semfora-engine query source <hash>

# Find callers of a symbol (reverse call graph)
semfora-engine query callers <hash>

# Get the full call graph
semfora-engine query callgraph

# List all symbols in a file
semfora-engine query file ./src/main.rs

# List supported languages
semfora-engine query languages
```

## Step 4: Analyze Code

```bash
# Analyze a single file
semfora-engine analyze path/to/file.rs

# Analyze a directory
semfora-engine analyze ./src

# Analyze uncommitted changes (working directory vs HEAD)
semfora-engine analyze --uncommitted

# Diff against main branch
semfora-engine analyze --diff main

# Diff against a specific commit
semfora-engine analyze --diff origin/main

# Analyze a specific commit
semfora-engine analyze --commit abc123
```

## Step 5: Validate Code Quality

```bash
# Validate a module (get module name from query overview first)
semfora-engine validate <module_name>

# Validate a specific file
semfora-engine validate --file-path ./src/main.rs

# Find duplicate code across the codebase
semfora-engine validate --duplicates

# Validate a specific symbol
semfora-engine validate --symbol-hash <hash>
```

## MCP Server for AI Agents

The MCP server runs as a subcommand and communicates via stdio (standard for MCP):

### Starting the Server

```bash
# Serve current directory
semfora-engine serve

# Serve a specific repository
semfora-engine serve --repo /path/to/project

# Without file watching (useful for CI/testing)
semfora-engine serve --repo . --no-watch --no-git-poll
```

### Configuring with Claude Desktop

Add to your Claude Desktop MCP config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "semfora-engine": {
      "type": "stdio",
      "command": "/path/to/semfora-engine/target/release/semfora-engine",
      "args": ["serve", "--repo", "/path/to/your/project"],
      "env": {
        "RUST_LOG": "semfora_engine=info"
      }
    }
  }
}
```

### Configuring with Other MCP Clients (VS Code, Cursor, etc.)

```json
{
  "mcpServers": {
    "semfora-engine": {
      "command": "/path/to/semfora-engine/target/release/semfora-engine",
      "args": ["serve", "--repo", "/path/to/your/project"]
    }
  }
}
```

### Available MCP Tools

Once connected, the AI has access to 18 tools:

| Tool | Description |
|------|-------------|
| `get_context` | Git context and index status (~200 tokens; use first) |
| `get_overview` | Repository architecture overview |
| `search` | Hybrid symbol + semantic search |
| `analyze` | Semantic analysis of file, directory, or module |
| `analyze_diff` | Analyze git changes / PR diffs |
| `get_file` | List symbols in a file |
| `get_symbol` | Detailed symbol information |
| `get_source` | Source code for symbol(s) or line range |
| `get_callers` | Reverse call graph (impact analysis) |
| `get_callgraph` | Full call graph |
| `validate` | Quality audit (complexity, duplicates) |
| `find_duplicates` | Duplicate code detection |
| `index` | Refresh/check the semantic index |
| `test` | Run or discover tests |
| `lint` | Run linters (auto-detects available tools) |
| `server_status` | Server status and layer info |
| `prep_commit` | Prepare commit message context |
| `get_languages` | List supported languages |

See [MCP Tools Reference](mcp-tools-reference.md) for full parameter documentation.

## Cache Management

```bash
# Show cache info
semfora-engine cache info

# Clear cache for current directory
semfora-engine cache clear

# Prune old caches (older than 30 days)
semfora-engine cache prune 30
```

## Output Formats

All commands support `--format`:

```bash
# Default: human-readable text
semfora-engine query overview

# TOON format (token-efficient for AI consumption)
semfora-engine query overview --format toon

# JSON format
semfora-engine query overview --format json
```

## Common Workflows

### Code Review

```bash
# 1. Index the repository (if not already done)
semfora-engine index generate .

# 2. Analyze the PR diff
semfora-engine analyze --diff origin/main

# 3. Find high-risk changes
semfora-engine search "process" --risk high
```

### Codebase Exploration

```bash
# 1. Get overview
semfora-engine query overview

# 2. Explore a specific module
semfora-engine query module src.api

# 3. Search for functionality
semfora-engine search "authentication"

# 4. Get details on a symbol
semfora-engine query symbol <hash>

# 5. Find what calls it
semfora-engine query callers <hash>
```

### Tracing Symbol Usage

```bash
# Trace a symbol through the call graph (incoming + outgoing)
semfora-engine trace <hash_or_name>

# Only incoming calls
semfora-engine trace <hash> --direction incoming

# Only outgoing calls, 3 levels deep
semfora-engine trace <hash> --direction outgoing --depth 3
```

## Troubleshooting

### "No index found"

```bash
semfora-engine index generate .
```

### Index is stale

```bash
semfora-engine index generate . --incremental
```

### Check index freshness

```bash
semfora-engine index check
```

### View cache info

```bash
semfora-engine cache info
```

## Next Steps

- [CLI Reference](cli.md) — Full command and option documentation
- [MCP Tools Reference](mcp-tools-reference.md) — All 18 MCP tools with parameters
- [Features](features.md) — Incremental indexing, layered indexes, risk assessment
- [Adding Languages](adding-languages.md) — Extend language support
