# Semfora Agent Architecture

**A Semantic-First Code Editor using Google ADK + Claude API**

This document describes the architecture for `semfora-cli`, a terminal-based code editing agent that **replaces** tools like Claude Code, Cursor, and Codex. Instead of an LLM that explores and discovers, we build an orchestrator that provides curated semantic context and uses Claude API purely for reasoning.

---

## Current Status

**Phase 1 (Foundation) COMPLETED:**
- `semfora-adk` Python package created with uv
- CLI subprocess integration with `semfora-mcp` binary
- TOON parsing via `toon-format` library (v0.9.0b1)
- Model B architecture implemented (orchestrator controls tools, Claude reasons)
- 31 tests passing

**Key Libraries:**
- Rust engine: `rtoon` v0.2.1 for TOON encoding (spec v3.0 compliant)
- Python ADK: `toon-format` for TOON decoding
- Python deps: `litellm`, `anthropic`, `rich`

---

## Scope

This document covers **local tooling we fully control**:

| Component | Description |
|-----------|-------------|
| `semfora-adk` | Python orchestration layer using Google ADK |
| `semfora-cli` | Interactive terminal editor (Claude Code replacement) |
| `semfora-ci` | Headless CI/CD integration |

**NOT in scope** (see plan.md):
- External agent support (Claude Code, Cursor, Codex using our MCP)
- Cloud service (Phase 7.0)
- Editor extensions (VS Code, Neovim)

---

## Semfora Engine Binaries Overview

The Semfora Engine provides two binaries built from the `semfora-engine` (Rust) project:

### 1. `semfora-mcp`

A command-line tool for developers that can:
- Analyze individual files
- Analyze directories recursively
- Analyze git diffs
- Print or benchmark TOON-encoded output
- Generate sharded caches
- Inspect or prune caches

This binary is designed for local developer workflows and debugging. It is **not** used by any ADK or CLI agent.

### 2. `semfora-mcp-server`

A headless binary that exposes the full MCP toolset over the MCP protocol.
This server is used by:

- External agents (Claude Code, Cursor, OpenAI Codex)
- Our internal ADK orchestrator (`semfora-adk`)
- The terminal editor (`semfora-cli`)
- The CI tool (`semfora-ci`)

All agent-controlled semantic operations go through the server, never through the CLI flags.

### Why Two Binaries?

The CLI version supports human-facing operations (file scanning, benchmarking, etc.).
The server version supports agent-facing operations (structured semantic tool calls).

---

## Key Distinction: Claude API vs Claude Code

| Term | What It Is | Our Relationship |
|------|------------|------------------|
| **Claude API** | Anthropic's API for LLM inference | We call it for reasoning |
| **Claude Code** | Anthropic's CLI agent that uses MCP | External agent that calls our MCP server |

**This architecture uses Claude API to BUILD A REPLACEMENT for Claude Code**, not to run inside it.

---

## Problem Statement

External AI code editors (Claude Code, Cursor, Codex) have fundamental inefficiencies:

1. The LLM decides when/how to use tools via prompting (unreliable)
2. No persistent memory - re-exploration on every session
3. Context wasted on "teaching" the agent to use tools efficiently
4. Inconsistent tool usage patterns lead to token bloat

**Our solution: Invert the control.**

```
External Agents (Claude Code):     LLM → decides tools → calls MCP → reasons
Our Agent (semfora-cli):           Orchestrator → calls tools → curates context → LLM reasons
```

The orchestrator makes ALL tool decisions. Claude API only receives curated semantic context and outputs reasoning/code.

---

## Three-Layer Architecture

```
+------------------------------------------------------------------------+
|                        LAYER 3: ACTION LAYER                            |
|                     (Claude API Reasoning + File Writes)                |
|                                                                         |
|   +------------------+  +------------------+  +------------------+      |
|   |  Claude Reasoner |  |   File Writer    |  |    Git Actor     |      |
|   |  (Opus/Sonnet)   |  |  (Patch/Apply)   |  | (Commit/Branch)  |      |
|   +------------------+  +------------------+  +------------------+      |
+------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------+
|                   LAYER 2: COGNITIVE ORCHESTRATOR                       |
|                        (Google ADK - Python)                            |
|                                                                         |
|   +------------------+  +------------------+  +------------------+      |
|   | SemforaMemory    |  | SemforaTools     |  | ContextBudget    |      |
|   | - repo_overview  |  | - 16 MCP tools   |  | - Token tracking |      |
|   | - module LRU     |  | - parallel exec  |  | - Auto-trim      |      |
|   | - session state  |  | - tool routing   |  | - Priority queue |      |
|   +------------------+  +------------------+  +------------------+      |
|                                                                         |
|   +------------------+  +------------------+  +------------------+      |
|   | TaskDecomposer   |  | WorkflowEngine   |  | SafetyGate       |      |
|   | - Intent parse   |  | - Sequential     |  | - Risk check     |      |
|   | - Subtask graph  |  | - Parallel       |  | - Scope verify   |      |
|   | - Dependency ord |  | - Loop/Iterate   |  | - Rollback plan  |      |
|   +------------------+  +------------------+  +------------------+      |
+------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------+
|                     LAYER 1: SEMANTIC ENGINE                            |
|                    (Existing Semfora - Rust)                            |
|                                                                         |
|   +------------------+  +------------------+  +------------------+      |
|   |   Tree-sitter    |  |  Shard Writer    |  |   Cache Layer    |      |
|   |   Extractors     |  |  - TOON encode   |  |   - XDG cache    |      |
|   |   - 8 lang       |  |  - Module shard  |  |   - Staleness    |      |
|   |   - Risk score   |  |  - Symbol index  |  |   - LRU evict    |      |
|   +------------------+  +------------------+  +------------------+      |
|                                                                         |
|   +------------------+  +------------------+  +------------------+      |
|   |  MCP Server      |  |   Git Integr.    |  |  Query Engine    |      |
|   |  - 16 tools      |  |   - Diff/Branch  |  |  - JSONL search  |      |
|   |  - Async Tokio   |  |   - Merge-base   |  |  - Symbol lookup |      |
|   |  - RMCP proto    |  |   - Commit walk  |  |  - Call graph    |      |
|   +------------------+  +------------------+  +------------------+      |
+------------------------------------------------------------------------+
```

---

## Layer 1: Semantic Engine (Existing)

The existing Semfora/semfora-mcp codebase provides the semantic foundation. **No changes required to this layer.**

### Available MCP Tools (16 total)

| Tool | Purpose | Token Cost |
|------|---------|------------|
| `get_repo_overview` | High-level architecture | ~2500 |
| `search_symbols` | Query symbol index | ~400/20 results |
| `list_symbols` | Browse module contents | ~800/50 results |
| `get_symbol` | Detailed symbol semantics | ~350 |
| `get_symbol_source` | Actual source code | ~400/50 lines |
| `analyze_diff` | Git diff with semantics | Varies |
| `get_call_graph` | Function relationships | Varies |
| `generate_index` | Create semantic cache | One-time |
| `list_modules` | Available modules | ~200 |
| `get_module` | Full module (expensive) | ~8000 |
| `analyze_file` | Single file analysis | ~500 |
| `analyze_directory` | Full codebase analysis | Large |
| `list_languages` | Supported languages | ~100 |

### Sharded Cache Structure

```
~/.cache/semfora/{repo-hash}/
  repo_overview.toon      # High-level architecture (~300KB max)
  symbol_index.jsonl      # Lightweight search index
  modules/
    api.toon              # Per-module semantic slices
    components.toon
    ...
  symbols/
    {hash}.toon           # Individual symbol details
  graphs/
    call_graph.toon       # Function invocation graph
    import_graph.toon     # File import relationships
    module_graph.toon     # Module dependency graph
```

---

## Layer 2: Cognitive Orchestrator (New - Google ADK)

The orchestrator is written in Python using Google ADK. It manages:
- Persistent semantic memory
- Parallel tool execution
- Context budget tracking
- Task decomposition

### Why Google ADK?

| Capability | MCP Server | Google ADK |
|------------|------------|------------|
| Memory persistence | Stateless | Built-in |
| Parallel tool calls | Sequential | Native |
| Context management | LLM decides | Orchestrator controls |
| Task decomposition | LLM decides | Programmable |
| LLM agnostic | Tied to client | Any API via LiteLLM |

### Component: SemforaMemory

Persistent semantic context across conversation turns.

```python
@dataclass
class SemforaMemory:
    """
    IMPORTANT: repo_overview is BOUNDED by design (~150KB max for any repo size).
    It uses aggregation (counts, module summaries) not enumeration (every file).

    NEVER cache full modules via get_module() - too expensive (8-12k tokens each).
    Instead: list_symbols() + get_symbol() for 4-5x token savings.
    """

    repo_overview: Optional[str] = None  # ~150KB max, bounded by aggregation
    repo_path: str = ""

    # NO module_cache - modules are too expensive to cache
    # Instead, cache individual symbols which are ~350 tokens each
    symbol_cache: OrderedDict = field(default_factory=OrderedDict)  # LRU
    max_symbols: int = 1000  # 1000 symbols × ~500 bytes = ~500KB

    # Query result caching (lightweight index entries only)
    search_result_cache: Dict[str, List[SymbolIndexEntry]] = field(default_factory=dict)

    viewed_symbols: List[str] = field(default_factory=list)
    search_history: List[str] = field(default_factory=list)
    context_tokens_used: int = 0
```

**Key behaviors:**
- `repo_overview` loaded once at session start (~2,500 tokens max, never grows with repo size)
- **NO module cache** - `get_module()` returns 8-12k tokens, too expensive
- Symbol cache for frequently accessed symbols - 1000 symbols (~500KB)
- Query results cached by search term for repeated lookups
- Tracks what agent has seen to avoid re-fetching

**Why repo_overview stays bounded:**

| Repo Size | Files | repo_overview Size | Why |
|-----------|-------|-------------------|-----|
| 100 MB | 2k | ~15 KB | Aggregates, doesn't enumerate |
| 500 MB | 10k | ~40 KB | ~50 module summaries, not 10k files |
| 1 GB | 20k | ~75 KB | Stats are counts, not lists |
| 3 GB | 60k | ~150 KB | Entry points capped, patterns limited |

The overview contains `total_files: 60000` not a list of 60k filenames.

### Component: ContextBudget

Token budget management for bounded agent operation.

```python
class Priority(Enum):
    CRITICAL = 1    # repo_overview, current task symbols
    HIGH = 2        # related symbols, call graph neighbors
    MEDIUM = 3      # searched symbols, module listings
    LOW = 4         # historical context, broad searches

@dataclass
class ContextBudget:
    max_context_tokens: int = 100_000  # Claude's window
    reserved_for_response: int = 4_000
    reserved_for_system: int = 2_000

    items: List[ContextItem] = field(default_factory=list)
    current_usage: int = 0
```

**Key behaviors:**
- Reserves space for system prompt and response generation
- Priority-based trimming when approaching limit
- CRITICAL items (repo_overview) never evicted
- Auto-trims LOW priority items to make room for HIGH

### Component: SemforaTools

ADK tool definitions wrapping existing MCP tools.

```python
class SemforaToolset:
    """
    CRITICAL: Prefer query-driven tools over bulk loading.

    CHEAP (use these):
    - search_symbols: ~400 tokens for 20 results
    - list_symbols: ~800 tokens for 50 results
    - get_symbol: ~350 tokens per symbol
    - get_symbol_source: ~400 tokens for 50 lines

    EXPENSIVE (avoid unless necessary):
    - get_module: 8,000-12,000 tokens (loads ALL symbols in module)
    - analyze_directory: Unbounded for large repos

    The symbol_index.jsonl is STREAMED with early exit,
    so queries don't load the full 60MB index for large repos.
    """

    @tool(description="Get high-level repo architecture. BOUNDED to ~150KB max regardless of repo size. Call FIRST.")
    def get_repo_overview(self, path: Optional[str] = None) -> str:
        """Token cost: ~2,500 (fixed, doesn't grow with repo size)"""
        return self._call_mcp("get_repo_overview", {"path": path})

    @tool(description="Search symbols by name. Returns lightweight entries only. Use get_symbol(hash) for details.")
    def search_symbols(self, query: str, module: Optional[str] = None,
                       kind: Optional[str] = None, limit: int = 20) -> str:
        """
        Token cost: ~400 for 20 results (default), ~2000 for 100 results (max)
        Returns: [{symbol, hash, kind, module, file, lines, risk}, ...]
        Next step: get_symbol(hash) for full semantic details
        """
        return self._call_mcp("search_symbols", {...})

    @tool(description="List all symbols in a module. MUCH cheaper than get_module().")
    def list_symbols(self, module: str, kind: Optional[str] = None,
                     limit: int = 50) -> str:
        """
        Token cost: ~800 for 50 results (default), ~3200 for 200 results (max)
        PREFER THIS over get_module() which costs 8-12k tokens
        """
        return self._call_mcp("list_symbols", {...})

    @tool(description="Get detailed semantic info for ONE symbol by hash.")
    def get_symbol(self, symbol_hash: str) -> str:
        """Token cost: ~350 per symbol"""
        return self._call_mcp("get_symbol", {...})

    # EXPENSIVE - use sparingly
    @tool(description="WARNING: Expensive (~8-12k tokens). Prefer list_symbols + get_symbol instead.")
    def get_module(self, module_name: str) -> str:
        """
        Token cost: 8,000-12,000 tokens (loads ALL symbols)
        AVOID: Use list_symbols() + get_symbol() for 4-5x savings
        """
        return self._call_mcp("get_module", {...})
```

**Query-Driven Pattern (REQUIRED):**

```
# BAD: 12,000 tokens
get_module("auth")  # Loads everything

# GOOD: 1,150 tokens (10x cheaper)
list_symbols("auth", limit=20)  # 800 tokens - find relevant symbols
get_symbol("abc123")            # 350 tokens - get just what you need
```

### Component: SemforaOrchestrator

Main orchestrator - controls ALL tool calls, uses Claude API for reasoning only.

```python
class SemforaOrchestrator:
    """
    IMPORTANT: Model B Architecture

    - Orchestrator makes ALL tool decisions (Python code, not LLM)
    - Claude API receives curated context and returns reasoning/code
    - NO tools are exposed to Claude - it just thinks about what we give it

    This is the opposite of Claude Code, where the LLM decides tool usage.
    """

    def __init__(self, repo_path: str, model: str = "anthropic/claude-sonnet-4-20250514"):
        self.memory = SemforaMemory(repo_path=repo_path)
        self.context_budget = ContextBudget()
        self.tools = SemforaToolset(mcp_server_path)
        self.model = model

        # NOTE: We do NOT register tools with an LlmAgent
        # The orchestrator calls tools directly, Claude just reasons

    async def initialize_session(self) -> Dict[str, Any]:
        """Load repo_overview into persistent memory at session start."""
        overview = await self.tools.get_repo_overview(self.repo_path)
        self.memory.load_repo_overview(overview)
        self.context_budget.add_item(overview, Priority.CRITICAL, "repo_overview")
        return {"status": "ready", "overview": overview}

    async def process_task(self, user_input: str) -> str:
        """
        Process user request through bounded semantic context.

        Flow:
        1. Orchestrator gathers relevant context (tool calls in Python)
        2. Claude API reasons about context (no tool access)
        3. Orchestrator applies any changes Claude suggests
        """
        # Step 1: Orchestrator gathers context (WE decide what to fetch)
        context = await self._gather_context_for_task(user_input)

        # Step 2: Claude reasons (NO tools, just curated context)
        response = await self._call_claude_for_reasoning(
            system_prompt=REASONING_PROMPT,
            context=context,
            user_task=user_input
        )

        # Step 3: If Claude suggests edits, orchestrator applies them
        if self._needs_file_edits(response):
            await self._apply_suggested_edits(response)

        return response.content

    async def _gather_context_for_task(self, task: str) -> str:
        """
        Orchestrator decides what semantic context to fetch.
        This is deterministic Python logic, not LLM decisions.
        """
        context_parts = [self.memory.repo_overview]

        # Extract keywords and search (orchestrator logic)
        keywords = self._extract_keywords(task)
        for keyword in keywords[:3]:  # Limit searches
            results = await self.tools.search_symbols(keyword, limit=10)
            context_parts.append(f"Search '{keyword}':\n{results}")

        # Get details for top symbols
        top_symbols = self._rank_symbols(results)
        for symbol_hash in top_symbols[:5]:
            detail = await self.tools.get_symbol(symbol_hash)
            context_parts.append(detail)

        return "\n\n---\n\n".join(context_parts)

    async def _call_claude_for_reasoning(self, system_prompt: str,
                                          context: str, user_task: str) -> str:
        """
        Call Claude API for reasoning. Claude has NO tool access.
        It receives curated context and outputs reasoning/code.
        """
        # Using Anthropic SDK or LiteLLM
        response = await anthropic.messages.create(
            model=self.model,
            system=system_prompt,
            messages=[{
                "role": "user",
                "content": f"## Semantic Context\n{context}\n\n## Task\n{user_task}"
            }]
        )
        return response

# Simple reasoning prompt - no tool instructions needed
REASONING_PROMPT = """You are a code assistant. You receive semantic context about a codebase
(symbol summaries, call graphs, risk levels) and help with coding tasks.

Given the context provided, analyze the request and either:
1. Explain code behavior based on the semantic summaries
2. Suggest specific code changes with file paths and line numbers
3. Identify potential issues or improvements

Be concise and specific. Reference symbols by their names from the context."""
```

**Key Difference from Claude Code:**

| Aspect | Claude Code (Model A) | semfora-cli (Model B) |
|--------|----------------------|----------------------|
| Tool decisions | LLM decides via prompting | Orchestrator decides in Python |
| Context gathering | LLM explores, often wastefully | Orchestrator curates efficiently |
| LLM's job | Explore + Reason + Act | Reason only |
| Memory | None across sessions | Persistent semantic memory |
| Token efficiency | ~25k per task | ~4k per task |

---

## Layer 3: Action Layer (Claude API + File System)

### ClaudeReasoner

For complex architectural decisions that need deeper analysis:

```python
class ClaudeReasoner:
    """
    Uses Claude Opus for:
    - Complex architectural decisions
    - Multi-file refactoring plans
    - Security and risk analysis
    - Code review synthesis
    """

    async def analyze_change_impact(self, semantic_diff, call_graph, intent) -> Dict:
        """Analyze if proposed changes match stated intent."""

    async def plan_refactoring(self, symbols, goal, constraints) -> Dict:
        """Plan multi-file refactoring with dependency ordering."""
```

### FileWriter

Safe file modification with rollback:

```python
class FileWriter:
    """
    - Automatic backups before any modification
    - Diff preview before commit
    - Atomic multi-file operations
    - Rollback on failure
    """

    def start_session(self) -> EditSession:
        """Start edit session with backup directory."""

    def stage_edit(self, file_path, original, modified, description) -> str:
        """Stage edit and return diff preview."""

    def commit(self) -> Tuple[bool, str]:
        """Apply all staged edits atomically."""

    def rollback(self) -> Tuple[bool, str]:
        """Restore files from backup."""
```

---

## Execution Modes

### Mode 1: Interactive CLI (Terminal Editor)

Entry point: `semfora` or `semfora-cli`

```
User: "Add pagination to the users API"

[Orchestrator receives request]

[Parallel Tool Calls]
  search_symbols("users")      → 8 results, 400 tokens
  search_symbols("pagination") → 3 results, 150 tokens

[Sequential Tool Calls]
  get_symbol("abc123")         → getUsersHandler, 350 tokens
  get_symbol("def456")         → PaginationParams, 350 tokens
  get_symbol_source(...)       → Actual code for editing, 400 tokens

[Claude Reasoning]
  "Need to add offset/limit params, update return type, add import"

[FileWriter]
  Stage edit → Preview diff → User confirms → Commit

Total: ~3000 tokens (vs ~25000 without semantic layer)
```

**CLI Commands:**
- Natural language queries for code understanding
- `/search <query>` - Direct symbol search
- `/diff [base] [target]` - Semantic diff
- `/edit` - Start edit session
- `/preview` - Show staged changes
- `/commit` - Apply changes
- `/rollback` - Undo changes
- `/status` - Memory and context state

### Mode 2: CI Mode (Headless Batch Processing)

Entry point: `semfora-ci`

```bash
# PR review
semfora-ci --mode pr --base main --head feature-branch --output json

# Single commit check
semfora-ci --mode commit --commit abc123 --output json

# Pre-commit hook
semfora-ci --mode commit --commit HEAD --output text
```

**Output Structure:**
```json
{
  "status": "changes_requested",  // approved | changes_requested | blocked
  "risk_score": "medium",
  "summary": "New API endpoint with database access",
  "findings": [
    {"severity": "medium", "type": "security", "message": "Missing input validation"},
    {"severity": "low", "type": "testing", "message": "No test coverage for new endpoint"}
  ]
}
```

**Exit Codes:**
- `0` - Approved
- `1` - Changes requested or blocked

---

## Package Structure (Modular Separation)

```
semfora/
  semfora-engine/          # EXISTING - Rust semantic engine (unchanged)
    src/
      cache.rs
      cli.rs
      detectors/
      extract.rs
      git/
      lib.rs
      mcp_server/
      schema.rs
      shard.rs
      toon.rs
    Cargo.toml

  semfora-adk/             # NEW - Python ADK orchestration layer
    semfora_adk/
      __init__.py
      memory.py            # SemforaMemory - persistent semantic context
      tools.py             # SemforaTools - MCP tool wrappers
      context.py           # ContextBudget - token management
      orchestrator.py      # SemforaOrchestrator - main ADK agent
      reasoning.py         # ClaudeReasoner - deep analysis
      writer.py            # FileWriter - safe file modifications
    pyproject.toml

  semfora-cli/             # NEW - Terminal code editor
    semfora_cli/
      __init__.py
      main.py              # Interactive CLI entry point
      commands.py          # Slash command handlers
      ui.py                # Rich terminal UI components
    pyproject.toml

  semfora-ci/              # NEW - CI/CD integration
    semfora_ci/
      __init__.py
      main.py              # CLI for CI pipelines
      github.py            # GitHub Actions helpers
      gitlab.py            # GitLab CI helpers
    pyproject.toml
    action.yml             # GitHub Action definition
```

**Separation rationale:**
- `semfora-engine` stays pure Rust with no Python/ADK dependencies
- `semfora-adk` is the cognitive layer, independent of UI
- `semfora-cli` and `semfora-ci` are thin UI layers over ADK
- Future: `semfora-vscode`, `semfora-neovim`, etc.

---

## Workflow Patterns

### Pattern 1: Understanding → Acting

```
Task: "Explain how authentication works"

1. [Orchestrator] Check memory for repo_overview
2. [Parallel] search_symbols("auth"), search_symbols("login"), search_symbols("session")
3. [Sequential] get_symbol for top results
4. [Optional] get_call_graph to understand flow
5. [Claude] Synthesize explanation from semantic context

NO FILE READS - Pure semantic understanding
```

### Pattern 2: Targeted Modification

```
Task: "Fix the bug in UserService.getById"

1. [Orchestrator] Check memory for repo_overview
2. [Search] search_symbols("UserService")
3. [Detail] get_symbol("abc123") for UserService.getById
4. [Source] get_symbol_source for actual code (editing requires real source)
5. [Claude] Analyze bug and propose fix
6. [FileWriter] Stage edit, preview diff
7. [User] Confirm
8. [FileWriter] Commit

MINIMAL FILE READS - Only what's needed for editing
```

### Pattern 3: Multi-File Refactoring

```
Task: "Rename User to Account across the codebase"

1. [Orchestrator] Check memory for repo_overview
2. [Search] search_symbols("User", limit=100)
3. [Graph] get_call_graph to understand dependencies
4. [Claude] Plan refactoring order (types first, then functions, then tests)
5. [Loop] For each file:
   a. get_symbol_source
   b. Stage edit
6. [FileWriter] Preview all changes
7. [User] Confirm
8. [FileWriter] Atomic commit

SYSTEMATIC - Semantic understanding guides the refactoring
```

### Pattern 4: CI Review

```
PR: "Add user preferences API"

1. [CI] Initialize session
2. [Diff] analyze_diff("main", "feature-branch")
3. [Graph] get_call_graph for changed symbols
4. [ClaudeReasoner] analyze_change_impact with:
   - Semantic diff
   - Call graph
   - PR description (stated intent)
5. [Output] Structured JSON with:
   - status: approved | changes_requested | blocked
   - findings: security, testing, architecture concerns
   - risk_score: low | medium | high

AUTOMATED - No human in the loop
```

---

## Token Budget Comparison

### Without Semantic Layer (Traditional)

```
Task: "Add pagination to users API"

Glob **/*.ts                  → 847 files (response)
Grep "users"                  → 23 matches
Read src/api/users.ts         → 450 lines
Read src/types/user.ts        → 120 lines
Read src/db/queries/users.ts  → 280 lines
Grep "pagination"             → 5 matches
Read src/components/Pagination.tsx → 95 lines
Read src/hooks/usePagination.ts    → 60 lines
... (more exploration)

Total: 15-35 tool calls, ~25,000+ tokens of raw source
```

### With Semantic Layer (Semfora Agent)

```
Task: "Add pagination to users API"

get_repo_overview             → 2500 tokens (knows API is in src/api/)
search_symbols("users")       → 400 tokens (8 results)
search_symbols("pagination")  → 150 tokens (3 results)
get_symbol("getUsersHandler") → 350 tokens
get_symbol("PaginationParams")→ 350 tokens
get_symbol_source(...)        → 400 tokens (only for editing)

Total: 6 tool calls, ~4,150 tokens
```

**Improvement: ~6x fewer tokens, ~3x fewer tool calls**

---

## Large Repository Scalability

The architecture is designed for repos with 60k+ files (3GB+ source code). Here's how each component stays bounded:

### Bounded by Design (Not Linear with Repo Size)

| Component | 2k files | 20k files | 60k files | Bound Type |
|-----------|----------|-----------|-----------|------------|
| repo_overview | 15 KB | 75 KB | 150 KB | **Aggregation** |
| symbol_index.jsonl | 2 MB | 20 MB | 60 MB | **Streamed** |
| Per search_symbols | 400 tok | 400 tok | 400 tok | **Limit 20** |
| Per list_symbols | 800 tok | 800 tok | 800 tok | **Limit 50** |
| Per get_symbol | 350 tok | 350 tok | 350 tok | **Fixed** |

### How repo_overview Stays Small

The overview uses **aggregation, not enumeration**:

```
# GOOD: Fixed size regardless of file count
{
  "total_files": 60000,           # Just a number
  "modules": [                     # ~50-100 summaries
    {"name": "api", "files": 500, "risk": "high"},
    {"name": "components", "files": 2000, "risk": "low"},
    ...
  ],
  "risk_breakdown": "high:5000,medium:20000,low:35000"
}

# BAD (what we avoid): Would grow unboundedly
{
  "files": ["file1.ts", "file2.ts", ... 60000 more ...]
}
```

### Streaming symbol_index.jsonl

For a 60k file repo, the symbol index is ~60MB. But queries use **streaming with early exit**:

```rust
// From cache.rs - streams line by line, stops when limit reached
for line in reader.lines() {
    let entry: SymbolIndexEntry = serde_json::from_str(&line)?;
    if matches_query(&entry, query) {
        results.push(entry);
        if results.len() >= limit {
            break;  // Early exit - don't scan entire 60MB
        }
    }
}
```

**Result:** A search query touching a 60MB index returns in milliseconds because it stops after finding 20 matches.

### Memory Model for ADK

| Tier | What | Size | Lifecycle |
|------|------|------|-----------|
| 0 | Paths only | ~100 B | Session |
| 1 | repo_overview | ~150 KB | Load once, keep forever |
| 2 | Symbol LRU cache | ~500 KB | 1000 symbols, evict LRU |
| 3 | Query result cache | ~50 KB | TTL 5 min |
| - | symbol_index.jsonl | 60 MB | **Never fully loaded** |
| - | Module shards | Skip | Use list_symbols instead |

**Total ADK memory for 3GB repo: ~700KB** (not 60MB+)

### Query Limit Enforcement

Hard-coded limits prevent runaway token usage:

```python
# From mcp_server/types.rs
search_symbols:  limit.min(100)   # Max 100 results
list_symbols:    limit.min(200)   # Max 200 results
get_symbol:      1 symbol only    # Always bounded
```

---

## Implementation Phases

### Phase 1: Foundation (COMPLETED)

**Goal:** Basic ADK agent with MCP tool integration

1. ✅ Created `semfora-adk` package structure
   - Python project with pyproject.toml (uv-managed)
   - LiteLLM and Anthropic API dependencies
   - toon-format library for TOON parsing

2. ✅ Implemented `SemforaTools`
   - CLI subprocess wrapper for semfora-mcp binary
   - TOON output parsing via toon-format library
   - Core tools: analyze_file, analyze_directory, get_repo_overview, analyze_diff
   - Error handling with fallback parsing

3. ✅ Basic `SemforaOrchestrator`
   - Model B architecture (orchestrator controls ALL tools)
   - Claude API for reasoning only (no tool exposure to LLM)
   - Context assembly from semantic summaries

**Deliverable:** Agent that can answer questions using semantic tools - COMPLETED

### Phase 2: Memory and Context (1-2 weeks)

**Goal:** Persistent semantic context across turns

1. `SemforaMemory` implementation
   - LRU caching for modules/symbols
   - Session state persistence
   - Overview preloading on init

2. `ContextBudget` implementation
   - Token estimation (~4 chars/token)
   - Priority-based trimming
   - Context assembly for prompts

**Deliverable:** Agent maintains context across conversation

### Phase 3: CLI Interface (2 weeks)

**Goal:** Interactive terminal editor

1. Basic interactive loop
   - Rich terminal UI
   - Natural language queries
   - Slash commands

2. File editing flow
   - Edit sessions with backup
   - Diff preview
   - Commit/rollback

3. Status and debugging
   - Memory state display
   - Token usage tracking
   - Debug mode

**Deliverable:** Usable terminal code editor

### Phase 4: CI Integration (2 weeks)

**Goal:** Automated PR review in CI pipelines

1. PR review command
   - Structured JSON/text output
   - Exit codes for CI gates
   - Configurable thresholds

2. GitHub Action
   - action.yml definition
   - Container image
   - Usage documentation

**Deliverable:** GitHub Action for automated code review

### Phase 5: Advanced Features (2-3 weeks)

**Goal:** Production-ready agent capabilities

1. Multi-agent patterns
   - Specialist sub-agents (security, testing)
   - Parallel analysis
   - Agent coordination

2. `ClaudeReasoner` for deep analysis
   - Architectural impact assessment
   - Refactoring planning
   - Complex code review

**Deliverable:** Full-featured semantic code editor

---

## Future: Cloud Sync Integration

When the hosted Semfora platform is ready (see Phase 7.0 in plan.md), the agent architecture supports seamless cloud sync:

```python
class SemforaMemory:
    async def sync_from_cloud(self):
        """Download pre-built semantic index from cloud in 1-3 seconds."""
        # Instead of local 30-60s generation
        index = await self.cloud_client.fetch_index(self.repo_remote)
        self.load_from_index(index)

    async def patch_local_diff(self):
        """Apply local uncommitted changes on top of cloud index."""
        # Combines cloud state with local edits
        local_diff = await self.semantic_engine.analyze_uncommitted()
        self.apply_diff_overlay(local_diff)
```

The local ADK agent becomes a **consumer of remote semantic state**, not a builder.

---

## Critical Files

| File | Purpose |
|------|---------|
| `src/mcp_server/mod.rs` | Core MCP server - all 16 tool handlers |
| `src/cache.rs` | CacheDir, SymbolIndexEntry - cache structure |
| `src/schema.rs` | SemanticSummary, RiskLevel - data model |
| `src/shard.rs` | ShardWriter - shard generation |
| `docs/query-driven-architecture.md` | Query workflow documentation |

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Token reduction vs raw source | 80%+ |
| Tool calls per task | < 10 average |
| Session init time | < 5 seconds |
| Context budget compliance | Never exceed 100k |
| CI review latency | < 30 seconds |

---

## Relationship to Plan.md

This document expands on concepts from the main plan:

- **Phase 3.0** (Semantic Sharding): Provides the semantic engine this agent consumes
- **Phase 7.0** (Hosted Platform): Future cloud sync integration
- **Phase 3.0E** (Graph Aggregation): Call graphs used for impact analysis
- **Phase 3.0F** (Detector Modularization): Language support for multi-lang repos

The agent architecture is **orthogonal to and builds upon** the existing engine phases.
