# Semfora-MCP Engineering Requirements Document

> Companion document to `plan.md` - Implementation specifications for Phase 1

---

## Implementation Status (Updated: 2025-12-02)

### Completed
| Component | File | Status | Notes |
|-----------|------|--------|-------|
| Project setup | `Cargo.toml` | Done | 16 language grammars, clap, serde, rmcp |
| Error handling | `src/error.rs` | Done | Exit codes 0-4 per spec |
| CLI interface | `src/cli.rs` | Done | `--format`, `--verbose`, `--print-ast` |
| Language detection | `src/lang.rs` | Done | 16 languages, families |
| Schema/types | `src/schema.rs` | Done | All structs with serde |
| Risk calculation | `src/risk.rs` | Done | Point-based scoring |
| TOON encoder | `src/toon.rs` | Done | rtoon v0.2.1, TOON spec v3.0 compliant |
| Extraction engine | `src/extract.rs` | Done | Direct AST traversal |
| Library exports | `src/lib.rs` | Done | Public API |
| Entry point | `src/main.rs` | Done | Full CLI |
| **MCP Server** | `src/mcp_server/` | Done | stdio transport, 4 tools |
| **MCP Binary** | `src/mcp_server/bin.rs` | Done | `semfora-mcp-server` binary |
| **Python ADK** | `semfora-adk/` | Done | Model B orchestration, toon-format parser |

### Language Extraction Support
| Language | Symbols | Imports | State | Control Flow | JSX |
|----------|---------|---------|-------|--------------|-----|
| TSX | Full | Full | Hooks | Full | Full |
| TypeScript | Full | Full | - | Full | - |
| JavaScript | Full | Full | Hooks | Full | - |
| JSX | Full | Full | Hooks | Full | Full |
| Rust | Full | Full | Let bindings | Full | - |
| Python | Full | Full | Assignments | Full | - |
| Go | Basic | Full | - | - | - |
| Java | Basic | Full | - | - | - |
| C/C++ | Basic | Includes | - | - | - |
| HTML/CSS/MD | Fallback | - | - | - | - |
| JSON/YAML/TOML | Fallback | - | - | - | - |

### MCP Server Tools
| Tool | Description |
|------|-------------|
| `analyze_file` | Analyze a single source file, returns TOON/JSON summary |
| `analyze_directory` | Analyze entire codebase with repo overview |
| `analyze_diff` | Analyze git diff between branches/commits |
| `list_languages` | List all supported programming languages |

### Pending / Phase 2
- [ ] External `.scm` query files (currently using direct AST traversal)
- [ ] Separate detector modules per language family
- [ ] Function argument extraction
- [ ] Component props extraction
- [ ] More insertion rules
- [ ] Git diff semantic comparison (before/after)
- [x] MCP integration (moved from Phase 4 to Phase 1)

### Test Results

**Rust Engine (semfora-engine):**
```
24 tests passed, 0 failed
- lang::tests (5 tests)
- schema::tests (5 tests)
- risk::tests (4 tests)
- toon::tests (7 tests)
- extract::tests (3 tests)
```

**Python ADK (semfora-adk):**
```
31 tests passed, 0 failed
- test_memory.py (4 tests)
- test_orchestrator.py (6 tests)
- test_tools.py (9 tests)
- test_toon_library.py (12 tests - comparing custom parser vs toon-format)
```

---

## 1. Overview

This document provides concrete engineering specifications for implementing the MCP Semantic Diff & TOON Encoder Phase 1 MVP as described in `plan.md`.

**Goal**: Build `semfora-mcp`, a Rust CLI that parses source files using tree-sitter, extracts semantic information, and outputs TOON-formatted summaries.

---

## 2. Technology Stack

### 2.1 Core Dependencies

```toml
[package]
name = "semfora-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core parsing
tree-sitter = "0.24"          # Pin to 0.24.x for grammar compatibility

# TOON encoding
rtoon = "0.2.1"               # Rust TOON encoder (TOON spec v3.0 compliant)

# Language grammars - Programming languages
tree-sitter-typescript = "0.23"   # Includes TSX
tree-sitter-javascript = "0.23"
tree-sitter-rust = "0.24"
tree-sitter-python = "0.23"
tree-sitter-go = "0.23"
tree-sitter-java = "0.23"
tree-sitter-c = "0.23"
tree-sitter-cpp = "0.23"

# Language grammars - Markup & Config
tree-sitter-html = "0.23"
tree-sitter-css = "0.23"
tree-sitter-json = "0.24"
tree-sitter-yaml = "0.7"
tree-sitter-toml-ng = "0.7"       # Use -ng variant (more maintained)
tree-sitter-md = "0.3"            # Markdown

# CLI & Error handling
clap = { version = "4.5", features = ["derive"] }
thiserror = "1.0"

[build-dependencies]
cc = "*"
```

### 2.2 Python ADK Dependencies

```toml
# semfora-adk/pyproject.toml (uv-managed)
[project]
dependencies = [
    "anthropic>=0.40.0",
    "litellm>=1.0.0",
    "rich>=13.0.0",
    "toon-format",  # TOON parser from GitHub
]
```

The `toon-format` library (v0.9.0b1, 792 tests, 91% coverage) handles all TOON parsing in the Python ADK.

### 2.3 Version Compatibility Notes

- **Critical**: tree-sitter 0.25+ has linker issues when combining multiple grammars. Pin to 0.24.x
- All grammar crates must be compatible with the core tree-sitter version
- TypeScript crate provides two separate grammars: `LANGUAGE_TYPESCRIPT` and `LANGUAGE_TSX`
- **TOON Spec Compliance**: rtoon v0.2.1 and toon-format both implement TOON spec v3.0 (no `---` separators)

---

## 3. Project Structure

```
semfora-mcp/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── cli.rs               # Clap argument definitions
│   ├── error.rs             # Error types and exit codes
│   ├── lang.rs              # Language detection + parser loading
│   ├── schema.rs            # Semantic model data structures
│   ├── extract.rs           # Semantic extraction orchestration
│   ├── detectors/           # Language-specific extraction
│   │   ├── mod.rs
│   │   ├── javascript.rs    # JS/TS/TSX/JSX family
│   │   ├── rust.rs
│   │   ├── python.rs
│   │   ├── go.rs
│   │   ├── java.rs
│   │   ├── c_family.rs      # C/C++
│   │   ├── markup.rs        # HTML/CSS/Markdown
│   │   └── config.rs        # JSON/YAML/TOML
│   ├── risk.rs              # Behavioral risk calculation
│   └── toon.rs              # TOON encoder
├── queries/                  # Tree-sitter query files (.scm)
│   ├── typescript/
│   ├── tsx/
│   ├── javascript/
│   ├── rust/
│   ├── python/
│   ├── go/
│   ├── java/
│   ├── c/
│   ├── cpp/
│   ├── html/
│   ├── css/
│   ├── json/
│   ├── yaml/
│   ├── toml/
│   └── markdown/
├── Cargo.toml
└── README.md
```

---

## 4. Supported Languages & Extensions

| Language | Extensions | Grammar Crate | Notes |
|----------|------------|---------------|-------|
| TypeScript | `.ts` | `tree-sitter-typescript` | `LANGUAGE_TYPESCRIPT` |
| TSX | `.tsx` | `tree-sitter-typescript` | `LANGUAGE_TSX` |
| JavaScript | `.js`, `.mjs`, `.cjs` | `tree-sitter-javascript` | |
| JSX | `.jsx` | `tree-sitter-javascript` | Same grammar as JS |
| Rust | `.rs` | `tree-sitter-rust` | |
| Python | `.py`, `.pyi` | `tree-sitter-python` | |
| Go | `.go` | `tree-sitter-go` | |
| Java | `.java` | `tree-sitter-java` | |
| C | `.c`, `.h` | `tree-sitter-c` | |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh` | `tree-sitter-cpp` | |
| HTML | `.html`, `.htm` | `tree-sitter-html` | |
| CSS | `.css` | `tree-sitter-css` | |
| JSON | `.json` | `tree-sitter-json` | |
| YAML | `.yaml`, `.yml` | `tree-sitter-yaml` | |
| TOML | `.toml` | `tree-sitter-toml-ng` | |
| Markdown | `.md`, `.markdown` | `tree-sitter-md` | |

---

## 5. Data Structures (schema.rs)

### 5.1 Semantic Model

```rust
/// Complete semantic summary of a file
#[derive(Debug, Default)]
pub struct SemanticSummary {
    pub file: String,
    pub language: String,
    pub symbol: Option<String>,
    pub symbol_kind: Option<SymbolKind>,
    pub props: Vec<Prop>,
    pub arguments: Vec<Argument>,
    pub return_type: Option<String>,
    pub insertions: Vec<String>,
    pub added_dependencies: Vec<String>,
    pub state_changes: Vec<StateChange>,
    pub control_flow_changes: Vec<ControlFlowChange>,
    pub public_surface_changed: bool,
    pub behavioral_risk: RiskLevel,
    pub raw_fallback: Option<String>,  // Safety fallback
    pub extraction_complete: bool,      // For safety validation
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SymbolKind {
    #[default]
    Function,
    Component,    // React/Vue component
    Class,
    Method,
    Interface,
    Trait,
    Struct,
    Enum,
    Module,
    TypeAlias,
}

#[derive(Debug)]
pub struct Prop {
    pub name: String,
    pub prop_type: Option<String>,
    pub default_value: Option<String>,
    pub required: bool,
}

#[derive(Debug)]
pub struct Argument {
    pub name: String,
    pub arg_type: Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug)]
pub struct StateChange {
    pub name: String,
    pub state_type: String,
    pub initializer: String,
}

#[derive(Debug)]
pub struct ControlFlowChange {
    pub kind: ControlFlowKind,
    pub location: Location,
}

#[derive(Debug, Clone, Copy)]
pub enum ControlFlowKind {
    If,
    For,
    While,
    Switch,
    Match,
    Try,
    Loop,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Default)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}
```

---

## 6. Error Handling (error.rs)

```rust
use thiserror::Error;
use std::process::ExitCode;

#[derive(Error, Debug)]
pub enum McpDiffError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Unsupported language for extension: {extension}")]
    UnsupportedLanguage { extension: String },

    #[error("Failed to parse file: {message}")]
    ParseFailure { message: String },

    #[error("Semantic extraction failed: {message}")]
    ExtractionFailure { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl McpDiffError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::FileNotFound { .. } => ExitCode::from(1),
            Self::UnsupportedLanguage { .. } => ExitCode::from(2),
            Self::ParseFailure { .. } => ExitCode::from(3),
            Self::ExtractionFailure { .. } => ExitCode::from(4),
            Self::Io(_) => ExitCode::from(1),
        }
    }
}

pub type Result<T> = std::result::Result<T, McpDiffError>;
```

---

## 7. CLI Interface (cli.rs)

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "semfora-mcp")]
#[command(about = "Semantic code analyzer with TOON output")]
#[command(version)]
pub struct Cli {
    /// Path to file to analyze
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Output format (toon or json)
    #[arg(short, long, default_value = "toon")]
    pub format: OutputFormat,

    /// Show verbose output including AST info
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Toon,
    Json,
}
```

---

## 8. Language Detection (lang.rs)

```rust
use std::path::Path;
use tree_sitter::Language;
use crate::error::{McpDiffError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Rust,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Html,
    Css,
    Json,
    Yaml,
    Toml,
    Markdown,
}

impl Lang {
    pub fn from_path(path: &Path) -> Result<Self> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| McpDiffError::UnsupportedLanguage {
                extension: "none".to_string()
            })?;

        match ext {
            "ts" => Ok(Self::TypeScript),
            "tsx" => Ok(Self::Tsx),
            "js" | "mjs" | "cjs" => Ok(Self::JavaScript),
            "jsx" => Ok(Self::Jsx),
            "rs" => Ok(Self::Rust),
            "py" | "pyi" => Ok(Self::Python),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "c" | "h" => Ok(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Ok(Self::Cpp),
            "html" | "htm" => Ok(Self::Html),
            "css" => Ok(Self::Css),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            "toml" => Ok(Self::Toml),
            "md" | "markdown" => Ok(Self::Markdown),
            _ => Err(McpDiffError::UnsupportedLanguage {
                extension: ext.to_string()
            }),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Html => "html",
            Self::Css => "css",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
        }
    }

    /// Language family for shared extraction logic
    pub fn family(&self) -> LangFamily {
        match self {
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => {
                LangFamily::JavaScript
            }
            Self::Rust => LangFamily::Rust,
            Self::Python => LangFamily::Python,
            Self::Go => LangFamily::Go,
            Self::Java => LangFamily::Java,
            Self::C | Self::Cpp => LangFamily::CFamily,
            Self::Html | Self::Css | Self::Markdown => LangFamily::Markup,
            Self::Json | Self::Yaml | Self::Toml => LangFamily::Config,
        }
    }

    pub fn supports_jsx(&self) -> bool {
        matches!(self, Self::Tsx | Self::Jsx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangFamily {
    JavaScript,
    Rust,
    Python,
    Go,
    Java,
    CFamily,
    Markup,
    Config,
}
```

---

## 9. Tree-Sitter Query System

### 9.1 Standard Capture Names

All query files use consistent capture names for uniform processing:

| Capture | Description |
|---------|-------------|
| `@symbol.name` | Symbol/function/class name |
| `@symbol.definition` | Entire definition node |
| `@symbol.params` | Parameter list |
| `@symbol.return_type` | Return type annotation |
| `@import.source` | Import module path |
| `@import.name` | Imported item name |
| `@import.alias` | Import alias |
| `@call.name` | Called function name |
| `@call.object` | Object for method calls |
| `@state.name` | Variable/state name |
| `@state.type` | Type annotation |
| `@state.init` | Initializer |
| `@control.kind` | Control flow type |
| `@jsx.tag` | JSX element tag |
| `@jsx.prop.name` | JSX prop name |
| `@export.name` | Exported name |

### 9.2 Query File Examples

#### queries/tsx/symbols.scm
```scheme
;; Function declarations
(function_declaration
  name: (identifier) @symbol.name
  parameters: (formal_parameters) @symbol.params
  return_type: (type_annotation)? @symbol.return_type) @symbol.definition

;; Arrow function components
(lexical_declaration
  (variable_declarator
    name: (identifier) @symbol.name
    value: (arrow_function
      parameters: (formal_parameters) @symbol.params
      return_type: (type_annotation)? @symbol.return_type))) @symbol.definition

;; Export default function
(export_statement
  (function_declaration
    name: (identifier) @symbol.name
    parameters: (formal_parameters) @symbol.params)) @symbol.definition

;; Class declarations
(class_declaration
  name: (type_identifier) @symbol.name) @symbol.definition
```

#### queries/tsx/imports.scm
```scheme
;; Named imports
(import_statement
  source: (string) @import.source
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @import.name
        alias: (identifier)? @import.alias))))

;; Default imports
(import_statement
  source: (string) @import.source
  (import_clause
    (identifier) @import.name))
```

#### queries/tsx/state.scm
```scheme
;; useState hook
(lexical_declaration
  (variable_declarator
    name: (array_pattern
      (identifier) @state.name)
    value: (call_expression
      function: (identifier) @_hook
      (#eq? @_hook "useState")
      arguments: (arguments
        (_)? @state.init)))) @state.declaration

;; useReducer hook
(lexical_declaration
  (variable_declarator
    name: (array_pattern
      (identifier) @state.name)
    value: (call_expression
      function: (identifier) @_hook
      (#eq? @_hook "useReducer")))) @state.declaration
```

#### queries/tsx/jsx.scm
```scheme
;; JSX elements
(jsx_element
  open_tag: (jsx_opening_element
    name: (identifier) @jsx.tag)) @jsx.element

;; Self-closing elements
(jsx_self_closing_element
  name: (identifier) @jsx.tag) @jsx.element

;; Link elements specifically
(jsx_self_closing_element
  name: (identifier) @jsx.tag
  (#eq? @jsx.tag "Link")) @jsx.link
```

#### queries/rust/symbols.scm
```scheme
;; Functions
(function_item
  name: (identifier) @symbol.name
  parameters: (parameters) @symbol.params
  return_type: (_)? @symbol.return_type) @symbol.definition

;; Structs
(struct_item
  name: (type_identifier) @symbol.name) @symbol.definition

;; Impl blocks
(impl_item
  type: (type_identifier) @symbol.name) @symbol.definition

;; Traits
(trait_item
  name: (type_identifier) @symbol.name) @symbol.definition
```

#### queries/rust/imports.scm
```scheme
;; Use declarations
(use_declaration
  argument: (scoped_identifier
    path: (_) @import.source
    name: (identifier) @import.name)) @import.statement

;; Use with alias
(use_declaration
  argument: (use_as_clause
    path: (scoped_identifier) @import.source
    alias: (identifier) @import.alias)) @import.statement
```

#### queries/python/symbols.scm
```scheme
;; Function definitions
(function_definition
  name: (identifier) @symbol.name
  parameters: (parameters) @symbol.params
  return_type: (type)? @symbol.return_type) @symbol.definition

;; Class definitions
(class_definition
  name: (identifier) @symbol.name) @symbol.definition

;; Decorated functions
(decorated_definition
  (function_definition
    name: (identifier) @symbol.name
    parameters: (parameters) @symbol.params)) @symbol.definition
```

---

## 10. Behavioral Risk Calculation (risk.rs)

```rust
use crate::schema::{SemanticSummary, RiskLevel};

pub fn calculate_risk(summary: &SemanticSummary) -> RiskLevel {
    let mut score = 0;

    // +1 per new import
    score += summary.added_dependencies.len();

    // +1 per state variable
    score += summary.state_changes.len();

    // +2 per control flow change
    score += summary.control_flow_changes.len() * 2;

    // +2 for I/O or network calls (detected via insertions)
    for insertion in &summary.insertions {
        if insertion.contains("network") || insertion.contains("fetch")
           || insertion.contains("invoke") || insertion.contains("I/O") {
            score += 2;
        }
    }

    // +3 for public API changes
    if summary.public_surface_changed {
        score += 3;
    }

    // +3 for persistence operations
    for insertion in &summary.insertions {
        if insertion.contains("storage") || insertion.contains("database")
           || insertion.contains("persist") {
            score += 3;
        }
    }

    match score {
        0..=1 => RiskLevel::Low,
        2..=3 => RiskLevel::Medium,
        _ => RiskLevel::High,
    }
}
```

---

## 11. TOON Encoder (toon.rs)

```rust
use crate::schema::{SemanticSummary, RiskLevel};
use std::fmt::Write;

pub fn encode_toon(summary: &SemanticSummary) -> String {
    let mut out = String::new();

    // Simple fields
    writeln!(out, "file: {}", summary.file).unwrap();
    writeln!(out, "language: {}", summary.language).unwrap();

    if let Some(ref sym) = summary.symbol {
        writeln!(out, "symbol: {}", sym).unwrap();
    }
    if let Some(ref kind) = summary.symbol_kind {
        writeln!(out, "symbol_kind: {}", format!("{:?}", kind).to_lowercase()).unwrap();
    }
    if let Some(ref ret) = summary.return_type {
        writeln!(out, "return_type: {}", ret).unwrap();
    }

    writeln!(out, "public_surface_changed: {}", summary.public_surface_changed).unwrap();
    writeln!(out, "behavioral_risk: {}", risk_to_string(summary.behavioral_risk)).unwrap();
    out.push('\n');

    // Insertions array (indented block)
    if !summary.insertions.is_empty() {
        writeln!(out, "insertions[{}]:", summary.insertions.len()).unwrap();
        for item in &summary.insertions {
            writeln!(out, "  {}", item).unwrap();
        }
        out.push('\n');
    }

    // Dependencies (inline array)
    if !summary.added_dependencies.is_empty() {
        writeln!(
            out,
            "added_dependencies[{}]: {}",
            summary.added_dependencies.len(),
            summary.added_dependencies.join(",")
        ).unwrap();
        out.push('\n');
    }

    // State changes (tabular)
    if !summary.state_changes.is_empty() {
        writeln!(
            out,
            "state_changes[{}]{{name,type,initializer}}:",
            summary.state_changes.len()
        ).unwrap();
        for state in &summary.state_changes {
            writeln!(out, "  {},{},{}", state.name, state.state_type, state.initializer).unwrap();
        }
        out.push('\n');
    }

    // Arguments (tabular)
    if !summary.arguments.is_empty() {
        writeln!(out, "arguments[{}]{{name,type,default}}:", summary.arguments.len()).unwrap();
        for arg in &summary.arguments {
            writeln!(
                out,
                "  {},{},{}",
                arg.name,
                arg.arg_type.as_deref().unwrap_or("_"),
                arg.default_value.as_deref().unwrap_or("_")
            ).unwrap();
        }
        out.push('\n');
    }

    // Props (tabular)
    if !summary.props.is_empty() {
        writeln!(out, "props[{}]{{name,type,default,required}}:", summary.props.len()).unwrap();
        for prop in &summary.props {
            writeln!(
                out,
                "  {},{},{},{}",
                prop.name,
                prop.prop_type.as_deref().unwrap_or("_"),
                prop.default_value.as_deref().unwrap_or("_"),
                prop.required
            ).unwrap();
        }
        out.push('\n');
    }

    // Control flow (inline)
    if !summary.control_flow_changes.is_empty() {
        let kinds: Vec<_> = summary.control_flow_changes.iter()
            .map(|c| format!("{:?}", c.kind).to_lowercase())
            .collect();
        writeln!(out, "control_flow[{}]: {}", kinds.len(), kinds.join(",")).unwrap();
        out.push('\n');
    }

    // Safety fallback
    if let Some(ref raw) = summary.raw_fallback {
        out.push_str("RAW BLOCK:\n```\n");
        out.push_str(raw);
        out.push_str("\n```\n");
    }

    out
}

fn risk_to_string(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}
```

---

## 12. Insertion Rule Engine

Insertions are deterministic, rule-based summaries (not AI-generated):

```rust
pub fn generate_insertions(
    summary: &SemanticSummary,
    jsx_elements: &[JsxElement],
    calls: &[Call]
) -> Vec<String> {
    let mut insertions = Vec::new();

    // JSX structure rules
    let tags: Vec<&str> = jsx_elements.iter().map(|e| e.tag.as_str()).collect();

    // Header with nav
    if tags.contains(&"header") {
        if tags.contains(&"nav") {
            insertions.push("header container with nav".to_string());
        } else {
            insertions.push("header container".to_string());
        }
    }

    // Route links
    let link_count = tags.iter().filter(|&&t| t == "Link" || t == "a").count();
    if link_count >= 3 {
        insertions.push(format!("{} route links", link_count));
    }

    // Dropdown pattern
    if tags.contains(&"button") && (tags.contains(&"div") || tags.contains(&"menu")) {
        insertions.push("dropdown menu".to_string());
    }

    // State hooks
    for state in &summary.state_changes {
        if state.state_type.contains("useState") || state.state_type.contains("useReducer") {
            insertions.push(format!("local {} state via {}", state.name, state.state_type));
        }
    }

    // Network/IO calls
    for call in calls {
        match call.name.as_str() {
            "fetch" | "axios" => insertions.push("network call introduced".to_string()),
            "invoke" => insertions.push("Tauri IPC call".to_string()),
            "open" | "read" | "write" => insertions.push("file I/O operation".to_string()),
            _ => {}
        }
    }

    insertions
}
```

---

## 13. Implementation Order

### Phase 1: Foundation (Days 1-2)
1. Initialize project structure
2. Add dependencies to Cargo.toml
3. Implement `error.rs` with exit codes
4. Implement `cli.rs` with clap
5. Implement `lang.rs` for language detection
6. Implement `schema.rs` data structures

### Phase 2: Core Parsing (Days 3-4)
7. Create query directory structure
8. Write TSX query files (all 6 types)
9. Implement basic `extract.rs` orchestration
10. Implement `detectors/javascript.rs` for TSX
11. Test with example TSX file from plan.md

### Phase 3: TOON Output (Days 5-6)
12. Implement `toon.rs` encoder
13. Implement `risk.rs` calculation
14. Implement insertion rule engine
15. Verify output matches plan.md example

### Phase 4: Language Expansion (Days 7-10)
16. Add Rust queries and detector
17. Add Python queries and detector
18. Add JavaScript/JSX support (shares TSX logic)
19. Add Go queries and detector
20. Add Java queries and detector

### Phase 5: Config & Markup (Days 11-12)
21. Add C/C++ queries and detector
22. Add JSON/YAML/TOML queries (simple structure)
23. Add HTML/CSS/Markdown queries (simple structure)

### Phase 6: Polish (Days 13-14)
24. Implement RAW BLOCK safety fallback
25. Add comprehensive tests
26. Add verbose mode output
27. Documentation and README

---

## 14. Testing Strategy

### Unit Tests
- Language detection for all extensions
- TOON encoding for each field type
- Risk calculation scoring
- Insertion rule matching

### Integration Tests
- Parse example file from plan.md, verify exact TOON output
- Test each language with representative sample
- Test error conditions (missing file, unsupported language, parse failure)

### Test File Location
```
tests/
├── fixtures/
│   ├── layout.tsx          # Example from plan.md
│   ├── sample.rs
│   ├── sample.py
│   ├── sample.go
│   └── ...
├── integration_tests.rs
└── toon_output_tests.rs
```

---

## 15. Known Limitations & Future Work

### Phase 1 Limitations
- No type checking (tree-sitter only)
- No cross-file analysis
- No git diff support (Phase 2)
- No MCP integration (Phase 4)

### Potential Issues
- Tree-sitter version compatibility between grammars
- Query patterns may need tuning for edge cases
- Some languages (Java, C++) have complex syntax requiring extensive queries

---

## 16. References

- [Tree-sitter Rust Bindings](https://docs.rs/tree-sitter)
- [Tree-sitter Query Syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries)
- [Clap Documentation](https://docs.rs/clap/latest/clap/)
- [thiserror Crate](https://docs.rs/thiserror)
- Original specification: `plan.md`
