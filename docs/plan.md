# MCP Semantic Diff & TOON Encoder (Semfora)

**A Semantic Compiler Front-End for Autonomous Code Agents**

Language: Rust
Output Formats: TOON, JSON
Execution Mode: Local CLI binary + MCP Server
Primary Goal: Deterministically generate semantic TOON summaries, diffs, and repository intelligence for AI agent consumption.

---

## Current Status

**Phase: 2.5 (Advanced MVP)**

The project has exceeded the original Phase 1-2 scope. Currently implemented:

| Capability | Status | Notes |
|------------|--------|-------|
| Deterministic semantic extraction | 100% | Tree-sitter based, multi-language |
| TOON encoding | 100% | 90%+ compression ratios achieved |
| Repo understanding mode | 90% | Framework detection, module grouping |
| File-level review mode | 85% | Per-file semantic projections |
| Git integration | 95% | Diff, merge-base, commit walking |
| MCP server | 90% | Async request handling, tool exposure |
| Call filtering | 85% | Sophisticated noise reduction |
| Risk scoring | 80% | Heuristic-based, graduated |

---

## 1. Vision & Motivation

Modern AI code review and autonomous coding systems fail at scale because:

- Raw diffs are token-inefficient
- Large files exceed context budgets
- Semantic changes are buried in formatting noise
- Multi-language stacks fracture agent understanding
- Agents cannot reliably track symbol identity across commits
- Safety gates require typed change classification, not text summaries

This project builds a **semantic compiler front-end** that:

- Converts source code into lossless semantic records
- Encodes records into TOON (Token-Oriented Object Notation)
- Provides explicit graph projections for agent reasoning
- Enables typed change classification for safety enforcement
- Maintains stable symbol identity across refactors
- Operates fully deterministically with zero AI calls

**This is not a code review tool. This is infrastructure for code-aware AI agents.**

---

## 2. Core Technologies

| Layer | Technology |
|-------|------------|
| Language | Rust |
| Parsing | tree-sitter (multi-language) |
| Encoding | TOON via rtoon v0.2.1 (TOON spec v3.0 compliant) |
| Version Control | Git (native integration) |
| Server Protocol | MCP (Model Context Protocol) |
| Async Runtime | Tokio |
| Python ADK | semfora-adk (uses toon-format library) |

**Design Principles:**
- No network access in core analysis
- No model calls
- No cloud dependency
- Deterministic output for identical input

---

## 3. Architecture Overview

```
semfora/
  src/
    main.rs
    cli.rs
    lib.rs

    schema.rs
    error.rs

    lang.rs
    extract.rs
    detectors/
      mod.rs

    toon.rs
    tokens.rs
    risk.rs

    git/
      mod.rs
      branch.rs
      commit.rs
      diff.rs

    mcp_server/
      mod.rs
      bin.rs

  docs/
    plan.md
    engineering.md

  tests/
    fixtures/
```

---

## Developer CLI Tools (`semfora-mcp`)

The Semfora Engine ships with a developer-facing CLI binary named `semfora-mcp` that provides:

- File analysis
- Recursive directory analysis
- Diff analysis (`--diff`, `--base`, `--commit`)
- Index generation (`--shard`)
- Cache management (`--cache-info`, `--cache-clear`)
- Benchmark tools for token compression (`--benchmark`)
- AST debugging tools (`--print-ast`)
- TOON / JSON output modes

This CLI exists to support:
- Engineering workflows
- Debugging of semantic extraction
- Validating TOON output
- Testing token efficiency
- Running one-off local analyses

None of the ADK or agent-driven systems (`semfora-cli`, `semfora-ci`) use this CLI.
They exclusively interact with the MCP server (`semfora-mcp-server`).

---

## Related Documents

| Document | Description |
|----------|-------------|
| [Semfora Agent Architecture](./semfora-agent-architecture.md) | Google ADK + Claude API code editor/CI integration design |
| [Query-Driven Architecture](./query-driven-architecture.md) | Token-efficient semantic query patterns |

---

## 4. Semantic Model

### 4.1 Core Schema (Current)

```rust
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
    pub local_imports: Vec<String>,
    pub state_changes: Vec<StateChange>,
    pub control_flow_changes: Vec<ControlFlowChange>,
    pub calls: Vec<Call>,
    pub public_surface_changed: bool,
    pub behavioral_risk: RiskLevel,
    pub raw_fallback: Option<String>,
}
```

### 4.2 Schema Extensions (Roadmap)

**Stable Symbol Identity:**

```rust
pub struct SymbolId {
    pub hash: String,
    pub namespace: String,
    pub symbol: String,
    pub kind: SymbolKind,
    pub arity: usize,
}
```

**Typed Surface Deltas:**

```rust
pub enum SurfaceDelta {
    StateAddition { name: String, state_type: String },
    StateRemoval { name: String },
    DependencyAdded { name: String },
    DependencyRemoved { name: String },
    ControlFlowComplexityChanged { before: usize, after: usize },
    PublicApiChanged { breaking: bool },
    CallArityChanged { symbol: String, before: usize, after: usize },
    PersistenceIntroduced,
    NetworkIntroduced,
    AuthenticationBoundaryChanged,
    PrivilegeBoundaryChanged,
}

pub struct SemanticDiff {
    pub file: String,
    pub deltas: Vec<SurfaceDelta>,
    pub risk_change: i8,
}
```

**Dependency Graphs:**

```rust
pub struct DependencyGraph {
    pub imports: HashMap<String, Vec<String>>,
    pub calls: HashMap<String, Vec<String>>,
    pub modules: HashMap<String, Vec<String>>,
}

pub struct StateInteractionGraph {
    pub state_symbol: String,
    pub callers: Vec<String>,
    pub side_effects: Vec<String>,
}
```

---

## 5. Supported Languages

| Language | Family | JSX | Status |
|----------|--------|-----|--------|
| TypeScript | JavaScript | Yes | Full |
| JavaScript | JavaScript | Yes | Full |
| TSX | JavaScript | Yes | Full |
| JSX | JavaScript | Yes | Full |
| Rust | Rust | No | Full |
| Python | Python | No | Full |
| Go | Go | No | Full |
| Java | Java | No | Partial |
| C | CFamily | No | Partial |
| C++ | CFamily | No | Partial |
| HTML | Markup | No | Basic |
| CSS | Markup | No | Basic |
| JSON | Config | No | Full |
| YAML | Config | No | Full |
| TOML | Config | No | Full |
| Markdown | Markup | No | Basic |

---

## 6. Call Filtering System

Noise vs semantic relevance filtering is a core differentiator.

**Preserved:**
- React hooks
- State setters
- Data fetching
- Database operations
- I/O operations

**Filtered:**
- Collection chaining
- Promise boilerplate
- ORM builders
- Math and string helpers

**Future:** Configurable per project.

---

## 7. Risk Scoring

| Signal | Points |
|--------|--------|
| New import | +1 |
| New state | +1 |
| Control flow | +1–2 |
| Network/I/O | +2 |
| Public API | +3 |
| Persistence | +3 |

**Mapping:**
- 0–1 Low
- 2–3 Medium
- 4+ High

---

## 8. Roadmap

### Phase 3.0: Semantic Sharding & Agent Consumption at Scale (PRIORITY)

This phase explicitly solves 50k–100k+ file repositories and keeps AI agents within bounded context.

#### Goals

- Eliminate monolithic TOON output for runtime use
- Convert Semfora into a semantic query engine
- Allow agents to request only the semantic slice they need
- Guarantee sub-second agent response times regardless of repo size

#### Core Shift

**From:**
One massive TOON file per repo

**To:**
A sharded, indexed semantic store with retrieval-based access.

#### On-Disk Semantic Layout

```
.semfora/
  repo_overview.toon

  modules/
    api.toon
    components.toon
    database.toon
    server.toon

  symbols/
    {symbol_id}.toon

  graphs/
    call_graph.toon
    import_graph.toon
    module_graph.toon

  diffs/
    commit_{sha}.toon
```

Each file is independently fetchable by an AI agent.

**Important:** The `.semfora/` directory should be:
- Added to `.gitignore` (not persisted to repositories)
- Stored in a temporary location accessible to AI agents
- Regenerated on demand when needed

#### MCP Query Tools (New)

| Tool | Description |
|------|-------------|
| get_repo_overview | Fetch high-level architecture only |
| get_module | Fetch full semantic slice for a module |
| get_symbol | Fetch a single symbol by SymbolId |
| get_call_graph | Fetch invocation dependencies |
| get_diff | Fetch typed diffs for a commit |
| get_impact_radius | Return transitive change impact |

#### Agent Consumption Workflow

See [Semfora Agent Architecture](./semfora-agent-architecture.md#workflow-patterns) for detailed workflow patterns including:
- Token budget comparisons (6x improvement over raw file reads)
- Interactive session flows
- CI review automation
- Memory and context management

**Agents never ingest full repository context.**

#### Performance Targets

| Operation | Target |
|-----------|--------|
| Full repo index | One-time cost (30–60s huge monorepos) |
| Module fetch | < 100ms |
| Symbol fetch | < 50ms |
| Diff verification | < 200ms |

#### Implementation Work

1. IR sharding pass after extraction
2. SymbolId-based file naming
3. Embedded index (FS, SQLite, or sled)
4. MCP router for semantic slices

#### Value Unlocked

- Autonomous large-repo editing
- Scoped agent reasoning
- Deterministic impact analysis
- Safe multi-agent coordination
- Zero token overflow failure modes

---

#### Initialization & First-Run Experience

**The Problem:** An agent opens a new repo and immediately asks "what does this codebase do?" but there's no semantic index yet. A 30-60 second generation delay is unacceptable for interactive use.

**Solution: Progressive Background Indexing**

```
┌─────────────────────────────────────────────────────────────┐
│                    MCP Server Startup                        │
├─────────────────────────────────────────────────────────────┤
│ 1. Detect working directory (cwd or configured path)        │
│ 2. Compute repo hash (from git remote or absolute path)     │
│ 3. Check for existing cache at $XDG_CACHE_HOME/semfora/     │
│ 4. If missing/stale:                                         │
│    a. Spawn background indexer thread                        │
│    b. Generate repo_overview FIRST (< 5 seconds)             │
│    c. Then modules, then symbols (parallelized)              │
│ 5. Mark server as ready immediately                          │
│ 6. Start file watcher for incremental updates                │
└─────────────────────────────────────────────────────────────┘
```

**During Indexing - Partial Results:**

When the agent queries before indexing completes, responses include status:

```
_type: repo_overview
indexing_status:
  in_progress: true
  files_indexed: 2341
  files_total: 8902
  percent: 26
  eta_seconds: 18
  modules_ready: ["src", "lib", "components"]
  modules_pending: ["packages", "apps"]
framework: "Next.js 14"
...
```

Agents can immediately start working with available data while indexing continues.

**Index Priority Order:**
1. `repo_overview.toon` - Framework, patterns, modules list (< 5s)
2. Entry point modules (app/, src/, lib/)
3. High-risk modules (db/, api/, auth/)
4. Remaining modules by file count
5. Individual symbols
6. Graph aggregation (call_graph, import_graph)

This ensures agents have useful context within seconds, not minutes.

---

#### Incremental Updates & Cache Freshness

**The Problem:** User or AI agent edits files. The cached index becomes stale. Re-indexing the entire repo on every change is wasteful.

**Solution: File-Level Granularity with Module Invalidation**

```
┌─────────────────────────────────────────────────────────────┐
│           File Change: src/api/users.ts modified            │
├─────────────────────────────────────────────────────────────┤
│ 1. File watcher detects change (fsnotify/inotify)           │
│ 2. Debounce: wait 2 seconds for additional changes          │
│ 3. Batch: collect all changed files                          │
│ 4. Re-extract: parse only changed files with tree-sitter    │
│ 5. Update symbols: write new symbols/{id}.toon files        │
│ 6. Invalidate module: mark modules/api.toon for refresh     │
│ 7. Update graphs: modify edges involving changed symbols    │
│ 8. Log: "Refreshed 3 files, 7 symbols in 0.4s"              │
└─────────────────────────────────────────────────────────────┘
```

**Staleness Detection:**

Each cached `.toon` file includes metadata:

```
_meta:
  schema_version: "1.0"
  generated_at: "2024-01-15T10:30:00Z"
  source_files:
    - path: "src/api/users.ts"
      mtime: 1705312200
      hash: "sha256:abc123..."
    - path: "src/api/auth.ts"
      mtime: 1705312100
      hash: "sha256:def456..."
```

On query:
1. Check if any `source_files[].mtime` is older than actual file mtime
2. If stale, regenerate that slice on-the-fly before returning
3. Background worker then updates the cache

**Update Triggers:**

| Trigger | When | Scope |
|---------|------|-------|
| File watcher | On save (debounced 2s) | Changed files only |
| Git hooks | Post-commit, post-merge, post-checkout | Files in diff |
| On query | If cache miss or stale | Single slice |
| Manual | CLI command `semfora refresh` | Full or targeted |

**Git Integration:**

```bash
# Post-checkout hook (optional, user installs)
#!/bin/sh
semfora refresh --git-diff HEAD@{1} HEAD
```

This only re-indexes files that changed between checkouts, not the entire repo.

---

#### Query Frequency & Token Efficiency

**Key Insight: Semantic queries REPLACE file reads at approximately 6:1 ratio.**

For detailed workflow comparisons and token budget analysis, see [Semfora Agent Architecture - Token Budget Comparison](./semfora-agent-architecture.md#token-budget-comparison).

Summary:
- **Without semantic layer:** 15-35 tool calls, ~25,000 tokens of raw source
- **With semantic layer:** 6 tool calls, ~4,150 tokens
- **Improvement:** ~6x fewer tokens, ~3x fewer tool calls

**When Agents Still Use Read:**

Semantic queries are for **understanding**. Read is for **editing**.

| Use Case | Tool |
|----------|------|
| "What does this codebase do?" | `get_repo_overview` |
| "Show me the API handlers" | `get_module("api")` |
| "What calls this function?" | `get_call_graph(symbol_id)` |
| "I need to edit this file" | `Read` (for exact source) |
| "What changed in this PR?" | `get_diff(commit)` |

---

#### Storage Location

**Decision: XDG-compliant cache with optional in-project override**

**Default Location:**
```
$XDG_CACHE_HOME/semfora/{repo-hash}/
  └── (falls back to ~/.cache/semfora/{repo-hash}/)
```

**Repo Hash Computation:**
```rust
fn compute_repo_hash(path: &Path) -> String {
    // Prefer git remote URL for consistency across clones
    if let Some(remote) = get_git_remote_url(path) {
        return hash(&remote);
    }
    // Fall back to absolute path
    hash(&path.canonicalize())
}
```

**Why Not In-Project by Default:**
- Pollutes project directory
- Risk of accidental commit (despite .gitignore)
- Multiple tools might conflict
- User may have read-only project directories

**Configuration Override:**

In `semfora.toml` or `Cargo.toml` [package.metadata.semfora]:
```toml
[semfora]
cache_dir = ".semfora"  # In-project, add to .gitignore
# or
cache_dir = "/tmp/semfora"  # Ephemeral
```

**Cache Cleanup:**

```bash
# List all cached repos
semfora cache list

# Remove cache for current repo
semfora cache clear

# Remove caches older than 30 days
semfora cache prune --older-than 30d

# Remove orphaned caches (repos that no longer exist)
semfora cache prune --orphaned
```

---

### Phase 3.0 Sub-priorities

**Priority 3.0A: Stable Symbol Identity** (COMPLETED)
- Add SymbolId struct with namespace-based hashing
- Populate in extraction pipeline
- Emit in TOON output
- Critical for: cross-commit tracking, agent memory, regression correlation

**Priority 3.0B: Typed Surface Deltas** (COMPLETED)
- Add SurfaceDelta enum
- Refactor diff generation to produce typed deltas
- Include risk_change calculation
- Critical for: safety gates, merge policies, agent constraints

**Priority 3.0C: Public API Surface Consistency** (COMPLETED)
- Audit all language extractors
- Ensure public_surface_changed is populated consistently
- Add export detection for each language family
- Critical for: breaking change detection

**Priority 3.0D: Schema Versioning** (COMPLETED)
- Add schema_version: "1.0" to all output
- Document schema contract
- Plan migration strategy
- Critical for: downstream consumer stability

**Priority 3.0E: Graph Aggregation** (COMPLETED)
- Populate RepoOverview.data_flow from local_imports ✓
- Add DependencyGraph to output (call_graph, import_graph in shard.rs) ✓
- Add module_graph generation ✓
- Consider StateInteractionGraph for complex repos (deferred to future)
- Critical for: impact radius queries, safe refactoring

**Priority 3.0F: Detector Modularization + Symbol Selection** (IN PROGRESS)
- Split extract.rs into language-family modules ✓
- Structure: src/detectors/{common,javascript,rust,python,go}.rs ✓
- Each detector: stateless, single-purpose, unit-testable ✓
- **Improve primary symbol selection heuristics:** ✓
  - Prioritize public/exported symbols over private helpers ✓
  - Prefer structs/enums/traits over functions for Rust ✓
  - Consider filename matching (e.g., `toon.rs` → prefer `encode_toon`) ✓
  - For multi-symbol files, consider emitting a `symbols[]` array in addition to primary (deferred)
- Remaining: Java, C/C++, Markup, Config detectors (lower priority)
- Critical for: maintainability, new language support, accurate semantic representation

---

### Phase 3.5: Robustness & Configurability

**Priority 7: Configurable Call Filtering**
- Move filter rules to configuration file
- Allow per-project overrides
- Support rule categories (noise, meaningful, always-include)
- Critical for: team customization, different codebases

**Priority 8: Trust Boundaries**
- Add immutable_paths configuration
- Default exclusions: node_modules, target, dist, vendor
- Mark generated code as read-only for agents
- Critical for: safe autonomous operation

**Priority 9: Performance Envelope Testing**
- Test against 100k+ file monorepo
- Test mixed-language directories
- Test circular import graphs
- Test heavy macro/metaprogramming usage
- Identify bottlenecks, add caching where needed

**Priority 10: Agent Consumption Specification**
- Create formal documentation for AI agents on how to use semfora-mcp
- Document all input formats, options, and CLI/MCP invocation patterns
- Define output interpretation guide (what each field means, how to reason about it)
- Provide decision trees for common agent tasks
- Include anti-patterns (what NOT to do with the output)
- Embed spec in MCP server system prompt or as retrievable resource
- Critical for: consistent agent behavior, self-service integration, reducing hallucination

---

### Phase 4.0: Rename + Dogfooding + Hardening

**Product Name: Semfora**

This phase transforms the prototype into a production-ready product.

**Rename & Rebrand:**
- ✅ Renamed package from `mcp-diff` to `semfora-mcp`
- Domains acquired: semfora.com, semfora.org, semfora.dev
- ✅ Updated all binaries, crates, and documentation
- Design logo and brand identity

**Internal Dogfooding:**
- Use Semfora to build Semfora (self-referential quality assurance)
- Run against the Semfora codebase continuously
- Validate real-world performance and failure modes
- Discover edge cases before customers do

**Product Hardening:**
- Large-repo benchmarks (100k+ files)
- Stability fixes under concurrency
- Index corruption recovery
- Clear failure modes and logging
- Config UX improvements
- CLI ergonomics polish
- MCP reliability contracts
- Documentation and examples

**Milestone Question:** "Would I trust this on a mission-critical codebase?"

---

### Phase 5.0: Intent Layer (Future)

**Structured Intent Verification:**

```yaml
intent:
  add:
    - api_endpoint: POST /users
    - persistence: users table
```

- Phase 5a: Structured intent input
- Phase 5b: Intent -> observed change comparison
- Phase 5c: Mismatch reporting
- Phase 5d (optional): NL -> structured intent via AI

---

### Phase 6.0: Cross-Language Intelligence (Future)

- Tauri invoke() -> Rust command mapping
- OpenAPI -> handler correlation
- DTO boundary tracking
- SDK ripple detection
- Multi-repo indexing

---

### Phase 7.0: Hosted Semfora Platform (Separate Program)

> **Note:** The local agent architecture using Google ADK is documented in [Semfora Agent Architecture](./semfora-agent-architecture.md). The hosted platform extends this with cloud sync capabilities, enabling 1-3 second cold starts instead of 30-60 second local index generation.

**Strategic Context:**

The local progressive indexing (Phase 3.0) is technically excellent but not sufficient as the primary commercial product experience. A 30-60 second cold start, even with partial results, kills:
- Conversion rates
- First-session trust
- Team adoption
- Enterprise pilots

**The Solution: Hybrid Local + Remote Semantic Indexing**

Local Semfora becomes a sync + delta engine, not the primary index builder:

| Mode | Cold Start | Use Case |
|------|------------|----------|
| Local-only (current) | 30-60s | Fallback, offline, OSS |
| Remote + local sync | 1-3s | Commercial product |

Instead of "build entire repo locally on first run", it becomes:
"Download prebuilt semantic state in 1-3 seconds, then patch local diffs."

**This is the difference between "cool tool" and "holy shit, this feels like magic."**

#### What the Hosted Platform Requires

This is a full SaaS product, not a feature. It requires:

**Core Backend (New System):**
- Public API server (Rust + Axum or similar)
- Project isolation and multi-tenancy
- Encrypted storage for semantic shards, diff history, symbol graphs
- Webhook ingestion (GitHub, GitLab, Azure DevOps)
- Background job processing (index builds, diff generation)
- Rate limiting and abuse protection

**Security and Compliance:**
- Repo access via GitHub App, GitLab App, Azure DevOps OAuth
- Token scoping per repository
- Encryption at rest and in transit
- Organization-level isolation
- Audit logs
- Optional VPC or self-hosted enterprise mode

**UI + Admin:**
- Authentication (GitHub OAuth, ADO OAuth, email+SSO later)
- Project settings dashboard
- Access control (owners, admins, read-only users)
- Index status and health monitoring
- Usage metrics
- API keys for MCP/local clients

**DevOps + Infrastructure:**
- Multi-tenant job runners
- Queue system (Redis/BullMQ or SQS)
- Object storage (S3-compatible)
- Database (Postgres)
- Secrets management
- CI image hardening

**Business Layer:**
- Billing (Stripe)
- Plans, quotas, limits
- Team management
- Terms of service, privacy policy
- Marketing website + documentation

#### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Semfora Platform                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ GitHub App   │    │ GitLab App   │    │  ADO OAuth   │       │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘       │
│         │                   │                   │                │
│         └───────────────────┼───────────────────┘                │
│                             ▼                                    │
│                   ┌─────────────────┐                            │
│                   │  Webhook API    │                            │
│                   └────────┬────────┘                            │
│                            ▼                                     │
│                   ┌─────────────────┐                            │
│                   │   Job Queue     │                            │
│                   └────────┬────────┘                            │
│                            ▼                                     │
│                   ┌─────────────────┐                            │
│                   │ Semfora Engine  │ ← (Phase 3 engine)         │
│                   └────────┬────────┘                            │
│                            ▼                                     │
│         ┌──────────────────┼──────────────────┐                  │
│         ▼                  ▼                  ▼                  │
│  ┌────────────┐    ┌────────────┐    ┌────────────┐             │
│  │  Sharded   │    │   Symbol   │    │   Graph    │             │
│  │    IR      │    │   Index    │    │   Store    │             │
│  └────────────┘    └────────────┘    └────────────┘             │
│                            │                                     │
│                            ▼                                     │
│                   ┌─────────────────┐                            │
│                   │   Query API     │                            │
│                   └────────┬────────┘                            │
│                            ▼                                     │
│         ┌──────────────────┼──────────────────┐                  │
│         ▼                  ▼                  ▼                  │
│  ┌────────────┐    ┌────────────┐    ┌────────────┐             │
│  │ Local CLI  │    │ MCP Server │    │  Web UI    │             │
│  │   Sync     │    │   Query    │    │ Dashboard  │             │
│  └────────────┘    └────────────┘    └────────────┘             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

#### CI Integration Flow

```yaml
# .github/workflows/semfora.yml
name: Semfora Index
on:
  push:
    branches: [main, develop]
  pull_request:

jobs:
  index:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: semfora/action@v1
        with:
          api-key: ${{ secrets.SEMFORA_API_KEY }}
          # Builds semantic index, uploads to Semfora backend
```

#### Local Sync Flow

```bash
# Instead of building locally (30-60s):
$ semfora index .

# Instant sync from remote (1-3s):
$ semfora sync
Downloading semantic index for myorg/myrepo@main...
Downloaded 2.3 MB in 1.2s
Applying local diff (3 files changed)...
Ready.

# MCP server uses synced index
$ semfora serve
```

#### How Current Work Fits Into Hosted Future

| Current Design Piece | Role in Hosted Platform |
|---------------------|-------------------------|
| Progressive indexing | ✅ Fallback when no remote index exists |
| File watcher | ✅ Local patch engine for uncommitted changes |
| Staleness detection | ✅ Consistency checker vs CI-built index |
| Git hooks | ✅ Optional offline mode |
| On-query regeneration | ✅ Cache-miss recovery |
| Sharded IR | ✅ Shared format between local + remote |
| Symbol IDs | ✅ Cross-system symbol identity |
| Typed diffs | ✅ PR safety gates |

**Nothing is wasted. The local engine becomes a consumer of remote state.**

#### Codebase Separation

The hosted platform should be separate codebases:

```
semfora/
├── semfora-engine/     # Rust semantic compiler (local CLI + MCP server)
├── semfora-adk/        # Python ADK for agent orchestration (Model B architecture)
├── semfora-cloud/      # API server, job workers, storage (Rust or TS)
├── semfora-web/        # Dashboard, settings, marketing (Next.js/React)
└── semfora-action/     # GitHub Action wrapper
```

This separation ensures:
- Engine remains pure, deterministic, no network dependencies
- Cloud layer handles tenancy, auth, billing
- Web layer handles UX
- Each can be versioned and deployed independently

**Milestone Question:** "Can teams safely and legally rely on this in production?"

---

## 9. Product Strategy & Business Model

### Product Tiers

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Free / OSS** | $0 | Local-only semantic engine, progressive background indexing, manual refresh, MCP server, local-only agents, no history, no team sharing | Hardcore devs, hackers, indie builders, OSS projects |
| **Pro** | $X/mo | Hosted CI indexing, automatic semantic state sync, local patch overlay, full diff history, branch comparisons, private repo support | Solo devs, startups, contractors |
| **Team** | $Y/seat/mo | Org-wide semantic graph, shared symbol registry, team-level access controls, usage analytics | Small-medium teams |
| **Enterprise** | Custom | Historical trend analysis, PR gating with typed safety policies, audit trails, air-gapped/VPC deployment, SOC2/GDPR posture, SSO, dedicated support | Large organizations |

### Value Proposition by Tier

**Free Tier Value:**
- Full semantic power locally
- Perfect for trying out the technology
- No account required
- Gateway to paid tiers

**Pro Tier Value:**
- "It just works" - instant semantic context on any repo
- Historical reasoning - "what changed last week?"
- Branch comparisons without local checkout

**Team/Enterprise Value:**
- Consistent semantic understanding across all team members
- Policy enforcement at PR time
- Audit trail for compliance
- Integration with existing CI/CD

---

## 10. Competitive Landscape

### Text-Based AI Code Reviewers

| Competitor | Approach | Semfora Advantage |
|------------|----------|-------------------|
| Reviewdog | Text-based linting | Semantic understanding, not pattern matching |
| CodeRabbit | LLM reads raw diffs | Token-efficient semantic IR, deterministic |
| Codium PR-Agent | LLM reads code | Structured typed diffs, not text interpretation |
| Amazon CodeGuru | ML-based analysis | Local-first, no cloud dependency in core |
| GitHub Copilot PR Review | LLM reads diffs | Cross-language symbol identity, agent-native |

**Their weakness:** They read diffs as text, not semantic compiler IR.

### Static Analyzers

| Competitor | Approach | Semfora Advantage |
|------------|----------|-------------------|
| Semgrep | Rule-based pattern matching | Semantic understanding, not regex |
| SonarQube | Quality metrics | Agent reasoning, not dashboards |
| Snyk | Security scanning | Broader semantic scope |
| CodeQL | Query language for code | Lower barrier, agent-consumable output |

**Their weakness:** They operate on rules + security, not agent reasoning or semantic compression.

### What No One Else Provides

1. **Token-optimized semantic IR for agents** - 90%+ compression vs raw source
2. **Deterministic graph-based semantic diffing** - Not LLM interpretation
3. **Cross-language symbol identity** - Stable IDs across refactors
4. **Local + CI hybrid semantic synchronization** - Best of both worlds
5. **Agent-first retrieval design** - Built for AI consumption, not human dashboards

**This is the moat.**

---

## 11. MCP Server Tools

**Current:**

| Tool | Description |
|------|-------------|
| analyze_file | Analyze a single source file |
| analyze_directory | Analyze entire codebase with framework detection |
| analyze_diff | Compare git branches/commits for code review |
| list_languages | List supported languages |

**Incoming with Phase 3:**

| Tool | Description |
|------|-------------|
| get_repo_overview | Fetch high-level architecture only |
| get_module | Fetch full semantic slice for a module |
| get_symbol | Fetch a single symbol by SymbolId |
| get_call_graph | Fetch invocation dependencies |
| get_diff | Fetch typed diffs for a commit |
| get_impact_radius | Return transitive change impact |

**Example Agent Prompts:**

Understanding mode:
> "Analyze this repo. What framework is it? Where are the entry points? What's the riskiest code?"

Review mode:
> "Analyze the diff from main to HEAD. What are the high-risk changes? Are there any breaking API changes?"

Safety gate mode (future):
> "Does this diff match the intent 'add pagination to users list'? Flag any scope creep."

---

## 12. Output Examples

### File Analysis (TOON)

```
file: ./src/components/UserList.tsx
language: tsx
symbol: UserList
symbol_kind: component
return_type: JSX.Element
behavioral_risk: medium
insertions[3]: "data fetching with useQuery","user card rendering","pagination controls"
added_dependencies[3]: useQuery,UserCard,Pagination
state[1]{name,type,init}:
  page,number,"1"
calls[2]{name,obj,await,try,count}:
  useQuery,_,_,_,_
  setPage,_,_,_,_
```

### Repository Overview (TOON)

```
_type: repo_overview
framework: "Next.js 14"
database: "Drizzle + PostgreSQL"
patterns[4]: "App Router","Server Components","API Routes","Drizzle ORM"
modules[8]{name,purpose,files,risk}:
  app,"Next.js app directory",12,medium
  components,"React components",24,low
  lib,"Utility functions",8,low
  db,"Database schema and queries",6,high
  api,"API route handlers",10,high
files: 60
risk_breakdown: "high:16,medium:20,low:24"
entry_points[1]: ./app/layout.tsx
```

---

## 13. Design Principles

1. **Deterministic:** Same input always produces same output
2. **Lossless:** No review-critical information lost (raw fallback if needed)
3. **Token-efficient:** 90%+ compression vs raw source
4. **Language-agnostic:** Unified model across all supported languages
5. **AI-ready but AI-independent:** Useful to humans, optimized for agents
6. **Safety-first:** Explicit failure modes, no silent degradation
7. **Retrieval-based at scale:** Progressive semantic disclosure, not monolithic dumps

---

## 14. Known Limitations

Current limitations to be addressed in future phases:

| Limitation | Impact | Planned Fix |
|------------|--------|-------------|
| Single symbol per file | Multi-symbol files (e.g., `schema.rs`) only report first symbol found | Phase 3.0F: Add `symbols[]` array |
| First-come symbol selection | Helper functions may be picked over main exports (e.g., `is_meaningful_call` vs `encode_toon`) | Phase 3.0F: Smarter heuristics |
| Comment-only files fall back to raw | Files like `detectors/mod.rs` with only docs show raw output | Expected behavior, not a bug |
| No cross-file symbol resolution | Calls show local names, not fully-qualified paths | Phase 3.0E: Graph aggregation |
| Monolithic output for large repos | Context overflow for AI agents | Phase 3.0: Semantic sharding |

---

## 15. Non-Goals

This system explicitly does not:

- Perform fuzzy natural language summarization
- Execute code
- Modify files
- Call AI models for core functionality
- Replace human code review
- Provide type checking (beyond what tree-sitter infers)

---

## 16. Success Metrics

**Compression:**
- Target: 85%+ reduction vs raw source
- Current: 90%+ achieved

**Coverage:**
- Target: Extract meaningful semantics from 95%+ of supported language files
- Current: ~90% (some edge cases fall back to raw)

**Latency:**
- Target: < 100ms for single file analysis
- Target: < 5s for 1000-file repo
- Target: < 100ms semantic lookup (with sharding)
- Current: Meeting targets for tested repos

**Agent utility:**
- Agents should be able to answer "what changed and why does it matter?" from TOON alone
- Agents should be able to identify breaking changes without reading raw diffs
- Agents should be able to assess impact radius from graph output
- Agents should never see full repository context (sharded access only)

---

## 17. Changelog

**v0.1.1 (Current)**
- TOON output now fully TOON spec v3.0 compliant (removed non-spec `---` separators)
- Using rtoon v0.2.1 Rust library for all TOON encoding
- Created semfora-adk Python package for agent orchestration (Model B architecture)
- Integrated toon-format Python library (v0.9.0b1) for TOON parsing in ADK
- Fixed TOML config parsing crash in config.rs
- Comprehensive test coverage (31 tests passing in semfora-adk)

**v0.1.0**
- Initial semantic extraction for JS/TS/Rust/Python/Go
- TOON encoding with compression analysis
- Git diff integration
- MCP server implementation
- Repo overview generation
- Call filtering system
- Risk scoring

**v0.2.0 (Planned)**
- Stable symbol IDs
- Typed surface deltas
- Schema versioning
- Consistent public_surface_changed
- Graph aggregation
- Detector modularization

**v0.3.0 (Semantic Sharding)**
- Sharded IR output (.semfora/ directory)
- MCP query interface (get_repo_overview, get_module, get_symbol, etc.)
- Large-repo agent scalability
- Progressive semantic disclosure

**v1.0.0 (Target)**
- Intent verification layer
- Full cross-language support
- Production-ready stability
