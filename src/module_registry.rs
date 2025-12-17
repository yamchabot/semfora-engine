//! SQLite-backed module name registry for conflict-aware naming at scale.
//!
//! Persists the mapping between full module paths and optimally shortened names
//! to enable O(1) lookups during incremental indexing.
//!
//! The SQLite file is stored in the cache directory alongside other index files,
//! ensuring it gets cleared when the cache is deleted.

use crate::cache::CacheDir;
use crate::error::{McpDiffError, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;

/// SQLite-backed module name registry
///
/// Provides persistent storage for the module name mappings computed by
/// the conflict-aware stripping algorithm. This enables:
/// - O(1) lookups by full path or short name
/// - Persistence across indexing sessions
/// - Foundation for incremental indexing (Phase 2b/2c)
pub struct ModuleRegistrySqlite {
    conn: Connection,
    db_path: PathBuf,
}

impl ModuleRegistrySqlite {
    /// Open or create registry at cache directory
    ///
    /// The database is created at `<cache_root>/module_registry.sqlite`.
    /// Schema is automatically created if the database is new.
    pub fn open(cache: &CacheDir) -> Result<Self> {
        let db_path = cache.module_registry_path();
        let conn = Connection::open(&db_path).map_err(|e| McpDiffError::IoError {
            path: db_path.clone(),
            message: format!("Failed to open module registry: {}", e),
        })?;

        // Check if schema exists
        let has_schema: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='modules'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !has_schema {
            Self::create_schema(&conn, &db_path)?;
        }

        Ok(Self { conn, db_path })
    }

    /// Create database schema
    fn create_schema(conn: &Connection, db_path: &PathBuf) -> Result<()> {
        conn.execute_batch(
            r#"
            -- Schema metadata
            CREATE TABLE schema_info (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT INTO schema_info VALUES ('version', '1');
            INSERT INTO schema_info VALUES ('created_at', datetime('now'));

            -- Module registry
            CREATE TABLE modules (
                full_path TEXT PRIMARY KEY,
                short_name TEXT NOT NULL,
                file_path TEXT
            );

            -- Global metadata
            CREATE TABLE registry_meta (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            INSERT INTO registry_meta VALUES ('strip_depth', 0);
            INSERT INTO registry_meta VALUES ('module_count', 0);

            -- Index for reverse lookup (short -> full)
            CREATE UNIQUE INDEX idx_short_name ON modules(short_name);
            "#,
        )
        .map_err(|e| McpDiffError::IoError {
            path: db_path.clone(),
            message: format!("Failed to create schema: {}", e),
        })?;

        Ok(())
    }

    /// Bulk insert modules (used during full index)
    ///
    /// This clears any existing data and inserts all modules in a single transaction.
    /// Follows patterns from sqlite_export.rs: transaction wrapping, prepare_cached.
    ///
    /// # Arguments
    /// * `entries` - Tuples of (full_path, short_name, file_path)
    /// * `strip_depth` - Number of path components stripped from all modules
    pub fn bulk_insert(
        &mut self,
        entries: &[(String, String, String)],
        strip_depth: usize,
    ) -> Result<()> {
        let tx = self.conn.transaction().map_err(|e| McpDiffError::IoError {
            path: self.db_path.clone(),
            message: format!("Transaction failed: {}", e),
        })?;

        // Clear existing data
        tx.execute("DELETE FROM modules", [])
            .map_err(|e| McpDiffError::IoError {
                path: self.db_path.clone(),
                message: format!("Clear failed: {}", e),
            })?;

        // Batch insert
        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO modules (full_path, short_name, file_path) VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| McpDiffError::IoError {
                    path: self.db_path.clone(),
                    message: format!("Prepare failed: {}", e),
                })?;

            for (full, short, file) in entries {
                stmt.execute(params![full, short, file])
                    .map_err(|e| McpDiffError::IoError {
                        path: self.db_path.clone(),
                        message: format!("Insert failed: {}", e),
                    })?;
            }
        }

        // Update metadata
        tx.execute(
            "UPDATE registry_meta SET value = ?1 WHERE key = 'strip_depth'",
            [strip_depth as i64],
        )
        .map_err(|e| McpDiffError::IoError {
            path: self.db_path.clone(),
            message: format!("Update strip_depth failed: {}", e),
        })?;

        tx.execute(
            "UPDATE registry_meta SET value = ?1 WHERE key = 'module_count'",
            [entries.len() as i64],
        )
        .map_err(|e| McpDiffError::IoError {
            path: self.db_path.clone(),
            message: format!("Update module_count failed: {}", e),
        })?;

        tx.commit().map_err(|e| McpDiffError::IoError {
            path: self.db_path.clone(),
            message: format!("Commit failed: {}", e),
        })?;

        Ok(())
    }

    /// Get short name for a full path (O(1) lookup)
    pub fn get_short_name(&self, full_path: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT short_name FROM modules WHERE full_path = ?1",
                [full_path],
                |row| row.get(0),
            )
            .ok()
    }

    /// Get full path for a short name (O(1) reverse lookup)
    pub fn get_full_path(&self, short_name: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT full_path FROM modules WHERE short_name = ?1",
                [short_name],
                |row| row.get(0),
            )
            .ok()
    }

    /// Check if short name exists (for conflict detection)
    pub fn has_short_name(&self, short_name: &str) -> bool {
        self.get_full_path(short_name).is_some()
    }

    /// Get stored strip depth
    pub fn get_strip_depth(&self) -> usize {
        self.conn
            .query_row(
                "SELECT value FROM registry_meta WHERE key = 'strip_depth'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v as usize)
            .unwrap_or(0)
    }

    /// Get module count
    pub fn get_module_count(&self) -> usize {
        self.conn
            .query_row(
                "SELECT value FROM registry_meta WHERE key = 'module_count'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v as usize)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_open_creates_schema() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        let reg = ModuleRegistrySqlite::open(&cache).unwrap();
        assert_eq!(reg.get_module_count(), 0);
        assert_eq!(reg.get_strip_depth(), 0);
    }

    #[test]
    fn test_bulk_insert_and_lookup() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();

        let entries = vec![
            (
                "src.game.player".to_string(),
                "game.player".to_string(),
                "/path/player.rs".to_string(),
            ),
            (
                "src.game.enemy".to_string(),
                "game.enemy".to_string(),
                "/path/enemy.rs".to_string(),
            ),
        ];

        reg.bulk_insert(&entries, 1).unwrap();

        assert_eq!(reg.get_module_count(), 2);
        assert_eq!(reg.get_strip_depth(), 1);
        assert_eq!(
            reg.get_short_name("src.game.player"),
            Some("game.player".to_string())
        );
        assert_eq!(
            reg.get_full_path("game.enemy"),
            Some("src.game.enemy".to_string())
        );
    }

    #[test]
    fn test_has_short_name() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();

        let entries = vec![(
            "src.player".to_string(),
            "player".to_string(),
            "".to_string(),
        )];
        reg.bulk_insert(&entries, 1).unwrap();

        assert!(reg.has_short_name("player"));
        assert!(!reg.has_short_name("enemy"));
    }

    #[test]
    fn test_bulk_insert_replaces_existing() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();

        // First insert
        let entries1 = vec![(
            "src.old".to_string(),
            "old".to_string(),
            "".to_string(),
        )];
        reg.bulk_insert(&entries1, 1).unwrap();
        assert_eq!(reg.get_module_count(), 1);
        assert!(reg.has_short_name("old"));

        // Second insert replaces
        let entries2 = vec![
            ("src.new.a".to_string(), "a".to_string(), "".to_string()),
            ("src.new.b".to_string(), "b".to_string(), "".to_string()),
        ];
        reg.bulk_insert(&entries2, 2).unwrap();

        assert_eq!(reg.get_module_count(), 2);
        assert_eq!(reg.get_strip_depth(), 2);
        assert!(!reg.has_short_name("old")); // Old data cleared
        assert!(reg.has_short_name("a"));
        assert!(reg.has_short_name("b"));
    }

    #[test]
    fn test_reopen_preserves_data() {
        let dir = tempdir().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        // Insert data
        {
            let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();
            let entries = vec![(
                "src.test".to_string(),
                "test".to_string(),
                "/path/test.rs".to_string(),
            )];
            reg.bulk_insert(&entries, 1).unwrap();
        }

        // Reopen and verify
        {
            let reg = ModuleRegistrySqlite::open(&cache).unwrap();
            assert_eq!(reg.get_module_count(), 1);
            assert_eq!(reg.get_strip_depth(), 1);
            assert_eq!(reg.get_short_name("src.test"), Some("test".to_string()));
        }
    }
}
