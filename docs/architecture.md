# Semfora Engine Architecture

> Consolidated architecture document combining original vision with current implementation.
> For historical context, see `docs/archive/2024-12-original/`.

---

## Overview

Semfora Engine is a semantic code analysis system that produces compressed TOON (Text Object-Oriented Notation) output for AI-assisted code review. It extracts symbols, dependencies, control flow, state changes, and risk assessments from source files.

**Core Value Proposition**: 70%+ token reduction through semantic compression, enabling efficient AI code analysis without reading raw source files.

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    External Consumers                            │
│  Claude Code  │  Cursor  │  semfora-cli  │  CI Pipelines        │
└─────────────────────────────────────────────────────────────────┘
                              │ MCP Protocol
┌─────────────────────────────────────────────────────────────────┐
│                 SEMFORA ENGINE (This Repository)                 │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │ MCP Server   │  │   CLI        │  │  Daemon      │           │
│  │ (bin)        │  │   (bin)      │  │  (WebSocket) │           │
│  └──────────────┘  └──────────────┘  └──────────────┘           │
│         │                │                  │                    │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Core Library                           │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐         │   │
│  │  │Extract  │ │ Shard   │ │ Cache   │ │  Git    │         │   │
│  │  │(AST)    │ │(Index)  │ │(Storage)│ │(Diff)   │         │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘         │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐         │   │
│  │  │Detectors│ │ TOON    │ │ Search  │ │  Risk   │         │   │
│  │  │(Lang)   │ │(Encode) │ │(BM25)   │ │(Score)  │         │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘         │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Binaries

| Binary | Purpose | Entry Point |
|--------|---------|-------------|
| `semfora-engine` | CLI for analysis, indexing, querying | `src/main.rs` |
| `semfora-engine-server` | MCP server for AI agent integration | `src/mcp_server/bin.rs` |
| `semfora-daemon` | WebSocket daemon for real-time updates | `src/socket_server/` |

---

## Core Modules

### Semantic Extraction (`src/extract.rs`, `src/detectors/`)

Tree-sitter based AST traversal for 20+ languages.

| Component | File | Purpose |
|-----------|------|---------|
| Extraction Engine | `extract.rs` | Orchestrates language-specific extraction |
| JavaScript Family | `detectors/javascript/` | TS, TSX, JS, JSX with framework detection |
| Rust | `detectors/rust.rs` | Full Rust extraction |
| Python | `detectors/python.rs` | Python with decorator support |
| C# | `detectors/csharp.rs` | Full C# with async/await, records, pattern matching |
| Go | `detectors/go.rs` | Go with methods and structs |
| HCL/Terraform | `detectors/hcl.rs` | Infrastructure-as-code extraction |
| Java/Kotlin/C/C++ | `detectors/*.rs` | Basic extraction |
| Config/Markup | `detectors/config.rs`, `markup.rs` | JSON, YAML, TOML, HTML, CSS, MD |

### Sharded Index (`src/shard.rs`)

Query-driven semantic index for efficient retrieval.

```
~/.cache/semfora/{repo-hash}/
├── repo_overview.toon        # Architecture summary (~150KB max)
├── symbol_index.jsonl        # Lightweight search index (streamable)
├── modules/
│   └── {module}.toon         # Per-module semantic slices
├── symbols/
│   └── {hash}.toon           # Individual symbol details
└── graphs/
    ├── call_graph.toon       # Function relationships
    └── import_graph.toon     # Module dependencies
```

**Key Design**: Symbol index entries are ~100 bytes each, enabling O(1) memory per query even for 600k+ symbol repos.

### MCP Server (`src/mcp_server/`)

30+ tools exposed via MCP protocol for AI agents.

**Query-Driven Tools (Preferred)**:
| Tool | Token Cost | Use Case |
|------|-----------|----------|
| `search_symbols` | ~400/20 results | Find symbols by name |
| `list_symbols` | ~800/50 results | Browse module contents |
| `get_symbol` | ~350 | Detailed semantic info |
| `get_symbol_source` | ~400/50 lines | Actual source code |
| `get_repo_overview` | ~500 | Architecture summary |
| `get_callers` | ~500 | Reverse call graph |
| `semantic_search` | ~800 | Conceptual/BM25 search |

**Expensive Tools (Use Sparingly)**:
| Tool | Token Cost | Notes |
|------|-----------|-------|
| `get_module` | 8,000-12,000 | Prefer `list_symbols` + `get_symbol` |
| `analyze_directory` | Unbounded | Use `generate_index` instead |

### Risk Scoring (`src/risk.rs`)

Point-based behavioral risk calculation:
- +1 per import
- +1 per state variable
- +2 per control flow change
- +2 for I/O/network calls
- +3 for public API changes
- +3 for persistence operations

Levels: `low` (0-1), `medium` (2-3), `high` (4+)

### TOON Encoding (`src/toon.rs`)

Compressed semantic notation achieving 70%+ token reduction vs raw source.

```
file: src/auth/login.ts
language: typescript
symbol: handleLogin
symbol_kind: function
behavioral_risk: high

insertions[3]:
  local isLoading state via useState
  network call introduced
  form validation

added_dependencies[2]: react,@/lib/api

state_changes[1]{name,type,init}:
  isLoading,useState,false
```

---

## Token Efficiency Patterns

### Query-Driven Workflow (Required)

```
1. get_repo_overview        → Understand architecture (~500 tokens)
2. search_symbols("query")  → Find relevant symbols (~400 tokens)
3. get_symbol(hash)         → Fetch details for specific symbols (~350 tokens)
4. get_symbol_source(...)   → Get code for editing (~400 tokens)
```

**vs Module-Loading Approach**:
```
get_module("auth")  → 10,000+ tokens (loads everything)
```

**Savings**: 5-10x token reduction per exploration session.

### Recommended MCP Instructions for AI Agents

```markdown
## Semfora Query-Driven Workflow

PREFER query-driven tools:
- search_symbols(query) → list_symbols(module) → get_symbol(hash)
- Use get_symbol_source for actual code only when editing

AVOID expensive operations:
- get_module (use list_symbols + get_symbol instead)
- analyze_directory (use generate_index + get_repo_overview)

Token budget per query:
- search_symbols: ~400 tokens (20 results)
- list_symbols: ~800 tokens (50 results)
- get_symbol: ~350 tokens
- get_symbol_source: ~400 tokens (50 lines)
```

---

## Supported Languages

| Language | Extensions | Extraction Level |
|----------|------------|------------------|
| TypeScript | `.ts`, `.mts`, `.cts` | Full (symbols, imports, state, control flow) |
| TSX | `.tsx` | Full + JSX/React hooks |
| JavaScript | `.js`, `.mjs`, `.cjs` | Full |
| JSX | `.jsx` | Full + JSX |
| Rust | `.rs` | Full |
| Python | `.py`, `.pyi` | Full |
| C# | `.cs` | Full (async/await, records, pattern matching) |
| Go | `.go` | Full (methods, structs, interfaces) |
| HCL/Terraform | `.tf`, `.hcl`, `.tfvars` | Full (blocks, resources, variables) |
| Java | `.java` | Basic |
| Kotlin | `.kt`, `.kts` | Basic |
| C/C++ | `.c`, `.cpp`, `.h`, etc. | Basic |
| HTML/CSS/SCSS | `.html`, `.css`, `.scss` | Structural |
| JSON/YAML/TOML/XML | `.json`, `.yaml`, `.toml`, `.xml` | Config extraction |
| Markdown | `.md` | Structural |
| Vue SFC | `.vue` | Full (script extraction with lang detection) |
| Shell/Bash | `.sh`, `.bash`, `.zsh` | Basic |
| Gradle | `.gradle` | Basic |

### Boilerplate Detection (`src/duplicate/boilerplate/`)

Semantic duplicate detection filters out expected boilerplate patterns.

| Language | Patterns | Coverage |
|----------|----------|----------|
| **JavaScript/TypeScript** | 19 | ReactQuery, ReactHook, EventHandler, ApiRoute, TestSetup, TypeGuard, ConfigExport, ReduxPattern, ValidationSchema, TestMock, NextjsDataFetching, ReactWrapper, ClassicReduxReducer, ApiWrapper, ContextProvider, SimpleContextHook, HOCWrapper, LazyComponent, SuspenseBoundary |
| **Rust** | 13 | TraitImpl, Builder, Getter, Setter, Constructor, Conversion, Derived, ErrorFrom, Iterator, Deref, Drop, Test, Serde |
| **C#** | 18 | ASP.NET (Controller, MinimalApi, Middleware, DI), Entity Framework (DbContext, DbSet, FluentApi, Migration), Testing (XUnit, NUnit, Moq), LINQ (Chain, Projection), Unity (Lifecycle, SerializedField, ScriptableObject), General (Property, Record) |

---

## Data Structures

### SemanticSummary (Core Model)

```rust
pub struct SemanticSummary {
    pub file: String,
    pub language: String,
    pub symbol: Option<String>,
    pub symbol_kind: Option<SymbolKind>,
    pub symbol_id: Option<SymbolId>,        // Hash for lookup
    pub lines: Option<String>,               // Line range
    pub props: Vec<Prop>,
    pub arguments: Vec<Argument>,
    pub return_type: Option<String>,
    pub insertions: Vec<String>,             // Behavioral summaries
    pub added_dependencies: Vec<String>,
    pub state_changes: Vec<StateChange>,
    pub control_flow_changes: Vec<ControlFlowChange>,
    pub calls: Vec<String>,                  // Function calls made
    pub public_surface_changed: bool,
    pub behavioral_risk: RiskLevel,
}
```

### SymbolIndexEntry (Lightweight Search)

```rust
pub struct SymbolIndexEntry {
    pub symbol: String,      // Symbol name
    pub hash: String,        // Lookup hash
    pub kind: String,        // fn, struct, component, etc.
    pub module: String,      // Module grouping
    pub file: String,        // File path
    pub lines: String,       // Line range
    pub risk: String,        // Risk level
}
```

---

## Error Handling

Exit codes for CLI operations:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | File not found / IO error |
| 2 | Unsupported language |
| 3 | Parse failure |
| 4 | Extraction failure |

---

## Future Architecture: ADK Integration

The engine is designed to support a cognitive orchestration layer (Model B architecture):

```
┌──────────────────────────────────────────────────────────────┐
│                    ADK Orchestrator (Python)                  │
│  - Makes ALL tool decisions (not the LLM)                    │
│  - Maintains persistent semantic memory                       │
│  - Manages context budget and token optimization             │
└──────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────┐
│                    Semfora Engine (This Repo)                 │
│  - Provides semantic extraction via MCP                      │
│  - Query-driven access to symbol index                       │
│  - Stateless per-request processing                          │
└──────────────────────────────────────────────────────────────┘
```

**Key Principle**: The orchestrator controls all tool calls; the LLM only reasons about curated semantic context.

---

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Regenerating Index

```bash
semfora-engine --dir . --shard
# or via MCP
generate_index(path=".")
```

---

## References

- Original engineering specs: `docs/archive/2024-12-original/engineering.md`
- Query-driven design: `docs/archive/2024-12-original/query-driven-architecture.md`
- ADK integration vision: `docs/archive/2024-12-original/semfora-agent-architecture.md`
- Token optimization: `docs/archive/2024-12-original/context-optimization.md`
