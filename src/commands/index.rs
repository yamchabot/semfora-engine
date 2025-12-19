//! Index command handler - Manage the semantic index

use std::fs;
use std::path::PathBuf;

use crate::cache::CacheDir;
use crate::cli::{IndexArgs, IndexOperation, OutputFormat};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::shard::ShardWriter;
use crate::Lang;

/// Run the index command
pub fn run_index(args: &IndexArgs, ctx: &CommandContext) -> Result<String> {
    match &args.operation {
        IndexOperation::Generate {
            path,
            force,
            incremental,
            max_depth,
            extensions,
        } => run_generate(
            path.clone(),
            *force,
            *incremental,
            *max_depth,
            extensions.clone(),
            ctx,
        ),
        IndexOperation::Check {
            auto_refresh,
            max_age,
        } => run_check(*auto_refresh, *max_age, ctx),
        IndexOperation::Export { path } => run_export(path.clone(), ctx),
    }
}

/// Generate or refresh the semantic index
fn run_generate(
    path: Option<PathBuf>,
    force: bool,
    incremental: bool,
    max_depth: usize,
    extensions: Vec<String>,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir =
        path.unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let cache = CacheDir::for_repo(&repo_dir)?;

    // Check if we should skip (unless force)
    if !force && cache.exists() {
        // Check freshness
        let meta_path = cache.root.join("meta.json");
        if meta_path.exists() {
            if let Ok(meta_content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                    if let Some(indexed_at) = meta.get("indexed_at").and_then(|v| v.as_str()) {
                        if let Ok(indexed_time) = chrono::DateTime::parse_from_rfc3339(indexed_at) {
                            let age = chrono::Utc::now().signed_duration_since(indexed_time);
                            if age < chrono::Duration::hours(1) && !incremental {
                                return Ok(format!(
                                    "Index is fresh ({}). Use --force to regenerate.",
                                    indexed_at
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if incremental && cache.exists() {
        // For incremental, just check if anything changed
        // For now, we just regenerate if forced or no cache exists
        return Ok("Incremental mode: Index exists. Use --force to regenerate.".to_string());
    }

    run_full_index(&repo_dir, &cache, max_depth, &extensions, ctx)
}

/// Run full index generation
fn run_full_index(
    repo_dir: &std::path::Path,
    cache: &CacheDir,
    max_depth: usize,
    extensions: &[String],
    ctx: &CommandContext,
) -> Result<String> {
    // Clear existing cache
    if cache.exists() {
        cache.clear()?;
    }

    // Collect files
    let files = collect_files(repo_dir, max_depth, extensions)?;

    if files.is_empty() {
        return Ok("No supported files found to index.".to_string());
    }

    if ctx.verbose {
        eprintln!("Indexing {} files...", files.len());
    }

    // Create shard writer (takes repo path)
    let mut writer = ShardWriter::new(repo_dir)?;

    // Process each file and collect summaries
    let mut summaries = Vec::new();
    let mut errors = 0;

    for (i, file_path) in files.iter().enumerate() {
        if ctx.progress && i % 50 == 0 {
            eprintln!(
                "Progress: {}/{} ({:.0}%)",
                i,
                files.len(),
                (i as f64 / files.len() as f64) * 100.0
            );
        }

        match process_file_for_index(file_path) {
            Ok(summary) => summaries.push(summary),
            Err(e) => {
                if ctx.verbose {
                    eprintln!("Error processing {}: {}", file_path.display(), e);
                }
                errors += 1;
            }
        }
    }

    // Add all summaries and write
    writer.add_summaries(summaries.clone());
    let stats = writer.write_all(&repo_dir.display().to_string())?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_generate",
        "action": "generate",
        "path": repo_dir.to_string_lossy(),
        "files_found": files.len(),
        "files_processed": summaries.len(),
        "errors": errors,
        "modules": stats.modules_written,
        "symbols": stats.symbols_written
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("Index generation complete:\n");
            output.push_str(&format!("  path: {}\n", repo_dir.display()));
            output.push_str(&format!("  files_found: {}\n", files.len()));
            output.push_str(&format!("  files_processed: {}\n", summaries.len()));
            output.push_str(&format!("  errors: {}\n", errors));
            output.push_str(&format!("  modules: {}\n", stats.modules_written));
            output.push_str(&format!("  symbols: {}\n", stats.symbols_written));
        }
    }

    Ok(output)
}

/// Check if the index is fresh or stale
fn run_check(auto_refresh: bool, max_age: u64, ctx: &CommandContext) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        if auto_refresh {
            return run_full_index(&repo_dir, &cache, 10, &[], ctx);
        }
        return Ok("No index found. Run `semfora index generate` to create one.".to_string());
    }

    // Read metadata
    let meta_path = cache.root.join("meta.json");
    if !meta_path.exists() {
        if auto_refresh {
            return run_full_index(&repo_dir, &cache, 10, &[], ctx);
        }
        return Ok(
            "Index metadata not found. Run `semfora index generate` to regenerate.".to_string(),
        );
    }

    let meta_content = fs::read_to_string(&meta_path)?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_content).map_err(|e| McpDiffError::GitError {
            message: format!("Parse error: {}", e),
        })?;

    let indexed_at = meta
        .get("indexed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let is_stale = if let Ok(indexed_time) = chrono::DateTime::parse_from_rfc3339(indexed_at) {
        let age = chrono::Utc::now().signed_duration_since(indexed_time);
        age.num_seconds() as u64 > max_age
    } else {
        true
    };

    if is_stale && auto_refresh {
        eprintln!("Index is stale. Refreshing...");
        return run_full_index(&repo_dir, &cache, 10, &[], ctx);
    }

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_check",
        "path": repo_dir.to_string_lossy(),
        "indexed_at": indexed_at,
        "is_stale": is_stale,
        "max_age_seconds": max_age
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("path: {}\n", repo_dir.display()));
            output.push_str(&format!("indexed_at: {}\n", indexed_at));
            output.push_str(&format!(
                "status: {}\n",
                if is_stale { "STALE" } else { "FRESH" }
            ));
            if is_stale {
                output.push_str("\nRun `semfora index generate` to refresh.\n");
            }
        }
    }

    Ok(output)
}

/// Export the index to SQLite
fn run_export(path: Option<String>, ctx: &CommandContext) -> Result<String> {
    use crate::sqlite_export::{default_export_path, SqliteExporter};

    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    let output_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path(&cache));

    if ctx.verbose {
        eprintln!("Exporting to: {}", output_path.display());
    }

    let exporter = SqliteExporter::new();
    let stats = exporter.export(&cache, &output_path, None)?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "index_export",
        "path": output_path.to_string_lossy(),
        "nodes": stats.nodes_inserted,
        "edges": stats.edges_inserted,
        "file_size": stats.file_size_bytes
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("Export complete:\n");
            output.push_str(&format!("  path: {}\n", output_path.display()));
            output.push_str(&format!("  nodes: {}\n", stats.nodes_inserted));
            output.push_str(&format!("  edges: {}\n", stats.edges_inserted));
            output.push_str(&format!("  file_size: {} bytes\n", stats.file_size_bytes));
        }
    }

    Ok(output)
}

// ============================================
// Helper Functions
// ============================================

/// Collect files to index
fn collect_files(
    dir: &std::path::Path,
    max_depth: usize,
    extensions: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, max_depth, 0, extensions, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(
    _root: &std::path::Path,
    dir: &std::path::Path,
    max_depth: usize,
    current_depth: usize,
    extensions: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    if current_depth > max_depth {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| McpDiffError::FileNotFound {
        path: format!("{}: {}", dir.display(), e),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files and common non-source directories
        if file_name.starts_with('.')
            || file_name == "node_modules"
            || file_name == "target"
            || file_name == "dist"
            || file_name == "build"
        {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(
                _root,
                &path,
                max_depth,
                current_depth + 1,
                extensions,
                files,
            )?;
        } else if path.is_file() {
            // Check extension filter
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if !extensions.is_empty() && !extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
                {
                    continue;
                }

                // Check if it's a supported language
                if Lang::from_path(&path).is_ok() {
                    files.push(path);
                }
            }
        }
    }

    Ok(())
}

/// Process a single file for indexing
fn process_file_for_index(file_path: &std::path::Path) -> Result<crate::SemanticSummary> {
    let lang = Lang::from_path(file_path)?;
    let source = fs::read_to_string(file_path)?;

    // Parse
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language for {}: {}", file_path.display(), e),
        })?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: format!("Failed to parse file: {}", file_path.display()),
        })?;

    // Extract
    crate::extract::extract(file_path, &source, &tree, lang)
}
