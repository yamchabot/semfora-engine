# Semfora Engine

Semantic code analyzer that produces compressed TOON (Text Object-Oriented Notation) output for AI-assisted code review. Extracts symbols, dependencies, control flow, state changes, and risk assessments from source files.

> [!IMPORTANT]
> **🚀 Transitioning to Rust-based ADK**
>
> We are moving away from the Python-based `semfora-adk` to a pure Rust implementation in [`semfora-cli`](https://github.com/Semfora-org/semfora-cli). The new Rust ADK provides better performance, single-binary distribution, and tighter integration with the semantic engine.
>
> ```bash
> # Use the new Rust-based CLI agent
> semfora-cli --rust-adk
> ```
>
> See the [semfora-cli repository](https://github.com/Semfora-org/semfora-cli) for installation and usage.

## Installation

```bash
cargo build --release
```

## Binaries

The project builds three binaries:

| Binary | Purpose |
|--------|---------|
| `semfora-mcp` | CLI for semantic analysis, indexing, and querying |
| `semfora-mcp-server` | MCP server for AI agent integration |
| `semfora-daemon` | WebSocket daemon for real-time updates |

## Usage

```bash
# Analyze a single file
semfora-mcp path/to/file.rs

# Analyze a directory and create sharded index
semfora-mcp --dir . --shard

# Query the index
semfora-mcp --search-symbols "authenticate"

# Start MCP server (for AI agents)
semfora-mcp-server

# Start WebSocket daemon (for real-time updates)
semfora-daemon
```

See [CLI Reference](docs/cli.md) for full documentation.

## Supported Languages

### Programming Languages

| Language | Extensions | Family | Implementation Details |
|----------|------------|--------|------------------------|
| **TypeScript** | `.ts`, `.mts`, `.cts` | JavaScript | Full AST extraction via `tree-sitter-typescript`; exports, interfaces, enums, decorators |
| **TSX** | `.tsx` | JavaScript | TypeScript + JSX/React component detection, hooks, styled-components |
| **JavaScript** | `.js`, `.mjs`, `.cjs` | JavaScript | Functions, classes, imports; framework detection for React/Express/Angular |
| **JSX** | `.jsx` | JavaScript | JavaScript + JSX component detection |
| **Rust** | `.rs` | Rust | Functions, structs, traits, enums; `pub` visibility detection via `tree-sitter-rust` |
| **Python** | `.py`, `.pyi` | Python | Functions, classes, decorators; underscore-prefix privacy convention |
| **Go** | `.go` | Go | Functions, methods, structs; uppercase-export convention via `tree-sitter-go` |
| **Java** | `.java` | Java | Classes, interfaces, enums, methods; public/private/protected modifiers |
| **Kotlin** | `.kt`, `.kts` | Kotlin | Classes, functions, objects; visibility modifiers via `tree-sitter-kotlin-ng` |
| **C** | `.c`, `.h` | C Family | Functions, structs, enums; `extern` detection via `tree-sitter-c` |
| **C++** | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh` | C Family | Classes, structs, templates; access specifiers via `tree-sitter-cpp` |
| **Shell/Bash** | `.sh`, `.bash`, `.zsh`, `.fish` | Shell | Function definitions, variable assignments, command calls via `tree-sitter-bash` |
| **Gradle** | `.gradle` | Gradle | Groovy-based build files; closures, method calls via `tree-sitter-groovy` |

### Framework Detection (JavaScript Family)

| Framework | Detection Method | Extracted Information |
|-----------|------------------|----------------------|
| **React** | Import from `react` | Components, hooks (useState, useEffect, etc.), forwardRef, memo |
| **Next.js** | File path patterns (`/app/`, `/pages/`) | API routes, layouts, pages, server/client components |
| **Express** | Import from `express` | Route handlers (GET, POST, etc.), middleware |
| **Angular** | `@Component`, `@Injectable` decorators | Components, services, modules |
| **Vue** | `.vue` files, composition API | SFC script extraction, Options API, Composition API, Pinia stores |

### Markup & Styling

| Language | Extensions | Implementation Details |
|----------|------------|------------------------|
| **HTML** | `.html`, `.htm` | Document structure via `tree-sitter-html` |
| **CSS** | `.css` | Stylesheet detection via `tree-sitter-css` |
| **SCSS/SASS** | `.scss`, `.sass` | Stylesheet detection via `tree-sitter-scss` |
| **Markdown** | `.md`, `.markdown` | Document structure via `tree-sitter-md` |

### Configuration & Data

| Language | Extensions | Implementation Details |
|----------|------------|------------------------|
| **JSON** | `.json` | Structure parsing via `tree-sitter-json` |
| **YAML** | `.yaml`, `.yml` | Structure parsing via `tree-sitter-yaml` |
| **TOML** | `.toml` | Structure parsing via `tree-sitter-toml-ng` |
| **XML** | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.plist`, `.pom` | Structure parsing via `tree-sitter-xml` |
| **HCL/Terraform** | `.tf`, `.hcl`, `.tfvars` | Infrastructure-as-code via `tree-sitter-hcl` |

### Single-File Components

| Format | Extension | Implementation Details |
|--------|-----------|------------------------|
| **Vue SFC** | `.vue` | Extracts `<script>` or `<script setup>` section; detects `lang` attribute (ts/tsx/js); parses with appropriate grammar |

## Known Unsupported Formats

These formats were identified in test repositories but are not currently supported:

| Format | Extensions | Count* | Reason |
|--------|------------|--------|--------|
| **Jest Snapshots** | `.shot` | 5,140 | Test artifacts, not semantic code |
| **MDX** | `.mdx` | 861 | Documentation format (Markdown + JSX) |
| **AsciiDoc** | `.adoc` | 690 | Documentation format |
| **Protocol Buffers** | `.proto`, `.pb` | 550 | `devgen-tree-sitter-protobuf` requires tree-sitter 0.21 (incompatible) |
| **Ruby** | `.rb` | varies | No tree-sitter grammar added yet |
| **Swift** | `.swift` | varies | No tree-sitter grammar added yet |
| **PHP** | `.php` | varies | No tree-sitter grammar added yet |
| **Scala** | `.scala` | varies | No tree-sitter grammar added yet |
| **Elixir** | `.ex`, `.exs` | varies | No tree-sitter grammar added yet |

*Counts from typescript-eslint, terraform, spring-framework, and prometheus test repositories.

## Architecture

```
src/
├── main.rs              # CLI entry point (semfora-mcp binary)
├── cli.rs               # CLI argument definitions
├── lib.rs               # Library exports
├── lang.rs              # Language detection from file extensions
├── extract.rs           # Main extraction orchestration
├── schema.rs            # SemanticSummary output schema
├── toon.rs              # TOON format encoding
├── risk.rs              # Behavioral risk calculation
├── error.rs             # Error types and exit codes
├── cache.rs             # Cache management and querying
├── shard.rs             # Sharded index generation
├── detectors/           # Language-specific extractors
│   ├── javascript/      # JS/TS with framework support
│   │   ├── core.rs      # Core JS/TS extraction
│   │   └── frameworks/  # React, Next.js, Express, Angular, Vue
│   ├── rust.rs
│   ├── python.rs
│   ├── go.rs
│   ├── java.rs
│   ├── kotlin.rs
│   ├── shell.rs
│   ├── gradle.rs
│   ├── c_family.rs
│   ├── markup.rs
│   ├── config.rs
│   ├── grammar.rs       # AST node mappings per language
│   └── generic.rs       # Generic extraction using grammars
├── mcp_server/          # MCP server (semfora-mcp-server binary)
│   ├── mod.rs           # MCP tool handlers
│   └── bin.rs           # Server entry point
└── socket_server/       # WebSocket daemon (semfora-daemon binary)
    ├── mod.rs           # Server architecture
    ├── bin.rs           # Daemon entry point
    ├── connection.rs    # Client connection handling
    ├── protocol.rs      # Message types
    └── repo_registry.rs # Multi-repo context management
```

## Adding a New Language

1. Add tree-sitter grammar to `Cargo.toml`
2. Add `Lang` variant in `lang.rs` with extension mapping
3. Add `LangGrammar` in `detectors/grammar.rs` with AST node mappings
4. (Optional) Create dedicated detector in `detectors/` for special features
5. Wire up in `extract.rs` dispatcher

## Documentation

| Document | Description |
|----------|-------------|
| [Features](docs/features.md) | Incremental indexing, layered indexes, risk assessment, and more |
| [CLI Reference](docs/cli.md) | Complete CLI usage, flags, and examples |
| [WebSocket Daemon](docs/websocket-daemon.md) | Real-time updates, protocol, and query methods |
| [Query-Driven Architecture](docs/query-driven-architecture.md) | Token-efficient querying patterns |
| [Engineering](docs/engineering.md) | Implementation details and status |

## License

MIT
