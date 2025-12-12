# Semfora CLI Reference

The `semfora-mcp` CLI is a semantic code analyzer that produces compressed TOON output for AI-assisted code review.

## Quick Start

```bash
# Build
cargo build --release

# Index a project
semfora-mcp --dir /path/to/project --shard

# Search for symbols
semfora-mcp --search-symbols "authenticate"

# Analyze uncommitted changes
semfora-mcp --uncommitted
```

## Installation

```bash
cargo build --release
# Binary: target/release/semfora-mcp
```

## Basic Usage

```bash
# Analyze a single file
semfora-mcp path/to/file.rs

# Analyze a directory (recursive)
semfora-mcp --dir path/to/project

# Analyze uncommitted changes
semfora-mcp --uncommitted

# Diff against main branch
semfora-mcp --diff
```

## Operation Modes

### Single File Analysis

```bash
semfora-mcp path/to/file.rs
semfora-mcp path/to/file.ts --format json
```

### Directory Analysis

```bash
# Analyze all files in a directory
semfora-mcp --dir ./src

# Limit recursion depth (default: 10)
semfora-mcp --dir ./src --max-depth 5

# Filter by file extension
semfora-mcp --dir ./src --ext rs --ext ts

# Include test files (excluded by default)
semfora-mcp --dir ./src --allow-tests

# Summary statistics only
semfora-mcp --dir ./src --summary-only
```

### Git Diff Analysis

```bash
# Diff against auto-detected base branch (main/master)
semfora-mcp --diff

# Diff against a specific branch
semfora-mcp --diff develop

# Explicit base branch
semfora-mcp --diff --base origin/main

# Analyze uncommitted changes (working directory vs HEAD)
semfora-mcp --uncommitted

# Analyze a specific commit
semfora-mcp --commit abc123

# Analyze all commits since base branch
semfora-mcp --commits
```

## Output Formats

```bash
# TOON format (default) - token-efficient for AI consumption
semfora-mcp file.rs --format toon

# JSON format - standard structured output
semfora-mcp file.rs --format json

# Verbose output with AST info
semfora-mcp file.rs --verbose

# Print parsed AST (debugging)
semfora-mcp file.rs --print-ast
```

## Sharded Indexing

For large repositories, create a sharded index for fast querying:

### Generate Index

```bash
# Generate sharded index (writes to ~/.cache/semfora-mcp/)
semfora-mcp --dir . --shard

# Incremental indexing (only re-index changed files)
semfora-mcp --dir . --shard --incremental

# Filter extensions during indexing
semfora-mcp --dir . --shard --ext ts --ext tsx
```

### Query Index

```bash
# Get repository overview
semfora-mcp --get-overview

# List all modules in the index
semfora-mcp --list-modules

# Get a specific module's symbols
semfora-mcp --get-module api

# Search for symbols by name
semfora-mcp --search-symbols "login"

# List all symbols in a module
semfora-mcp --list-symbols auth

# Get a specific symbol by hash
semfora-mcp --get-symbol abc123def456

# Get the call graph
semfora-mcp --get-call-graph
```

### Query Filtering

```bash
# Filter by symbol kind
semfora-mcp --search-symbols "handle" --kind fn

# Filter by risk level
semfora-mcp --list-symbols api --risk high

# Limit results (default: 50)
semfora-mcp --search-symbols "test" --limit 20
```

## Cache Management

```bash
# Show cache information
semfora-mcp --cache-info

# Clear cache for current directory
semfora-mcp --cache-clear

# Prune caches older than N days
semfora-mcp --cache-prune 30
```

## Static Analysis

```bash
# Run static code analysis on the index
semfora-mcp --analyze

# Analyze a specific module only
semfora-mcp --analyze --analyze-module api
```

## Token Analysis

Analyze token efficiency of TOON compression:

```bash
# Full detailed report
semfora-mcp file.rs --analyze-tokens full

# Compact single-line summary
semfora-mcp file.rs --analyze-tokens compact

# Include compact JSON comparison
semfora-mcp file.rs --analyze-tokens full --compare-compact
```

## Benchmarking

```bash
# Run token efficiency benchmark
semfora-mcp --benchmark
```

## Test File Exclusion

By default, test files are excluded from analysis. Test patterns by language:

| Language | Excluded Patterns |
|----------|-------------------|
| Rust | `*_test.rs`, `tests/**` |
| TypeScript/JS | `*.test.ts`, `*.spec.ts`, `__tests__/**` |
| Python | `test_*.py`, `*_test.py`, `tests/**` |
| Go | `*_test.go` |
| Java | `*Test.java`, `*Tests.java` |

Use `--allow-tests` to include test files.

## Directory for Index Queries

When using query commands (`--get-overview`, `--search-symbols`, etc.), the CLI uses the cache for the current working directory. The cache location is determined by the git remote URL hash for reproducibility.

## Examples

### Typical Workflow

```bash
# 1. Generate index for a project
cd my-project
semfora-mcp --dir . --shard

# 2. Get project overview
semfora-mcp --get-overview

# 3. Search for specific functionality
semfora-mcp --search-symbols "authenticate" --kind fn

# 4. Get details on a symbol
semfora-mcp --get-symbol abc123def456

# 5. Analyze changes before commit
semfora-mcp --uncommitted

# 6. Analyze feature branch diff
semfora-mcp --diff main
```

### Code Review Workflow

```bash
# Analyze PR changes
semfora-mcp --diff origin/main

# Focus on specific file types
semfora-mcp --diff origin/main --ext ts --ext tsx

# Get summary only
semfora-mcp --diff origin/main --summary-only
```

## Environment Variables

- `RUST_LOG`: Control logging verbosity (e.g., `RUST_LOG=semfora_mcp=debug`)

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | File not found or IO error |
| 2 | Unsupported language |
| 3 | Parse failure |
| 4 | Semantic extraction or query error |
| 5 | Git error (not a git repo, etc.) |

## See Also

- [WebSocket Daemon](websocket-daemon.md) - Real-time index updates via WebSocket
- [Main README](../README.md) - Supported languages and architecture
