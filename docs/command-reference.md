# Semfora Engine Command Reference

This document provides a comprehensive reference of all CLI commands and MCP tools available in semfora-engine.

## CLI Commands

### Top-Level Commands

| Command | Description |
|---------|-------------|
| `semfora-engine analyze` | Analyze files, directories, or git changes |
| `semfora-engine search` | Search for code (runs both symbol and semantic search by default) |
| `semfora-engine query` | Query the semantic index (symbols, source, callers, etc.) |
| `semfora-engine validate` | Run quality audits (complexity, duplicates) |
| `semfora-engine index` | Manage the semantic index |
| `semfora-engine cache` | Manage the cache |
| `semfora-engine lint` | Run linters and code quality tools (auto-detects language) |
| `semfora-engine security` | Security scanning and CVE detection |
| `semfora-engine test` | Run or detect tests |
| `semfora-engine commit` | Prepare information for writing a commit message |
| `semfora-engine setup` | Setup semfora-engine installation and MCP client configuration |
| `semfora-engine uninstall` | Uninstall semfora-engine or remove MCP configurations |
| `semfora-engine config` | Manage semfora-engine configuration |
| `semfora-engine benchmark` | Run token efficiency benchmark |

### Query Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine query overview` | Get repository overview |
| `semfora-engine query module <NAME>` | Get a specific module's details |
| `semfora-engine query symbol <HASH>` | Get a specific symbol by hash |
| `semfora-engine query source <FILE>` | Get source code for a file or symbol |
| `semfora-engine query callers <HASH>` | Get callers of a symbol (reverse call graph) |
| `semfora-engine query callgraph` | Get the call graph |
| `semfora-engine query file <PATH>` | Get all symbols in a file |
| `semfora-engine query languages` | List supported languages |

### Index Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine index generate [PATH]` | Generate or refresh the semantic index |
| `semfora-engine index check` | Check if the index is fresh or stale |
| `semfora-engine index export [PATH]` | Export the index to SQLite |

### Cache Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine cache info` | Show cache information |
| `semfora-engine cache clear` | Clear the cache for the current directory |
| `semfora-engine cache prune <DAYS>` | Prune caches older than N days |

### Security Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine security scan` | Scan for CVE vulnerability patterns |
| `semfora-engine security update` | Update security patterns from pattern server |
| `semfora-engine security stats` | Show security pattern statistics |

### Config Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine config show` | Show current configuration |
| `semfora-engine config set <KEY> <VALUE>` | Set a configuration value |
| `semfora-engine config reset` | Reset configuration to defaults |

### Lint Subcommands

| Command | Description |
|---------|-------------|
| `semfora-engine lint detect` | Detect available linters for the project |
| `semfora-engine lint scan` | Run linters and report issues |
| `semfora-engine lint fix` | Apply automatic fixes (dry-run by default) |
| `semfora-engine lint recommend` | Get linter recommendations for the project |

**Supported Linters (26 linters across 16 languages):**

| Category | Languages | Linters |
|----------|-----------|---------|
| Core 4 | Rust | Clippy, rustfmt |
| | JavaScript/TypeScript | ESLint, Biome, Prettier, TSC, Oxlint |
| | Python | Ruff, Black, Mypy, Pylint |
| | Go | golangci-lint, gofmt, go vet |
| JVM | Java | Checkstyle, SpotBugs, PMD |
| | Kotlin | detekt, ktlint |
| Systems | C/C++ | clang-tidy, cppcheck, cpplint |
| | C# | dotnet-format, Roslyn, StyleCop |
| Web | HTML | HTMLHint, html-validate |
| | CSS/SCSS | Stylelint |
| Config | JSON/YAML/TOML/XML | jsonlint, yamllint, taplo, xmllint |
| Infrastructure | Terraform | TFLint, terraform validate/fmt |
| | Shell | ShellCheck, shfmt |
| Documentation | Markdown | markdownlint |

**Lint Options:**

| Option | Description |
|--------|-------------|
| `--linter <NAME>` | Force a specific linter (e.g., `clippy`, `eslint`, `ruff`) |
| `--limit <N>` | Maximum issues to return (default: 100) |
| `--severity-filter <LEVEL>` | Filter by severity: `error`, `warning`, `info` |
| `--fixable-only` | Only show issues that can be auto-fixed |
| `--safe-only` | Only apply safe auto-fixes (with `fix` subcommand) |
| `--dry-run` | Show what would be fixed without making changes |

---

## MCP Tools

These tools are available when using semfora-engine as an MCP server for AI agents.

### Context & Overview

| Tool | Description |
|------|-------------|
| `get_context` | Get quick git and project context (~200 tokens). Use this FIRST when starting work on a repository to understand: current branch, last commit, index status, and project type. |
| `get_overview` | Get the repository overview from a pre-built sharded index. Returns a compact summary with framework detection, module list, risk breakdown, and entry points. |
| `server_status` | Get server status including mode, features, and optionally detailed layer status. |

### Search & Query

| Tool | Description |
|------|-------------|
| `search` | Unified search - runs BOTH symbol and semantic search by default (hybrid mode). Returns symbol matches AND conceptually related code in one call. Use `mode='symbols'` for exact name match, `mode='semantic'` for BM25 conceptual search, or `mode='raw'` for regex patterns. |
| `get_symbol` | Get detailed semantic information for symbol(s). Supports single hash, batch hashes (max 20), or file+line location. Returns complete semantic summaries including calls, state changes, and control flow. |
| `get_source` | Get source code for symbol(s) or line range. Three modes: batch (hashes array), single hash, or file+lines. Returns code snippets with context lines. |
| `get_file` | Get symbols from a file or module (mutually exclusive). Use `file_path` for file-centric view, or `module` for module-centric view. Returns lightweight index entries with optional source snippets. |
| `get_languages` | Get all programming languages supported by semfora-engine for semantic analysis. |

### Analysis

| Tool | Description |
|------|-------------|
| `analyze` | Unified analysis: auto-detects file, directory, or module. For files: extracts semantic info. For directories: returns overview with module grouping. For modules: returns detailed semantic info from index. |
| `analyze_diff` | Use for code reviews - analyzes changes between git branches or commits semantically. Shows new/modified symbols, changed dependencies, and risk assessment. Use `target_ref='WORKING'` to review uncommitted changes. |
| `get_callgraph` | Understand code flow and dependencies between functions. Use with filters (module, symbol) for targeted analysis. Returns a mapping of symbol → [called symbols]. Set `export='sqlite'` to export to database. |
| `get_callers` | Use before modifying existing code to understand impact radius. Answers 'what functions call this symbol?' Shows what will break if you change this function. Returns direct callers and optionally transitive callers (up to depth 3). |

### Quality & Validation

| Tool | Description |
|------|-------------|
| `validate` | Unified quality audit - validates complexity, duplicates, and impact radius. Auto-detects scope: provide `symbol_hash` OR `file_path+line` for single symbol, `file_path` alone for file, or `module` for module-level validation. |
| `find_duplicates` | Unified duplicate detection. Codebase scan (default): Find all duplicate clusters for health audits. Single symbol check: Pass `symbol_hash` to check one symbol before writing new code. Output is token-optimized with grouping by module. |

### Index Management

| Tool | Description |
|------|-------------|
| `index` | Unified index management - smart refresh by default (checks freshness, rebuilds only if stale). Use `force=true` to always regenerate. Returns index status and statistics. |

### Testing

| Tool | Description |
|------|-------------|
| `test` | Unified test runner - runs tests by default (auto-detects framework). Use `detect_only=true` to only detect available test frameworks without running. |

### Linting

| Tool | Description |
|------|-------------|
| `lint` | Unified linter - auto-detects and runs linters across 16 languages (26 linters). Use `detect_only=true` to list available linters, `mode="fix"` to auto-fix. Supports: Rust, JS/TS, Python, Go, Java, Kotlin, C/C++, C#, HTML, CSS, JSON, YAML, TOML, XML, Terraform, Shell, Markdown. |

### Security

| Tool | Description |
|------|-------------|
| `security` | Unified security tool - scans for CVE vulnerability patterns by default. Use `stats_only=true` for pattern statistics, `update=true` to update patterns from remote source. Matches function signatures against pre-compiled fingerprints from NVD/GHSA data. |

### Commit Preparation

| Tool | Description |
|------|-------------|
| `prep_commit` | Prepare information for writing a commit message. Gathers git context, analyzes staged and unstaged changes semantically, and returns a compact summary with optional complexity metrics. This tool NEVER commits - it only provides information. |

---

## Quick Reference by Use Case

| Use Case | CLI Command | MCP Tool |
|----------|-------------|----------|
| Start exploring a codebase | `semfora-engine query overview` | `get_context` → `get_overview` |
| Find a function/symbol | `semfora-engine search "query"` | `search` |
| Get symbol details | `semfora-engine query symbol <hash>` | `get_symbol` |
| Read source code | `semfora-engine query source <file>` | `get_source` |
| Analyze a file | `semfora-engine analyze <file>` | `analyze` |
| Review PR/changes | `semfora-engine analyze --diff main` | `analyze_diff` |
| Find what calls a function | `semfora-engine query callers <hash>` | `get_callers` |
| Find duplicates | `semfora-engine validate --duplicates` | `find_duplicates` |
| Check code quality | `semfora-engine validate <target>` | `validate` |
| Run linter | `semfora-engine lint scan` | `lint` |
| Auto-fix lint issues | `semfora-engine lint fix` | `lint` (with `mode="fix"`) |
| Detect available linters | `semfora-engine lint detect` | `lint` (with `detect_only=true`) |
| Run tests | `semfora-engine test` | `test` |
| Security scan | `semfora-engine security scan` | `security` |
| Prepare commit message | `semfora-engine commit` | `prep_commit` |
| Generate/refresh index | `semfora-engine index generate` | `index` |
| Check index freshness | `semfora-engine index check` | `index` (with auto-check) |

---

## Global Options

All CLI commands support these global options:

| Option | Description |
|--------|-------------|
| `-f, --format <FORMAT>` | Output format: `text` (default), `toon` (token-efficient), `json` |
| `-v, --verbose` | Show verbose output |
| `--progress` | Show progress percentage during long operations |
| `-h, --help` | Print help information |
| `-V, --version` | Print version |
