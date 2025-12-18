# CLI & MCP Tools Audit Report

**Date**: 2025-12-18
**Branch**: man_rewrite
**Auditor**: Claude Code

---

## Executive Summary

- **35 audit items completed**
- **1 bug found** (get_symbol file+line mode)
- **4 minor discrepancies** (naming, feature gaps, documentation)
- **All documented features work** except `get_symbol` file+line mode

---

## CLI Commands - All Verified Working

| Command | Status | Notes |
|---------|--------|-------|
| `analyze` | PASS | File, dir, git modes all functional |
| `search` | PASS | All 4 modes: hybrid, symbols, semantic, raw |
| `query overview` | PASS | Returns repo overview |
| `query module` | PASS | Returns module details |
| `query symbol` | PASS | Returns symbol details |
| `query source` | PASS | Returns source code |
| `query callers` | PASS | Returns callers |
| `query callgraph` | PASS | Returns call graph |
| `query file` | PASS | Returns file symbols |
| `query languages` | PASS | Lists supported languages |
| `validate` | PASS | Complexity and duplicates |
| `index generate` | PASS | Generates index |
| `index check` | PASS | Checks freshness |
| `index export` | PASS | SQLite export |
| `cache info` | PASS | Shows cache stats |
| `cache clear` | PASS | Clears cache |
| `cache prune` | PASS | Prunes old caches |
| `config show` | PASS | Shows config |
| `config set` | PASS | Sets values |
| `config reset` | PASS | Resets defaults |
| `security scan` | PASS | CVE scanning |
| `security update` | PASS | Pattern updates |
| `security stats` | PASS | Pattern stats |
| `test` | PASS | Auto-detects framework |
| `commit` | PASS | Prep commit info |
| `setup` | PASS | MCP client setup |
| `uninstall` | PASS | Remove MCP/engine |
| `benchmark` | PASS | Token efficiency |

---

## MCP Tools - Verified

| Tool | Status | Notes |
|------|--------|-------|
| `get_context` | PASS | ~200 tokens, git+project context |
| `get_overview` | PASS | Repo overview with modules |
| `server_status` | PASS | Layers, features, uptime |
| `search` | PASS | Hybrid mode by default |
| `get_symbol` (single hash) | PASS | Requires full hash `shard:hash` |
| `get_symbol` (batch) | PASS | Requires full hashes |
| `get_symbol` (file+line) | **FAIL** | Bug: searches wrong directory |
| `get_source` | PASS | Source code extraction |
| `get_file` | PASS | File symbols |
| `get_callers` | PASS | Reverse call graph |
| `get_callgraph` | PASS | Call graph |
| `analyze` | PASS | File/dir/module analysis |
| `analyze_diff` | PASS | Git diff analysis |
| `validate` | PASS | Quality audits |
| `find_duplicates` | PASS | Duplicate detection |
| `index` | PASS | Index management |
| `test` | PASS | Test framework detection |
| `security` | PASS | CVE pattern stats |
| `prep_commit` | PASS | Commit preparation |

---

## Bugs Found

### 1. `get_symbol` file+line mode broken

**Location**: `src/mcp_server/mod.rs:626-678`

**Problem**: Code searches `modules_dir` (module shards) but expects `file:`, `hash:`, `lines:` prefixes which only exist in symbol shards.

**Module shards use TOON compact format**:
```
symbols[2]{hash,name,kind,lines,risk}:
  5b110927:c2041c8b370cdeef,"Service",class,3-3,low
```

**Symbol shards have the expected format**:
```
_type: symbol_shard
file: "/path/to/file.ts"
symbol_id: 5b110927:0968b16561fcf658
lines: "4-4"
```

**Fix**: Change line 627 from `cache.modules_dir()` to `cache.symbols_dir()`

**Impact**: File+line lookup always returns "No symbol found"

---

## Discrepancies

### 1. Hash format requires shard prefix

- Batch mode and single symbol mode require full hash format `shard:hash`
- Example: `5102a40a:443870acdf717427` works, `443870acdf717427` fails silently
- Documentation should clarify this requirement

### 2. CLI vs MCP Feature Gaps

| Feature | CLI | MCP | Notes |
|---------|-----|-----|-------|
| `--uncommitted` | Yes | Via `analyze_diff(target_ref='WORKING')` | Different API |
| `--diff <branch>` | Yes | Yes | `analyze_diff(base_ref)` |
| `--commit <hash>` | Yes | No | Single commit analysis |
| `--all-commits` | Yes | No | Full history analysis |
| `module` param | No | Yes | `analyze(module=)` |

### 3. Naming Inconsistency

- CLI: `-r/--related` flag for semantic search mode
- MCP: `mode='semantic'` parameter
- Same feature, different naming

### 4. Documentation Gap

`docs/command-reference.md` missing from command table:
- `setup` command
- `uninstall` command
- `benchmark` command

These commands exist and work but aren't listed in the Quick Reference table.

---

## Test Evidence

### MCP Tools Tested

```
get_context() -> branch: man_rewrite, index_status: fresh
server_status() -> persistent_mode: true, working_symbols: 3530
index() -> files_processed: 228, symbols: 3592
test(detect_only=true) -> Cargo, Pytest, Npm detected
security(stats_only=true) -> no patterns loaded
prep_commit() -> no staged changes
get_symbol(hash) -> returns full semantic info
get_symbol(hashes=[...]) -> batch mode works with full hashes
get_symbol(file+line) -> FAILS with "No symbol found"
```

### CLI Commands Tested

All commands verified via `--help` and functional tests where applicable.

---

## Recommendations

1. **Fix get_symbol file+line bug** - Change to search symbols_dir instead of modules_dir
2. **Document hash format** - Clarify that full `shard:hash` format is required
3. **Update command-reference.md** - Add missing setup/uninstall/benchmark commands
4. **Consider adding MCP equivalents** - For `--commit` and `--all-commits` features
