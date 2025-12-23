# Plan: Cross‑App Variable and State Tracking (All Languages/Libraries)

## Goal
Preserve and expand existing functionality to track functions/classes/methods/structs and variable instances across the entire application, including:
- React props passed down the component tree (same variable identity, not re‑created per hop).
- Redux state read/write/readwrite across the application (same state slot identity).
- ECS component presence/reads/writes (e.g., Bevy `Health`, `Position`) tied to entity identity.
- Token‑efficient output that preserves high‑value semantic detail.

This plan **does not simplify away** semantics; it re‑organizes detection so it scales to 5+ libraries and 26 languages while retaining deep tracking.

---

## Phase 0: Inventory and Invariants (No Behavior Change)
1. **Catalog current outputs**
   - `SemanticSummary`, `SymbolInfo`, `Call`, `FrameworkEntryPoint`, insertions, call graph edges, module graph.
   - Identify where Redux/React data flows are already emitted (e.g., `summary.calls`, `summary.symbols`).
2. **Define invariants**
   - A prop/state/component must map to one identity across files.
   - Reads/writes must attach to that identity, not generate new variables.
   - Output must remain compact but reconstructable.

Deliverable: a short doc listing current outputs and invariants with tests that must not regress.

---

## Phase 1: Unified Identity Model
Introduce a **language‑agnostic identity model** to unify tracking across frameworks:

### Core identity types
- `SymbolIdentity`: function/class/method/struct definition
- `ValueIdentity`: variable instance (local, param, prop, state slot, component)
- `StateSlot`: redux or store state slice path (e.g., `auth.accessToken`)
- `PropSlot`: component prop name + component identity
- `ComponentSlot`: ECS component type + entity identity
- `EntityIdentity`: ECS entity (stable or inferred identity)

### Core edge types
- `read`, `write`, `readwrite`
- `alias` (x references y)
- `pass` (argument flow)
- `return` (return flow)
- `store` (value assigned to slot)
- `dispatch` / `selector` / `effect` (library semantics)
- `component_add` / `component_remove` / `component_read`

Deliverable: schema definitions and mapping table from existing outputs to new identity edges.

---

## Phase 2: Data‑Driven Pattern Manifest (Routing Layer)
Replace ad‑hoc string matching with manifest files that **route** to extractors.

### Manifest captures
- Imports/exports signatures (with alias resolution)
- Call signatures (function/method/member calls)
- File/path conventions
- AST query fragments (tree‑sitter patterns)

### Example manifest entry (conceptual)
```toml
[library.redux]
imports = ["redux", "react-redux", "@reduxjs/toolkit"]
calls = ["createSlice", "configureStore", "createAsyncThunk", "useSelector", "useDispatch"]
queries = ["(call_expression function: (identifier) @fn)" ]

[semantics]
route_to = "redux_extractor"
```

Deliverable: manifest schema + initial Redux/React/Bevy entries (no behavior change).

---

## Phase 3: Two‑Stage Extraction (Per Language)
Split extraction into two deterministic stages:

### Stage A: Normalized IR (language‑specific)
Emit a language‑neutral IR:
- `call`, `assign`, `member_access`, `destructure`, `return`, `import`, `export`, `type_ref`
- Line/column ranges for identity mapping

### Stage B: Library Semantics (manifest‑driven)
Use the manifest to route IR nodes into existing library logic:
- Redux: action → state write edges, selector reads, thunk lifecycle, etc.
- React: prop flow down tree (alias edges)
- Bevy: entity/component insertions and query reads

Deliverable: normalized IR emitter for JS/TS, then Rust (Bevy), with a compatibility bridge so existing detectors still run.

---

## Phase 4: Global Identity Resolution (Cross‑File)
Build an identity resolver that connects symbol/value identities across modules:

### Resolution inputs
- Imports/exports (named, default, re‑export, barrel files)
- Aliasing (e.g., `import { createSlice as cs }`)
- Type hints / signatures (TS/Rust) where available

### Outputs
- Stable `SymbolIdentity` hashes across files
- `ValueIdentity` link graph that merges repeated references

Deliverable: resolver that de‑dupes variables across files and allows global flow queries.

---

## Phase 5: Preserve and Extend Redux Semantics
Retain existing Redux detail but make it identity‑aware and alias‑safe:

- **Action type constants**: map to `SymbolIdentity` and link to reducer writes.
- **Reducers**: detect classic and RTK; emit `write` edges to `StateSlot`.
- **Selectors**: map reads to `StateSlot` using resolved identity of `state`.
- **Thunks**: lifecycle edges to `StateSlot` and side effects.
- **RTK Query**: `createApi` endpoints and cache state slots.

Deliverable: Redux extractor updated to consume normalized IR and emit identity‑aware edges.

---

## Phase 6: React Prop Flow Semantics
- Create `PropSlot` identities tied to component identity.
- Track prop destructuring and pass‑through as `alias`/`pass` edges.
- When a prop flows through multiple components, it remains the same `ValueIdentity`.

Deliverable: React extractor that builds a prop flow graph with stable identities.

---

## Phase 7: ECS / Bevy Semantics
- `commands.spawn().insert(Component)` → `component_add` on `EntityIdentity`.
- `query::<&Component>()` → `component_read` from the same `ComponentSlot`.
- `query::<&mut Component>()` → `component_write`.
- Tie `System` functions to component slots they read/write.

Deliverable: Bevy ECS extractor and identity mappings (Rust).

---

## Phase 8: Token‑Efficient Output Format
Produce a compact output optimized for LLMs:

- Identity hashes (`id`) + short names
- Minimal edge lists with counts
- Key paths for state/props/components
- Summaries grouped by module

Example (conceptual):
```json
{
  "symbol": "authReducer",
  "id": "s:1a2b",
  "writes": ["state.auth.accessToken"],
  "reads": ["state.auth.user"],
  "edges": 6
}
```

Deliverable: encoder updates to include identity‑aware summaries without bloating token count.

---

## Phase 9: Validation and Regression Suite
Create fixtures across languages/libraries:

- React props: `App -> Layout -> Page -> Card`
- Redux: classic + RTK + RTK Query
- Bevy ECS: components + queries + systems

Assertions:
- Same prop/state/component identity across files
- No duplicate variables for the same slot
- Read/write edges correctly tagged

Deliverable: test suite covering at least JS/TS + Rust with extensible structure.

---

## Phase 10: Rollout Strategy
- Introduce IR + manifest behind feature flag.
- Keep existing detectors as fallback until parity reached.
- Migrate library by library, language by language.

Deliverable: migration checklist with clear parity milestones.

---

## Summary of What This Preserves
- Existing Redux/React detection and symbol creation.
- Rich state/prop/component flow tracking.
- Call graph / symbol graph functionality.
- Token‑efficient summaries for LLMs.

## Summary of What This Adds
- Cross‑file identity resolution for stable variables.
- ECS/Bevy semantics integrated in the same identity model.
- Data‑driven pattern routing that scales to 26 languages and 5+ libraries.

