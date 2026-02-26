//! Tests for SQLite schema enrichment - verifying new columns exist and data persists

#[cfg(test)]
mod sqlite_schema_tests {
    use rusqlite::{params, Connection, Result as SqliteResult};

    fn setup_test_db() -> SqliteResult<Connection> {
        let conn = Connection::open_in_memory()?;

        // Create the updated schema with new columns
        conn.execute_batch(
            r#"
            CREATE TABLE nodes (
                hash TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                module TEXT,
                file_path TEXT,
                line_start INTEGER,
                line_end INTEGER,
                risk TEXT DEFAULT 'low',
                complexity INTEGER DEFAULT 0,
                caller_count INTEGER DEFAULT 0,
                callee_count INTEGER DEFAULT 0,
                is_exported INTEGER DEFAULT 0,
                decorators TEXT DEFAULT '',
                framework_entry_point TEXT DEFAULT '',
                arity INTEGER DEFAULT 0,
                is_self_recursive INTEGER DEFAULT 0
            );

            CREATE TABLE edges (
                caller_hash TEXT NOT NULL,
                callee_hash TEXT NOT NULL,
                PRIMARY KEY (caller_hash, callee_hash),
                FOREIGN KEY (caller_hash) REFERENCES nodes(hash),
                FOREIGN KEY (callee_hash) REFERENCES nodes(hash)
            );
            "#,
        )?;

        Ok(conn)
    }

    #[test]
    fn test_schema_has_new_columns() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Verify all new columns exist by trying to insert and select them
        conn.execute(
            "INSERT INTO nodes (hash, name, kind, is_exported, decorators, framework_entry_point, arity)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params!["hash1", "test_func", "function", 1, "@pytest.fixture", "test function", 2],
        )?;

        let mut stmt =
            conn.prepare("SELECT is_exported, decorators, framework_entry_point, arity FROM nodes WHERE hash = ?1")?;
        let mut rows = stmt.query(params!["hash1"])?;

        assert!(
            rows.next()?.is_some(),
            "Row should exist with new columns"
        );

        Ok(())
    }

    #[test]
    fn test_is_exported_column() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Insert exported and non-exported symbols
        conn.execute(
            "INSERT INTO nodes (hash, name, kind, is_exported) VALUES (?, ?, ?, ?)",
            params!["exported1", "public_api", "function", 1],
        )?;

        conn.execute(
            "INSERT INTO nodes (hash, name, kind, is_exported) VALUES (?, ?, ?, ?)",
            params!["private1", "private_fn", "function", 0],
        )?;

        // Query and verify
        let mut stmt = conn.prepare("SELECT is_exported FROM nodes WHERE hash = ?1")?;

        let exported: i32 = stmt.query_row(params!["exported1"], |row| row.get(0))?;
        assert_eq!(exported, 1, "Exported symbol should have is_exported = 1");

        let private: i32 = stmt.query_row(params!["private1"], |row| row.get(0))?;
        assert_eq!(private, 0, "Private symbol should have is_exported = 0");

        Ok(())
    }

    #[test]
    fn test_decorators_column() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        let decorators = "@pytest.fixture,@app.route,@deprecated";
        conn.execute(
            "INSERT INTO nodes (hash, name, kind, decorators) VALUES (?, ?, ?, ?)",
            params!["func1", "decorated_func", "function", decorators],
        )?;

        let mut stmt = conn.prepare("SELECT decorators FROM nodes WHERE hash = ?1")?;
        let retrieved: String = stmt.query_row(params!["func1"], |row| row.get(0))?;

        assert_eq!(retrieved, decorators, "Decorators should be preserved exactly");

        Ok(())
    }

    #[test]
    fn test_empty_decorators() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        conn.execute(
            "INSERT INTO nodes (hash, name, kind, decorators) VALUES (?, ?, ?, ?)",
            params!["func2", "plain_func", "function", ""],
        )?;

        let mut stmt = conn.prepare("SELECT decorators FROM nodes WHERE hash = ?1")?;
        let retrieved: String = stmt.query_row(params!["func2"], |row| row.get(0))?;

        assert!(
            retrieved.is_empty(),
            "Empty decorators should remain empty"
        );

        Ok(())
    }

    #[test]
    fn test_framework_entry_point_column() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Insert different framework entry point types
        conn.execute(
            "INSERT INTO nodes (hash, name, kind, framework_entry_point) VALUES (?, ?, ?, ?)",
            params!["test1", "test_func", "function", "test function"],
        )?;

        conn.execute(
            "INSERT INTO nodes (hash, name, kind, framework_entry_point) VALUES (?, ?, ?, ?)",
            params!["nest1", "app_controller", "class", "NestJS controller"],
        )?;

        conn.execute(
            "INSERT INTO nodes (hash, name, kind, framework_entry_point) VALUES (?, ?, ?, ?)",
            params!["reg1", "normal_func", "function", ""],
        )?;

        // Verify retrieval
        let mut stmt =
            conn.prepare("SELECT framework_entry_point FROM nodes WHERE hash = ?1")?;

        let test_fep: String = stmt.query_row(params!["test1"], |row| row.get(0))?;
        assert_eq!(test_fep, "test function");

        let nest_fep: String = stmt.query_row(params!["nest1"], |row| row.get(0))?;
        assert_eq!(nest_fep, "NestJS controller");

        let reg_fep: String = stmt.query_row(params!["reg1"], |row| row.get(0))?;
        assert!(reg_fep.is_empty(), "Empty framework entry point should be empty string");

        Ok(())
    }

    #[test]
    fn test_arity_column() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        let test_cases = vec![
            ("func0", 0),
            ("func2", 2),
            ("func5", 5),
            ("func12", 12),
        ];

        for (hash, arity) in test_cases.iter() {
            conn.execute(
                "INSERT INTO nodes (hash, name, kind, arity) VALUES (?, ?, ?, ?)",
                params![hash, format!("func_{}", arity), "function", arity],
            )?;
        }

        let mut stmt = conn.prepare("SELECT arity FROM nodes WHERE hash = ?1")?;

        for (hash, expected_arity) in test_cases.iter() {
            let retrieved: i32 = stmt.query_row(params![*hash], |row| row.get(0))?;
            assert_eq!(
                retrieved as usize, *expected_arity,
                "Arity should be preserved for {}",
                hash
            );
        }

        Ok(())
    }

    #[test]
    fn test_is_self_recursive_computation() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Insert nodes
        conn.execute(
            "INSERT INTO nodes (hash, name, kind, is_self_recursive) VALUES (?, ?, ?, ?)",
            params!["func_a", "recursive_func", "function", 0],
        )?;

        conn.execute(
            "INSERT INTO nodes (hash, name, kind, is_self_recursive) VALUES (?, ?, ?, ?)",
            params!["func_b", "regular_func", "function", 0],
        )?;

        // Create a self-recursive edge (func_a calls func_a)
        conn.execute(
            "INSERT INTO edges (caller_hash, callee_hash) VALUES (?, ?)",
            params!["func_a", "func_a"],
        )?;

        // Create a non-recursive edge (func_b calls func_a)
        conn.execute(
            "INSERT INTO edges (caller_hash, callee_hash) VALUES (?, ?)",
            params!["func_b", "func_a"],
        )?;

        // Run the is_self_recursive UPDATE statement
        conn.execute(
            "UPDATE nodes SET is_self_recursive = 1
             WHERE hash IN (
                 SELECT caller_hash FROM edges WHERE caller_hash = callee_hash
             )",
            [],
        )?;

        // Verify results
        let mut stmt = conn.prepare("SELECT is_self_recursive FROM nodes WHERE hash = ?1")?;

        let recursive: i32 = stmt.query_row(params!["func_a"], |row| row.get(0))?;
        assert_eq!(recursive, 1, "func_a should be marked as self-recursive");

        let regular: i32 = stmt.query_row(params!["func_b"], |row| row.get(0))?;
        assert_eq!(regular, 0, "func_b should NOT be marked as self-recursive");

        Ok(())
    }

    #[test]
    fn test_multiple_self_recursive_functions() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Create 3 functions
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO nodes (hash, name, kind) VALUES (?, ?, ?)",
                params![format!("f{}", i), format!("func_{}", i), "function"],
            )?;
        }

        // f1 calls itself
        conn.execute(
            "INSERT INTO edges (caller_hash, callee_hash) VALUES (?, ?)",
            params!["f1", "f1"],
        )?;

        // f2 calls f3 (not recursive)
        conn.execute(
            "INSERT INTO edges (caller_hash, callee_hash) VALUES (?, ?)",
            params!["f2", "f3"],
        )?;

        // f3 calls itself
        conn.execute(
            "INSERT INTO edges (caller_hash, callee_hash) VALUES (?, ?)",
            params!["f3", "f3"],
        )?;

        // Mark self-recursive
        conn.execute(
            "UPDATE nodes SET is_self_recursive = 1
             WHERE hash IN (
                 SELECT caller_hash FROM edges WHERE caller_hash = callee_hash
             )",
            [],
        )?;

        // Verify
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM nodes WHERE is_self_recursive = 1")?;
        let count: i32 = stmt.query_row([], |row| row.get(0))?;

        assert_eq!(count, 2, "Exactly 2 functions should be self-recursive (f1 and f3)");

        Ok(())
    }

    #[test]
    fn test_default_values_for_new_columns() -> SqliteResult<()> {
        let conn = setup_test_db()?;

        // Insert without specifying new columns - should use defaults
        conn.execute(
            "INSERT INTO nodes (hash, name, kind) VALUES (?, ?, ?)",
            params!["test_hash", "test_name", "function"],
        )?;

        let mut stmt = conn.prepare(
            "SELECT is_exported, decorators, framework_entry_point, arity, is_self_recursive
             FROM nodes WHERE hash = ?1",
        )?;

        let mut rows = stmt.query(params!["test_hash"])?;
        let row = rows.next()?.expect("Row should exist");

        let is_exported: i32 = row.get(0)?;
        let decorators: String = row.get(1)?;
        let framework_entry_point: String = row.get(2)?;
        let arity: i32 = row.get(3)?;
        let is_self_recursive: i32 = row.get(4)?;

        assert_eq!(is_exported, 0, "Default is_exported should be 0");
        assert!(decorators.is_empty(), "Default decorators should be empty");
        assert!(
            framework_entry_point.is_empty(),
            "Default framework_entry_point should be empty"
        );
        assert_eq!(arity, 0, "Default arity should be 0");
        assert_eq!(is_self_recursive, 0, "Default is_self_recursive should be 0");

        Ok(())
    }
}
