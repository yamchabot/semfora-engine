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
| `semfora-engine` | CLI for semantic analysis, indexing, and querying |
| `semfora-engine-server` | MCP server for AI agent integration |
| `semfora-daemon` | WebSocket daemon for real-time updates |

## Usage

```bash
# Analyze a single file
semfora-engine path/to/file.rs

# Analyze a directory and create sharded index
semfora-engine --dir . --shard

# Query the index
semfora-engine --search-symbols "authenticate"

# Start MCP server (for AI agents)
semfora-engine-server

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

## Duplicate Detection & Boilerplate Patterns

Semfora Engine includes semantic duplicate detection that identifies similar functions while filtering out expected boilerplate patterns.

### Current Boilerplate Coverage

| Language | Patterns | Status |
|----------|----------|--------|
| **JavaScript/TypeScript** | 14 patterns | Full support |
| **Rust** | 13 patterns | Full support |
| **C#** | 18 patterns | Full support |
| **Python** | 0 patterns | Planned |
| **Go** | 0 patterns | Planned |
| **Java** | 0 patterns | Planned |
| **C/C++** | 0 patterns | Planned |

### JavaScript/TypeScript Patterns (14)
- ReactQuery, ReactHook, EventHandler, ApiRoute, TestSetup, TypeGuard
- ConfigExport, ReduxPattern, ValidationSchema, TestMock, NextjsDataFetching
- ReactWrapper, ClassicReduxReducer, ApiWrapper

### Rust Patterns (13)
- RustTraitImpl, RustBuilder, RustGetter, RustSetter, RustConstructor
- RustConversion, RustDerived, RustErrorFrom, RustIterator, RustDeref
- RustDrop, RustTest, RustSerde

### C# Patterns (18)
- **ASP.NET Core**: AspNetController, AspNetMinimalApi, AspNetMiddleware, AspNetDI
- **Entity Framework**: EFDbContext, EFDbSet, EFFluentApi, EFMigration
- **Testing**: XUnitTest, NUnitTest, MoqSetup
- **LINQ**: LinqChain, LinqProjection
- **Unity**: UnityLifecycle, UnitySerializedField, UnityScriptableObject
- **General**: CSharpProperty, CSharpRecord

### Planned Boilerplate Patterns

| Language | Planned Patterns | Priority |
|----------|------------------|----------|
| **Python** | pytest fixtures, dataclasses, FastAPI/Flask routes, Pydantic models, Django views | High |
| **Go** | HTTP handlers, middleware, error wrapping, builder structs, test helpers | High |
| **Java** | Spring controllers/services, Lombok-generated, DTOs, JPA entities, JUnit tests | High |
| **C/C++** | Getters/setters, RAII wrappers, copy/move boilerplate, operator overloads | Medium |
| **Kotlin** | Data classes, Spring Boot, Ktor routing, Android ViewModel, coroutines | High |

## Language Roadmap

Prioritized based on enterprise adoption potential and architectural fit with Semfora's orchestrator-first design.

### C# and .NET Ecosystem (COMPLETE)

Full C# support implemented with 18 boilerplate patterns covering ASP.NET Core, Entity Framework, Unity, LINQ, and testing frameworks. See [C# Patterns](#c-patterns-18) above.

### HCL/Terraform (COMPLETE)

Full HCL support implemented for infrastructure-as-code analysis (`.tf`, `.hcl`, `.tfvars`).

### Priority 1: Kotlin

Complements Java support for Android and modern JVM coverage.

| Item | Details |
|------|---------|
| **Extensions** | `.kt`, `.kts` |
| **Parser** | `tree-sitter-kotlin` (already integrated) |
| **Targets** | Data classes, coroutines, extension functions, sealed classes |

**Framework Patterns**: Spring Boot, Ktor routing, Android ViewModel + LiveData, coroutine scopes

### Priority 2: Swift

Unlocks iOS/macOS and SwiftUI ecosystems.

| Item | Details |
|------|---------|
| **Extensions** | `.swift` |
| **Parser** | `tree-sitter-swift` |
| **Targets** | Struct vs class semantics, protocol conformance, property wrappers, async/await |

### Priority 3: PHP

High ROI due to extreme boilerplate density in Laravel/WordPress codebases.

| Item | Details |
|------|---------|
| **Extensions** | `.php` |
| **Parser** | `tree-sitter-php` |
| **Targets** | Laravel controllers, service providers, middleware, Eloquent models |

### Priority 4: Ruby

Smaller but relevant via Rails ecosystem.

| Item | Details |
|------|---------|
| **Extensions** | `.rb` |
| **Parser** | `tree-sitter-ruby` |
| **Targets** | ActiveRecord models, Rails controllers, RSpec scaffolding |

### Priority 5: Infra Languages (Non-Semantic)

Structural parsing for repo comprehension without full semantic analysis.

| Language | Extensions | Mode |
|----------|------------|------|
| PowerShell | `.ps1` | Parser-only |
| Dockerfile | `Dockerfile` | Structural |
| Makefile | `Makefile` | Structural |

## Known Unsupported Formats

These formats were identified in test repositories but are not currently supported:

| Format | Extensions | Count* | Reason |
|--------|------------|--------|--------|
| **Jest Snapshots** | `.shot` | 5,140 | Test artifacts, not semantic code |
| **MDX** | `.mdx` | 861 | Documentation format (Markdown + JSX) |
| **AsciiDoc** | `.adoc` | 690 | Documentation format |
| **Protocol Buffers** | `.proto`, `.pb` | 550 | `devgen-tree-sitter-protobuf` requires tree-sitter 0.21 (incompatible) |
| **Scala** | `.scala` | varies | Complex AST, only if enterprise demand |
| **Elixir** | `.ex`, `.exs` | varies | Lower priority |

*Counts from typescript-eslint, terraform, spring-framework, and prometheus test repositories.

## Architecture

```
src/
├── main.rs              # CLI entry point (semfora-engine binary)
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
├── mcp_server/          # MCP server (semfora-engine-server binary)
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
| [Quick Start](docs/quickstart.md) | Get up and running in 5 minutes |
| [CLI Reference](docs/cli.md) | Complete CLI usage, flags, and examples |
| [Features](docs/features.md) | Incremental indexing, layered indexes, risk assessment |
| [WebSocket Daemon](docs/websocket-daemon.md) | Real-time updates, protocol, and query methods |
| [Adding Languages](docs/adding-languages.md) | Guide for adding new language support |
| [Engineering](docs/engineering.md) | Implementation details and status |

## License

MIT
