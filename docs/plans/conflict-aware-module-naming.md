# Plan: Conflict-Aware Module Name Stripping

## Status: Phase 1 Complete, Phase 2 Planned

**Phase 1 (Complete):** Core algorithm implemented in `src/shard.rs`
**Phase 2 (This Plan):** SQLite-backed registry for incremental indexing

---

## Overview

Replace hardcoded source markers (`/src/`, `/packages/`, etc.) with an adaptive algorithm that strips common prefixes from module names while ensuring uniqueness. Phase 1 implemented the core algorithm with in-memory registry; Phase 2 adds SQLite persistence for incremental indexing at terabyte scale.

---

## Why SQLite (Not JSON)

### The Problem at Scale

| Modules | JSON Registry Size | JSON Load Time | Memory Peak |
|---------|-------------------|----------------|-------------|
| 100K    | ~20MB             | ~500ms         | ~60MB       |
| 1M      | ~200MB            | ~5s            | ~600MB      |
| 10M     | ~2GB              | ~30s           | ~6GB        |

JSON requires loading the entire file for any operation. At terabyte scale (10M+ modules), this is unacceptable.

### SQLite Advantages

1. **O(1) point lookups** - no full file scan
2. **Incremental writes** - change 1 entry, write 1 row
3. **No memory spike** - streaming access
4. **ACID guarantees** - crash safe
5. **Already integrated** - `rusqlite 0.32` in Cargo.toml, patterns in `sqlite_export.rs`

### Access Patterns

| Operation | Pattern | SQLite | JSON |
|-----------|---------|--------|------|
| Full index | Bulk write | O(n) | O(n) |
| Incremental add | Point lookup + insert | O(1) | O(n) |
| Conflict check | Point lookup by short name | O(1) | O(n) |
| MCP query | Point lookup both directions | O(1) | O(n) |

---

## Schema

```sql
-- File: .cache/semfora/<repo_hash>/module_registry.sqlite

-- Schema metadata
CREATE TABLE schema_info (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO schema_info VALUES ('version', '1');
INSERT INTO schema_info VALUES ('created_at', datetime('now'));

-- Module registry
CREATE TABLE modules (
    full_path TEXT PRIMARY KEY,      -- src.game.player
    short_name TEXT NOT NULL UNIQUE, -- game.player (after stripping)
    shard_path TEXT NOT NULL,        -- modules/game.player.toon
    file_path TEXT                   -- /abs/path/to/player.rs
);

-- Global metadata
CREATE TABLE registry_meta (
    key TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
INSERT INTO registry_meta VALUES ('strip_depth', 0);
INSERT INTO registry_meta VALUES ('module_count', 0);

-- Indexes (created after bulk insert)
CREATE INDEX idx_short_name ON modules(short_name);
CREATE INDEX idx_file_path ON modules(file_path);
```

**Size estimate:** ~100 bytes/row → 10M modules = ~1GB SQLite file (with indexes)

---

## Implementation

### File: `src/module_registry.rs` (New)

```rust
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use crate::error::Result;

/// SQLite-backed module name registry for conflict-aware naming
pub struct ModuleRegistrySqlite {
    conn: Connection,
    strip_depth: usize,
}

impl ModuleRegistrySqlite {
    /// Open or create registry at cache path
    pub fn open(cache_dir: &Path) -> Result<Self> {
        let db_path = cache_dir.join("module_registry.sqlite");
        let conn = Connection::open(&db_path)?;

        // Check if schema exists
        let has_schema: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='modules'",
                [],
                |_| Ok(true)
            )
            .unwrap_or(false);

        if !has_schema {
            Self::create_schema(&conn)?;
        }

        let strip_depth = Self::load_strip_depth(&conn)?;

        Ok(Self { conn, strip_depth })
    }

    /// Bulk insert during full index (follows sqlite_export.rs patterns)
    pub fn bulk_insert(&mut self, modules: &[(String, String, String, String)]) -> Result<()> {
        // modules: [(full_path, short_name, shard_path, file_path), ...]

        let tx = self.conn.transaction()?;

        // Clear existing data
        tx.execute("DELETE FROM modules", [])?;

        // Batch insert
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO modules (full_path, short_name, shard_path, file_path)
                 VALUES (?1, ?2, ?3, ?4)"
            )?;

            for (full, short, shard, file) in modules {
                stmt.execute(params![full, short, shard, file])?;
            }
        }

        // Update count
        tx.execute(
            "UPDATE registry_meta SET value = ?1 WHERE key = 'module_count'",
            [modules.len() as i64]
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Point lookup: full path → short name
    pub fn get_short_name(&self, full_path: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT short_name FROM modules WHERE full_path = ?1",
                [full_path],
                |row| row.get(0)
            )
            .ok()
    }

    /// Point lookup: short name → full path (for conflict detection)
    pub fn get_full_path(&self, short_name: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT full_path FROM modules WHERE short_name = ?1",
                [short_name],
                |row| row.get(0)
            )
            .ok()
    }

    /// Check if short name already exists (O(1) conflict detection)
    pub fn has_conflict(&self, short_name: &str) -> bool {
        self.get_full_path(short_name).is_some()
    }

    /// Incremental add with conflict detection
    pub fn add_module(
        &mut self,
        full_path: &str,
        file_path: &str,
    ) -> Result<AddResult> {
        let short_name = strip_n_components(full_path, self.strip_depth);

        // Check for conflict
        if let Some(existing_full) = self.get_full_path(&short_name) {
            if existing_full != full_path {
                return Ok(AddResult::Conflict {
                    new_full: full_path.to_string(),
                    existing_full,
                    conflicting_short: short_name,
                });
            }
        }

        // No conflict - insert
        let shard_path = format!("modules/{}.toon", short_name);
        self.conn.execute(
            "INSERT OR REPLACE INTO modules (full_path, short_name, shard_path, file_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![full_path, short_name, shard_path, file_path]
        )?;

        Ok(AddResult::Added { short_name })
    }

    /// Handle conflict by expanding both names
    pub fn resolve_conflict(
        &mut self,
        new_full: &str,
        existing_full: &str,
    ) -> Result<(String, String)> {
        // Find minimum expansion to disambiguate
        let new_expanded = expand_until_unique(new_full, existing_full, self.strip_depth);
        let existing_expanded = expand_until_unique(existing_full, new_full, self.strip_depth);

        // Update existing module's short name
        let old_shard = self.get_shard_path(existing_full)?;
        let new_shard = format!("modules/{}.toon", existing_expanded);

        self.conn.execute(
            "UPDATE modules SET short_name = ?1, shard_path = ?2 WHERE full_path = ?3",
            params![existing_expanded, new_shard, existing_full]
        )?;

        // Insert new module with expanded name
        let new_shard_path = format!("modules/{}.toon", new_expanded);
        self.conn.execute(
            "INSERT INTO modules (full_path, short_name, shard_path, file_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![new_full, new_expanded, new_shard_path, ""] // file_path filled later
        )?;

        Ok((new_expanded, existing_expanded))
    }

    /// Remove module (incremental delete)
    pub fn remove_module(&mut self, full_path: &str) -> Result<Option<String>> {
        let short_name = self.get_short_name(full_path);
        self.conn.execute(
            "DELETE FROM modules WHERE full_path = ?1",
            [full_path]
        )?;
        Ok(short_name)
    }

    /// Get all mappings (for overview generation)
    pub fn get_all_mappings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT full_path, short_name FROM modules"
        )?;

        let mappings = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(mappings)
    }

    // ... private helpers ...
}

pub enum AddResult {
    Added { short_name: String },
    Conflict { new_full: String, existing_full: String, conflicting_short: String },
}
```

### Integration with ShardWriter

```rust
// In src/shard.rs

impl ShardWriter {
    pub fn write_all(&mut self) -> Result<()> {
        // 1. Compute optimal names (existing Phase 1 code)
        self.compute_module_registry();

        // 2. Persist to SQLite
        if let Some(ref registry) = self.module_registry {
            let mut sqlite_registry = ModuleRegistrySqlite::open(&self.cache.root)?;

            let entries: Vec<_> = self.modules.iter()
                .map(|(full, summaries)| {
                    let short = registry.get_short(full).unwrap_or(full.clone());
                    let shard = format!("modules/{}.toon", short);
                    let file = summaries.first().map(|s| s.file.clone()).unwrap_or_default();
                    (full.clone(), short, shard, file)
                })
                .collect();

            sqlite_registry.bulk_insert(&entries)?;
        }

        // 3. Write shards (existing code)
        self.write_module_shards()?;
        self.write_repo_overview()?;

        Ok(())
    }
}
```

---

## Helper Functions

```rust
/// Strip first N components from dotted path
fn strip_n_components(name: &str, n: usize) -> String {
    name.split('.').skip(n).collect::<Vec<_>>().join(".")
}

/// Expand name until it differs from other
fn expand_until_unique(target: &str, other: &str, current_depth: usize) -> String {
    for depth in (0..current_depth).rev() {
        let target_expanded = strip_n_components(target, depth);
        let other_expanded = strip_n_components(other, depth);
        if target_expanded != other_expanded {
            return target_expanded;
        }
    }
    target.to_string() // Fully expanded
}
```

---

## Performance

### Full Index (Bulk Insert)

Following `sqlite_export.rs` patterns:
- Use transaction for entire bulk insert
- `prepare_cached` for repeated statements
- Create indexes AFTER bulk insert

| Modules | Time | File Size |
|---------|------|-----------|
| 100K    | ~1s  | ~10MB     |
| 1M      | ~10s | ~100MB    |
| 10M     | ~90s | ~1GB      |

### Incremental Operations

All O(1) with indexes:
- `get_short_name`: ~0.1ms
- `has_conflict`: ~0.1ms
- `add_module`: ~1ms
- `remove_module`: ~1ms

---

## Migration Strategy

### Phase 2a: Add SQLite Backend (Non-Breaking)

1. Add `module_registry.rs` with `ModuleRegistrySqlite`
2. Modify `ShardWriter::write_all()` to persist after computing
3. Keep in-memory registry for current operations
4. SQLite is write-only initially (for testing)

### Phase 2b: Read from SQLite

1. On `ShardWriter::new()`, check for existing `module_registry.sqlite`
2. If exists and fresh, load `strip_depth` from SQLite
3. Use SQLite for conflict detection in incremental mode

### Phase 2c: Full Incremental Support

1. `add_file()` uses `ModuleRegistrySqlite::add_module()`
2. Conflict triggers shard rename + SQLite update
3. `remove_file()` uses `ModuleRegistrySqlite::remove_module()`

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/module_registry.rs` | **NEW** - SQLite-backed registry |
| `src/shard.rs` | Import and use `ModuleRegistrySqlite` |
| `src/cache.rs` | Add `module_registry_path()` helper |
| `src/lib.rs` | Export `module_registry` module |

---

## Testing Strategy

### Unit Tests (module_registry.rs)

```rust
#[test]
fn test_bulk_insert_and_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let mut reg = ModuleRegistrySqlite::open(dir.path()).unwrap();

    reg.bulk_insert(&[
        ("src.game.player".into(), "game.player".into(), "...".into(), "...".into()),
        ("src.game.enemy".into(), "game.enemy".into(), "...".into(), "...".into()),
    ]).unwrap();

    assert_eq!(reg.get_short_name("src.game.player"), Some("game.player".into()));
    assert_eq!(reg.get_full_path("game.player"), Some("src.game.player".into()));
}

#[test]
fn test_conflict_detection() {
    let dir = tempfile::tempdir().unwrap();
    let mut reg = ModuleRegistrySqlite::open(dir.path()).unwrap();

    reg.bulk_insert(&[
        ("src.game.player".into(), "player".into(), "...".into(), "...".into()),
    ]).unwrap();

    assert!(reg.has_conflict("player"));
    assert!(!reg.has_conflict("enemy"));
}

#[test]
fn test_incremental_add() {
    // ... test AddResult::Added and AddResult::Conflict
}

#[test]
fn test_conflict_resolution() {
    // ... test resolve_conflict expands both names correctly
}
```

### Integration Tests

```rust
#[test]
fn test_full_index_persists_to_sqlite() {
    // Index a test repo, verify module_registry.sqlite exists
}

#[test]
fn test_registry_survives_restart() {
    // Index, restart, verify registry loaded correctly
}
```

---

## Rollback Plan

1. SQLite file includes `version` in `schema_info`
2. If issues, delete `module_registry.sqlite` → falls back to in-memory
3. `--force` reindex regenerates everything

---

## Success Criteria

1. ✅ No hardcoded directory lists (Phase 1 complete)
2. ⬜ SQLite registry persisted after full index
3. ⬜ Point lookups in <1ms at 10M modules
4. ⬜ Incremental add/remove without full reindex
5. ⬜ All existing tests pass
6. ⬜ Memory usage <100MB for registry operations at any scale
