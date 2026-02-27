//! SQLite Export for Call Graph Data
//!
//! Exports semantic index and call graph data to SQLite for external visualization tools
//! like semfora-graph. Designed for streaming to handle millions/billions of edges.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::{params, Connection};

use crate::schema::CallGraphEdge;
use crate::{CacheDir, McpDiffError, Result, SymbolIndexEntry};

/// Export statistics returned after successful export
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportStats {
    /// Number of nodes inserted
    pub nodes_inserted: usize,
    /// Number of edges inserted
    pub edges_inserted: usize,
    /// Number of module-level edges inserted
    pub module_edges_inserted: usize,
    /// Number of import relationships inserted
    pub imports_inserted: usize,
    /// Number of inheritance edges inserted
    pub inheritance_inserted: usize,
    /// Export duration in milliseconds
    pub duration_ms: u64,
    /// Output file path
    pub output_path: String,
    /// Final file size in bytes
    pub file_size_bytes: u64,
}

/// Progress callback type for streaming exports
pub type ProgressCallback = Box<dyn Fn(ExportProgress) + Send>;

/// Progress information during export
#[derive(Debug, Clone)]
pub struct ExportProgress {
    pub phase: ExportPhase,
    pub current: usize,
    pub total: usize,
    pub message: String,
}

/// Export phases
#[derive(Debug, Clone, Copy)]
pub enum ExportPhase {
    CreatingSchema,
    InsertingNodes,
    InsertingEdges,
    ComputingModuleEdges,
    UpdatingCounts,
    CreatingIndexes,
    Finalizing,
}

/// SQLite exporter for call graph data
pub struct SqliteExporter {
    batch_size: usize,
}

impl Default for SqliteExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteExporter {
    /// Create a new SQLite exporter with default batch size (5000)
    pub fn new() -> Self {
        Self { batch_size: 5000 }
    }

    /// Set the batch size for transactions
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.clamp(100, 50000);
        self
    }

    /// Export call graph to SQLite file
    ///
    /// Streams data from cache to avoid memory blowout on large graphs.
    pub fn export(
        &self,
        cache: &CacheDir,
        output_path: &Path,
        progress: Option<ProgressCallback>,
        include_escape_refs: bool,
    ) -> Result<ExportStats> {
        let start = Instant::now();

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Remove existing file if present
        if output_path.exists() {
            fs::remove_file(output_path)?;
        }

        // Open connection
        let mut conn = Connection::open(output_path).map_err(|e| McpDiffError::ExportError {
            message: format!("Failed to create SQLite database: {}", e),
        })?;

        // Report progress
        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::CreatingSchema,
                current: 0,
                total: 0,
                message: "Creating schema...".to_string(),
            });
        }

        // Create schema
        Self::create_schema(&conn)?;

        // Insert nodes and get module mapping for edges
        let (nodes_inserted, node_modules) =
            self.insert_nodes_streaming(&mut conn, cache, &progress, include_escape_refs)?;

        // Insert edges and collect module edge counts
        let (edges_inserted, module_edge_counts) = self.insert_edges_streaming(
            &mut conn,
            cache,
            &node_modules,
            &progress,
            include_escape_refs,
        )?;

        // Insert module-level edges
        let module_edges_inserted =
            self.insert_module_edges(&mut conn, module_edge_counts, &progress)?;

        // Insert import-graph (module-level import dependencies)
        let imports_inserted = self.insert_imports(&mut conn, cache)?;

        // Insert inheritance edges
        let inheritance_inserted = self.insert_inheritance(&mut conn, cache)?;

        // Update caller/callee counts
        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::UpdatingCounts,
                current: 0,
                total: 0,
                message: "Updating node counts...".to_string(),
            });
        }
        Self::update_counts(&conn)?;

        // Create indexes after bulk insert (faster)
        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::CreatingIndexes,
                current: 0,
                total: 0,
                message: "Creating indexes...".to_string(),
            });
        }
        Self::create_indexes(&conn)?;

        // Finalize
        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::Finalizing,
                current: 0,
                total: 0,
                message: "Finalizing...".to_string(),
            });
        }

        // Get file size
        let file_size_bytes = fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

        Ok(ExportStats {
            nodes_inserted,
            edges_inserted,
            module_edges_inserted,
            imports_inserted,
            inheritance_inserted,
            duration_ms: start.elapsed().as_millis() as u64,
            output_path: output_path.display().to_string(),
            file_size_bytes,
        })
    }

    /// Create SQLite schema
    fn create_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- Schema metadata
            CREATE TABLE schema_info (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT INTO schema_info VALUES ('version', '1.0');
            INSERT INTO schema_info VALUES ('created_at', datetime('now'));
            INSERT INTO schema_info VALUES ('generator', 'semfora-engine');

            -- Nodes table: symbols from the codebase
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
                is_self_recursive INTEGER DEFAULT 0,
                is_async INTEGER DEFAULT 0,
                return_type TEXT DEFAULT '',
                ext_package TEXT DEFAULT '',
                base_classes TEXT DEFAULT ''
            );

            -- Edges table: function call relationships
            -- edge_kind: 'call' (function), 'read' (variable read), 'write' (variable write), 'readwrite' (compound)
            CREATE TABLE edges (
                caller_hash TEXT NOT NULL,
                callee_hash TEXT NOT NULL,
                call_count INTEGER DEFAULT 1,
                edge_kind TEXT NOT NULL DEFAULT 'call',
                PRIMARY KEY (caller_hash, callee_hash, edge_kind)
            );

            -- Module-level aggregated edges for high-level visualization
            CREATE TABLE module_edges (
                caller_module TEXT NOT NULL,
                callee_module TEXT NOT NULL,
                edge_count INTEGER NOT NULL,
                PRIMARY KEY (caller_module, callee_module)
            );

            -- Module-level import dependencies (from import statements, not call graph)
            -- Gives a clean dependency DAG separate from the call graph;
            -- also catches dead modules (imported but never called)
            CREATE TABLE imports (
                importer_module TEXT NOT NULL,
                imported_module TEXT NOT NULL,
                import_count INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (importer_module, imported_module)
            );

            -- Inheritance edges: class X extends/implements Y
            CREATE TABLE inheritance (
                child_hash TEXT NOT NULL,
                parent_hash TEXT NOT NULL,
                child_module TEXT,
                parent_module TEXT,
                parent_name TEXT NOT NULL,
                PRIMARY KEY (child_hash, parent_hash)
            );
            "#,
        )
        .map_err(|e| McpDiffError::ExportError {
            message: format!("Failed to create schema: {}", e),
        })?;

        Ok(())
    }

    /// Stream nodes from symbol index to SQLite
    fn insert_nodes_streaming(
        &self,
        conn: &mut Connection,
        cache: &CacheDir,
        progress: &Option<ProgressCallback>,
        include_escape_refs: bool,
    ) -> Result<(usize, HashMap<String, String>)> {
        let index_path = cache.symbol_index_path();
        if !index_path.exists() {
            return Err(McpDiffError::FileNotFound {
                path: index_path.display().to_string(),
            });
        }

        let file = fs::File::open(&index_path)?;
        let reader = BufReader::new(file);

        // Track hash -> module for edge processing
        let mut node_modules: HashMap<String, String> = HashMap::new();
        let mut batch: Vec<SymbolIndexEntry> = Vec::with_capacity(self.batch_size);
        let mut total_inserted = 0;

        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::InsertingNodes,
                current: 0,
                total: 0,
                message: "Reading symbol index...".to_string(),
            });
        }

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let entry: SymbolIndexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.is_escape_local && !include_escape_refs {
                continue;
            }

            // Track module for this hash
            node_modules.insert(entry.hash.clone(), entry.module.clone());
            batch.push(entry);

            if batch.len() >= self.batch_size {
                total_inserted += self.flush_node_batch(conn, &batch)?;

                if let Some(ref cb) = progress {
                    cb(ExportProgress {
                        phase: ExportPhase::InsertingNodes,
                        current: total_inserted,
                        total: 0,
                        message: format!("Inserted {} nodes...", total_inserted),
                    });
                }

                batch.clear();
            }
        }

        // Flush remaining
        if !batch.is_empty() {
            total_inserted += self.flush_node_batch(conn, &batch)?;
        }

        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::InsertingNodes,
                current: total_inserted,
                total: total_inserted,
                message: format!("Inserted {} nodes", total_inserted),
            });
        }

        Ok((total_inserted, node_modules))
    }

    /// Flush a batch of nodes to SQLite
    fn flush_node_batch(&self, conn: &mut Connection, batch: &[SymbolIndexEntry]) -> Result<usize> {
        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR REPLACE INTO nodes
                     (hash, name, kind, module, file_path, line_start, line_end, risk, complexity,
                      is_exported, decorators, framework_entry_point, arity,
                      is_async, return_type, ext_package, base_classes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare failed: {}", e),
                })?;

            for entry in batch {
                let (start, end) = parse_line_range(&entry.lines);
                let fep_str = if entry.framework_entry_point.is_none() {
                    String::new()
                } else {
                    entry.framework_entry_point.description().to_string()
                };
                stmt.execute(params![
                    entry.hash,
                    entry.symbol,
                    entry.kind,
                    entry.module,
                    entry.file,
                    start,
                    end,
                    entry.risk,
                    entry.cognitive_complexity as i64,
                    entry.is_exported as i64,
                    entry.decorators,
                    fep_str,
                    entry.arity as i64,
                    entry.is_async as i64,
                    entry.return_type,
                    entry.ext_package,
                    entry.base_classes,
                ])
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Insert failed: {}", e),
                })?;
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(batch.len())
    }

    /// Stream edges from call_graph.toon to SQLite
    fn insert_edges_streaming(
        &self,
        conn: &mut Connection,
        cache: &CacheDir,
        node_modules: &HashMap<String, String>,
        progress: &Option<ProgressCallback>,
        include_escape_refs: bool,
    ) -> Result<(usize, HashMap<(String, String), usize>)> {
        let graph_path = cache.call_graph_path();
        if !graph_path.exists() {
            // No call graph yet - return empty
            return Ok((0, HashMap::new()));
        }

        let file = fs::File::open(&graph_path)?;
        let reader = BufReader::new(file);

        // Track module-level edges for aggregation
        let mut module_edge_counts: HashMap<(String, String), usize> = HashMap::new();

        // Track external nodes we need to create: hash -> (name, module)
        let mut external_nodes: HashMap<String, (String, String)> = HashMap::new();

        // Batch: (caller_hash, callee_hash, edge_kind)
        let mut batch: Vec<(String, String, String)> = Vec::with_capacity(self.batch_size);
        let mut total_inserted = 0;

        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::InsertingEdges,
                current: 0,
                total: 0,
                message: "Reading call graph...".to_string(),
            });
        }

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            // Skip header lines
            if line.is_empty()
                || line.starts_with("_type:")
                || line.starts_with("schema_version:")
                || line.starts_with("edges:")
            {
                continue;
            }

            // Parse "caller_hash: [callee1, callee2:read, callee3:write, ...]" format
            // Note: hash may contain colons (two-part format), so find ": ["
            if let Some(bracket_pos) = line.find(": [") {
                let caller = line[..bracket_pos].trim();
                let rest = line[bracket_pos + 2..].trim();

                // Parse the array part
                if rest.starts_with('[') && rest.ends_with(']') {
                    let inner = &rest[1..rest.len() - 1];

                    // Split by comma while respecting quoted strings
                    for callee_str in split_respecting_quotes(inner) {
                        // Parse edge with optional edge_kind suffix
                        let edge = CallGraphEdge::decode(callee_str);
                        if edge.edge_kind.is_escape_ref() && !include_escape_refs {
                            continue;
                        }
                        let callee = &edge.callee;
                        let edge_kind = edge.edge_kind.as_edge_kind().to_string();

                        // Handle external calls - create node with kind='external'
                        // Format: ext:package:symbol or ext:symbol (no package)
                        if callee.starts_with("ext:") {
                            let rest = &callee[4..]; // Remove "ext:" prefix
                            let (ext_module, ext_name) = if let Some(colon_pos) = rest.find(':') {
                                // Has package: ext:package:symbol
                                let pkg = &rest[..colon_pos];
                                let name = &rest[colon_pos + 1..];
                                (pkg.to_string(), name.to_string())
                            } else {
                                // No package: ext:symbol
                                ("__external__".to_string(), rest.to_string())
                            };
                            external_nodes.insert(callee.to_string(), (ext_name, ext_module));
                        }

                        // Track module-level edges (only for known nodes)
                        let caller_mod = node_modules.get(caller).cloned();
                        let callee_mod = if callee.starts_with("ext:") {
                            // Parse package from ext:package:symbol or use __external__
                            let rest = &callee[4..];
                            if let Some(colon_pos) = rest.find(':') {
                                Some(rest[..colon_pos].to_string())
                            } else {
                                Some("__external__".to_string())
                            }
                        } else {
                            node_modules.get(callee.as_str()).cloned()
                        };

                        if let (Some(cm), Some(ce)) = (caller_mod, callee_mod) {
                            *module_edge_counts.entry((cm, ce)).or_default() += 1;
                        }

                        batch.push((caller.to_string(), callee.clone(), edge_kind));

                        if batch.len() >= self.batch_size {
                            total_inserted += self.flush_edge_batch(conn, &batch)?;

                            if let Some(ref cb) = progress {
                                cb(ExportProgress {
                                    phase: ExportPhase::InsertingEdges,
                                    current: total_inserted,
                                    total: 0,
                                    message: format!("Inserted {} edges...", total_inserted),
                                });
                            }

                            batch.clear();
                        }
                    }
                }
            }
        }

        // Flush remaining edges
        if !batch.is_empty() {
            total_inserted += self.flush_edge_batch(conn, &batch)?;
        }

        // Insert external nodes
        if !external_nodes.is_empty() {
            self.insert_external_nodes(conn, &external_nodes)?;
        }

        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::InsertingEdges,
                current: total_inserted,
                total: total_inserted,
                message: format!(
                    "Inserted {} edges ({} external calls)",
                    total_inserted,
                    external_nodes.len()
                ),
            });
        }

        Ok((total_inserted, module_edge_counts))
    }

    /// Flush a batch of edges to SQLite with edge_kind
    fn flush_edge_batch(
        &self,
        conn: &mut Connection,
        batch: &[(String, String, String)],
    ) -> Result<usize> {
        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO edges (caller_hash, callee_hash, edge_kind, call_count)
                     VALUES (?1, ?2, ?3, 1)
                     ON CONFLICT(caller_hash, callee_hash, edge_kind) DO UPDATE SET call_count = call_count + 1",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare failed: {}", e),
                })?;

            for (caller, callee, edge_kind) in batch {
                stmt.execute(params![caller, callee, edge_kind])
                    .map_err(|e| McpDiffError::ExportError {
                        message: format!("Insert edge failed: {}", e),
                    })?;
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(batch.len())
    }

    /// Insert external call nodes
    /// external_nodes maps hash -> (name, module)
    fn insert_external_nodes(
        &self,
        conn: &mut Connection,
        external_nodes: &HashMap<String, (String, String)>,
    ) -> Result<()> {
        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO nodes
                     (hash, name, kind, module, risk, ext_package)
                     VALUES (?1, ?2, 'external', ?3, 'low', ?4)",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare failed: {}", e),
                })?;

            for (hash, (name, module)) in external_nodes {
                // ext_package: use module name when it's a real package (not __external__)
                let pkg = if module != "__external__" {
                    module.as_str()
                } else {
                    ""
                };
                stmt.execute(params![hash, name, module, pkg])
                    .map_err(|e| McpDiffError::ExportError {
                        message: format!("Insert external node failed: {}", e),
                    })?;
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(())
    }

    /// Insert module-level aggregated edges
    fn insert_module_edges(
        &self,
        conn: &mut Connection,
        module_edge_counts: HashMap<(String, String), usize>,
        progress: &Option<ProgressCallback>,
    ) -> Result<usize> {
        if module_edge_counts.is_empty() {
            return Ok(0);
        }

        if let Some(ref cb) = progress {
            cb(ExportProgress {
                phase: ExportPhase::ComputingModuleEdges,
                current: 0,
                total: module_edge_counts.len(),
                message: "Inserting module edges...".to_string(),
            });
        }

        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        let count = module_edge_counts.len();

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO module_edges (caller_module, callee_module, edge_count)
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare failed: {}", e),
                })?;

            for ((caller_mod, callee_mod), edge_count) in module_edge_counts {
                stmt.execute(params![caller_mod, callee_mod, edge_count as i64])
                    .map_err(|e| McpDiffError::ExportError {
                        message: format!("Insert module edge failed: {}", e),
                    })?;
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(count)
    }

    /// Populate the `imports` table from the module_graph.toon cache file.
    ///
    /// The module graph encodes module-level import dependencies, giving a
    /// clean DAG separate from the call graph. It also surfaces dead modules
    /// (imported but never called).
    fn insert_imports(&self, conn: &mut Connection, cache: &CacheDir) -> Result<usize> {
        let path = cache.module_graph_path();
        if !path.exists() {
            return Ok(0);
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Ok(0),
        };

        // Parse TOON module graph: each data line is  "module": ["dep1", "dep2"]
        let mut pairs: Vec<(String, String)> = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("_type:")
                || line.starts_with("schema_version:")
                || line.starts_with("modules:")
                || line.is_empty()
            {
                continue;
            }
            // Parse: "importer": ["dep1","dep2"]
            if let Some(colon_pos) = line.find("\": [") {
                let importer = line[1..colon_pos].to_string(); // strip leading "
                let deps_str = &line[colon_pos + 4..]; // skip ": ["
                let deps_str = deps_str.trim_end_matches(']');
                for dep in deps_str.split(',') {
                    let dep = dep.trim().trim_matches('"');
                    if !dep.is_empty() {
                        pairs.push((importer.clone(), dep.to_string()));
                    }
                }
            }
        }

        if pairs.is_empty() {
            return Ok(0);
        }

        let count = pairs.len();
        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR REPLACE INTO imports (importer_module, imported_module, import_count)
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare failed: {}", e),
                })?;

            for (importer, imported) in &pairs {
                stmt.execute(params![importer, imported, 1_i64])
                    .map_err(|e| McpDiffError::ExportError {
                        message: format!("Insert import failed: {}", e),
                    })?;
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(count)
    }

    /// Populate the `inheritance` table from base_classes stored on class nodes.
    ///
    /// For each node where `base_classes` is non-empty, tries to resolve each
    /// parent name to a hash in the nodes table. Unresolved parents are still
    /// inserted with a synthetic hash so the relationship is queryable.
    fn insert_inheritance(&self, conn: &mut Connection, _cache: &CacheDir) -> Result<usize> {
        // Collect (child_hash, child_module, parent_name) for all class nodes with base_classes
        let rows: Vec<(String, String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT hash, module, base_classes FROM nodes
                     WHERE base_classes != '' AND base_classes IS NOT NULL",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare inheritance query failed: {}", e),
                })?;

            let rows: std::result::Result<Vec<_>, _> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Query inheritance nodes failed: {}", e),
                })?
                .collect();

            rows.map_err(|e| McpDiffError::ExportError {
                message: format!("Collect inheritance nodes failed: {}", e),
            })?
        };

        if rows.is_empty() {
            return Ok(0);
        }

        let tx = conn.transaction().map_err(|e| McpDiffError::ExportError {
            message: format!("Transaction failed: {}", e),
        })?;

        let mut count = 0;
        {
            let mut insert_stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO inheritance
                     (child_hash, parent_hash, child_module, parent_module, parent_name)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare inheritance insert failed: {}", e),
                })?;

            let mut lookup_stmt = tx
                .prepare_cached(
                    "SELECT hash, module FROM nodes
                     WHERE name = ?1 AND kind IN ('class', 'interface', 'trait')
                     LIMIT 1",
                )
                .map_err(|e| McpDiffError::ExportError {
                    message: format!("Prepare inheritance lookup failed: {}", e),
                })?;

            for (child_hash, child_module, base_classes_str) in &rows {
                for parent_name in base_classes_str.split(',') {
                    let parent_name = parent_name.trim();
                    if parent_name.is_empty() {
                        continue;
                    }

                    // Try to resolve parent name to a hash in the nodes table
                    let (parent_hash, parent_module) = lookup_stmt
                        .query_row(params![parent_name], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        })
                        .unwrap_or_else(|_| {
                            // Unresolved: create a synthetic hash so the edge is still stored
                            (format!("unresolved:{}", parent_name), String::new())
                        });

                    insert_stmt
                        .execute(params![
                            child_hash,
                            parent_hash,
                            child_module,
                            parent_module,
                            parent_name,
                        ])
                        .map_err(|e| McpDiffError::ExportError {
                            message: format!("Insert inheritance failed: {}", e),
                        })?;
                    count += 1;
                }
            }
        }

        tx.commit().map_err(|e| McpDiffError::ExportError {
            message: format!("Commit failed: {}", e),
        })?;

        Ok(count)
    }

    /// Update caller/callee counts on nodes
    fn update_counts(conn: &Connection) -> Result<()> {
        conn.execute(
            "UPDATE nodes SET callee_count = (
                SELECT COUNT(*) FROM edges WHERE edges.caller_hash = nodes.hash
            )",
            [],
        )
        .map_err(|e| McpDiffError::ExportError {
            message: format!("Update callee_count failed: {}", e),
        })?;

        conn.execute(
            "UPDATE nodes SET caller_count = (
                SELECT COUNT(*) FROM edges WHERE edges.callee_hash = nodes.hash
            )",
            [],
        )
        .map_err(|e| McpDiffError::ExportError {
            message: format!("Update caller_count failed: {}", e),
        })?;

        // Mark self-recursive nodes (symbols that call themselves)
        conn.execute(
            "UPDATE nodes SET is_self_recursive = 1
             WHERE hash IN (
                 SELECT caller_hash FROM edges WHERE caller_hash = callee_hash
             )",
            [],
        )
        .map_err(|e| McpDiffError::ExportError {
            message: format!("Update is_self_recursive failed: {}", e),
        })?;

        Ok(())
    }

    /// Create indexes after bulk insert (faster than creating before)
    fn create_indexes(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- Node indexes for queries
            CREATE INDEX idx_nodes_name ON nodes(name);
            CREATE INDEX idx_nodes_module ON nodes(module);
            CREATE INDEX idx_nodes_kind ON nodes(kind);
            CREATE INDEX idx_nodes_risk ON nodes(risk);
            CREATE INDEX idx_nodes_file ON nodes(file_path);
            CREATE INDEX idx_nodes_caller_count ON nodes(caller_count DESC);
            CREATE INDEX idx_nodes_callee_count ON nodes(callee_count DESC);

            -- Edge indexes for traversal
            CREATE INDEX idx_edges_caller ON edges(caller_hash);
            CREATE INDEX idx_edges_callee ON edges(callee_hash);

            -- Module edge indexes
            CREATE INDEX idx_module_edges_caller ON module_edges(caller_module);
            CREATE INDEX idx_module_edges_callee ON module_edges(callee_module);
            CREATE INDEX idx_module_edges_count ON module_edges(edge_count DESC);
            "#,
        )
        .map_err(|e| McpDiffError::ExportError {
            message: format!("Failed to create indexes: {}", e),
        })?;

        Ok(())
    }
}

/// Get default export path for a repository
pub fn default_export_path(cache: &CacheDir) -> PathBuf {
    cache.root.join("call_graph.sqlite")
}

/// Split a string by comma while respecting quoted strings.
/// Handles entries like: "foo","bar","ext:baz(a, b, c).unwrap"
fn split_respecting_quotes(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b',' if !in_quotes => {
                let part = s[start..i].trim();
                if !part.is_empty() {
                    result.push(part);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last part
    let part = s[start..].trim();
    if !part.is_empty() {
        result.push(part);
    }

    result
}

/// Parse line range string (e.g., "45-89") into (start, end)
fn parse_line_range(lines: &str) -> (Option<i64>, Option<i64>) {
    if lines.is_empty() {
        return (None, None);
    }

    let parts: Vec<&str> = lines.split('-').collect();
    match parts.as_slice() {
        [start] => {
            let s = start.parse::<i64>().ok();
            (s, s)
        }
        [start, end] => {
            let s = start.parse::<i64>().ok();
            let e = end.parse::<i64>().ok();
            (s, e)
        }
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_range() {
        assert_eq!(parse_line_range("45-89"), (Some(45), Some(89)));
        assert_eq!(parse_line_range("100"), (Some(100), Some(100)));
        assert_eq!(parse_line_range(""), (None, None));
        assert_eq!(parse_line_range("abc"), (None, None));
    }

    #[test]
    fn test_batch_size_clamping() {
        let exporter = SqliteExporter::new().with_batch_size(10);
        assert_eq!(exporter.batch_size, 100); // Minimum

        let exporter = SqliteExporter::new().with_batch_size(100000);
        assert_eq!(exporter.batch_size, 50000); // Maximum

        let exporter = SqliteExporter::new().with_batch_size(5000);
        assert_eq!(exporter.batch_size, 5000); // Normal
    }
}
