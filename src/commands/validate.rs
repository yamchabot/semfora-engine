//! Validate command handler - Quality audits (complexity, duplicates, impact)

use std::collections::BTreeMap;
use std::path::Path;

use crate::cache::{load_function_signatures, CacheDir};
use crate::cli::{OutputFormat, SymbolScope, ValidateArgs};
use crate::commands::CommandContext;
use crate::duplicate::DuplicateKind;
use crate::error::{McpDiffError, Result};
use crate::mcp_server::helpers::{
    find_symbol_by_hash, find_symbol_by_location, format_batch_validation_results,
    format_validation_result, validate_single_symbol, validate_symbols_batch,
};
use crate::normalize_kind;
use crate::{DuplicateDetector, FunctionSignature};

/// Run the validate command - unified validation with auto scope detection
///
/// Scope priority: symbol_hash > file_path+line > file_path > module > duplicates
pub fn run_validate(args: &ValidateArgs, ctx: &CommandContext) -> Result<String> {
    // Use provided path or current directory
    let repo_dir = match &args.path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    // Scope detection (in order of priority):
    // 1. symbol_hash → single symbol validation
    if let Some(ref hash) = args.symbol_hash {
        return run_validate_symbol_by_hash(args, &cache, hash, ctx);
    }

    // 2. file_path + line → single symbol at location
    // 3. file_path only → file-level validation
    if let Some(ref file_path) = args.file_path {
        if let Some(line) = args.line {
            return run_validate_symbol_by_location(args, &cache, file_path, line, ctx);
        }
        return run_validate_file(args, &cache, file_path, ctx);
    }

    // 4. module → module-level validation
    if let Some(ref module_name) = args.module {
        return run_validate_module(args, &cache, module_name, ctx);
    }

    // 5. duplicates flag or legacy target-based routing
    if args.duplicates {
        if let Some(ref target) = args.target {
            // Check if target looks like a hash (for single symbol duplicate check)
            if target.contains(':') || target.len() >= 16 {
                return run_check_duplicates(target, args.threshold, &cache, ctx);
            }
        }
    }

    // Default: run full duplicate scan with optional file/module filter
    run_find_duplicates(args, &cache, ctx)
}

/// Validate a single symbol by hash (DEDUP-304)
fn run_validate_symbol_by_hash(
    args: &ValidateArgs,
    cache: &CacheDir,
    hash: &str,
    _ctx: &CommandContext,
) -> Result<String> {
    let symbol_entry =
        find_symbol_by_hash(cache, hash).map_err(|e| McpDiffError::GitError { message: e })?;

    let result = validate_single_symbol(cache, &symbol_entry, args.threshold);
    let output = format_validation_result(&result);
    // Note: include_source not yet implemented in CLI (source snippets require additional handling)
    Ok(output)
}

/// Validate a single symbol by file+line location (DEDUP-304)
fn run_validate_symbol_by_location(
    args: &ValidateArgs,
    cache: &CacheDir,
    file_path: &str,
    line: usize,
    _ctx: &CommandContext,
) -> Result<String> {
    let symbol_entry = find_symbol_by_location(cache, file_path, line)
        .map_err(|e| McpDiffError::GitError { message: e })?;

    let result = validate_single_symbol(cache, &symbol_entry, args.threshold);
    let output = format_validation_result(&result);
    // Note: include_source not yet implemented in CLI (source snippets require additional handling)
    Ok(output)
}

/// Validate all symbols in a file (DEDUP-304)
fn run_validate_file(
    args: &ValidateArgs,
    cache: &CacheDir,
    file_path: &str,
    _ctx: &CommandContext,
) -> Result<String> {
    let all_entries = cache
        .load_all_symbol_entries()
        .map_err(|e| McpDiffError::GitError {
            message: format!("Failed to load symbol index: {}", e),
        })?;

    let mut entries: Vec<_> = all_entries
        .into_iter()
        .filter(|e| e.file.ends_with(file_path) || file_path.ends_with(&e.file))
        .collect();

    if let Some(ref kind) = args.kind {
        let normalized = normalize_kind(kind);
        entries.retain(|e| e.kind.eq_ignore_ascii_case(normalized));
    }
    let symbol_scope = args.symbol_scope.for_kind(args.kind.as_deref());
    entries.retain(|e| symbol_scope.matches_kind(&e.kind));

    if entries.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No symbols found in file: {}", file_path),
        });
    }

    let results = validate_symbols_batch(cache, &entries, args.threshold);
    let output = format_batch_validation_results(&results, file_path);

    Ok(output)
}

/// Validate all symbols in a module (DEDUP-304)
fn run_validate_module(
    args: &ValidateArgs,
    cache: &CacheDir,
    module_name: &str,
    _ctx: &CommandContext,
) -> Result<String> {
    let all_entries = cache
        .load_all_symbol_entries()
        .map_err(|e| McpDiffError::GitError {
            message: format!("Failed to load symbol index: {}", e),
        })?;

    let mut entries: Vec<_> = all_entries
        .into_iter()
        .filter(|e| e.module.eq_ignore_ascii_case(module_name) || e.module.ends_with(module_name))
        .collect();

    if let Some(ref kind) = args.kind {
        let normalized = normalize_kind(kind);
        entries.retain(|e| e.kind.eq_ignore_ascii_case(normalized));
    }
    let symbol_scope = args.symbol_scope.for_kind(args.kind.as_deref());
    entries.retain(|e| symbol_scope.matches_kind(&e.kind));

    entries.truncate(args.limit.min(500));

    if entries.is_empty() {
        return Err(McpDiffError::FileNotFound {
            path: format!("No symbols found in module: {}", module_name),
        });
    }

    let results = validate_symbols_batch(cache, &entries, args.threshold);
    let output = format_batch_validation_results(&results, &format!("module:{}", module_name));

    Ok(output)
}

// load_signatures removed - now uses crate::cache::load_function_signatures (DEDUP-105)

/// Wrapper to load signatures with crate error type
fn load_signatures(cache: &CacheDir) -> Result<Vec<FunctionSignature>> {
    load_function_signatures(cache).map_err(|e| McpDiffError::FileNotFound { path: e })
}

/// Extract a short module name from a file path (fallback for old cached data)
/// e.g., "/home/user/project/src/Presentation/Nop.Web/Factories/ProductModelFactory.cs"
///    -> "Nop.Web.Factories"
fn extract_module_name_from_path(file_path: &str) -> String {
    let path = Path::new(file_path);

    // Get parent directory and file stem
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy());
    let grandparent = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy());

    match (grandparent, parent) {
        (Some(gp), Some(p)) => format!("{}.{}", gp, p),
        (None, Some(p)) => p.to_string(),
        _ => path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// Get module name from SymbolRef, falling back to path extraction for old cached data
fn get_module_name(symbol: &crate::duplicate::SymbolRef) -> String {
    if !symbol.module.is_empty() {
        symbol.module.clone()
    } else {
        extract_module_name_from_path(&symbol.file)
    }
}

/// Find all duplicates in the codebase
fn run_find_duplicates(
    args: &ValidateArgs,
    cache: &CacheDir,
    ctx: &CommandContext,
) -> Result<String> {
    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    eprintln!("Loading function signatures...");
    let mut signatures = load_signatures(cache)?;

    // Filter by target if specified (file path or module name)
    if let Some(ref target) = args.target {
        // Normalize path separators for cross-platform compatibility
        let target_normalized = target.replace('/', std::path::MAIN_SEPARATOR_STR);
        let target_lower = target_normalized.to_lowercase();
        signatures.retain(|sig| sig.file.to_lowercase().contains(&target_lower));

        if signatures.is_empty() {
            return Err(McpDiffError::FileNotFound {
                path: format!("No symbols found matching: {}", target),
            });
        }
    }

    // Filter by minimum lines only for duplicate detection (DEDUP-207)
    if args.duplicates {
        signatures.retain(|sig| sig.line_count >= args.min_lines);
    }

    if signatures.is_empty() {
        // Respect output format for empty results
        return match ctx.format {
            OutputFormat::Json => Ok(serde_json::json!({
                "_type": "duplicate_analysis",
                "clusters": 0,
                "message": "No function signatures found in index."
            })
            .to_string()),
            OutputFormat::Toon | OutputFormat::Text => {
                Ok("No function signatures found in index.".to_string())
            }
        };
    }

    eprintln!(
        "Analyzing {} signatures for duplicates...",
        signatures.len()
    );

    let exclude_boilerplate = !args.include_boilerplate;
    let detector =
        DuplicateDetector::new(args.threshold).with_boilerplate_exclusion(exclude_boilerplate);

    let mut clusters = detector.find_all_clusters(&signatures);
    let total_clusters = clusters.len();

    // Sort clusters by specified criteria (DEDUP-207)
    match args.sort_by.as_str() {
        "size" => {
            // Sort by primary function size (lines), largest first
            clusters.sort_by(|a, b| {
                let a_size = a.primary.end_line.saturating_sub(a.primary.start_line);
                let b_size = b.primary.end_line.saturating_sub(b.primary.start_line);
                b_size.cmp(&a_size)
            });
        }
        "count" => {
            // Sort by number of duplicates, most first
            clusters.sort_by(|a, b| b.duplicates.len().cmp(&a.duplicates.len()));
        }
        _ => {
            // Default: sort by highest similarity in cluster
            clusters.sort_by(|a, b| {
                let a_max = a
                    .duplicates
                    .iter()
                    .map(|d| d.similarity)
                    .fold(0.0_f64, f64::max);
                let b_max = b
                    .duplicates
                    .iter()
                    .map(|d| d.similarity)
                    .fold(0.0_f64, f64::max);
                b_max
                    .partial_cmp(&a_max)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    // Apply pagination with offset and limit (DEDUP-207)
    let limit = args.limit.min(200);
    let paginated: Vec<_> = clusters.into_iter().skip(args.offset).take(limit).collect();

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            // JSON keeps full paths for programmatic access (DEDUP-207 enhanced)
            let json_value = serde_json::json!({
                "_type": "duplicate_analysis",
                "threshold": args.threshold,
                "boilerplate_excluded": exclude_boilerplate,
                "min_lines": args.min_lines,
                "sort_by": args.sort_by,
                "total_signatures": signatures.len(),
                "filter": args.target,
                "clusters": total_clusters,
                "offset": args.offset,
                "limit": limit,
                "showing": paginated.len(),
                "total_duplicates": paginated.iter().map(|c| c.duplicates.len()).sum::<usize>(),
                "cluster_details": paginated.iter().map(|c| serde_json::json!({
                    "primary": c.primary.name,
                    "primary_file": c.primary.file,
                    "primary_hash": c.primary.hash,
                    "duplicate_count": c.duplicates.len(),
                    "duplicates": c.duplicates.iter().take(5).map(|d| serde_json::json!({
                        "name": d.symbol.name,
                        "file": d.symbol.file,
                        "similarity": d.similarity,
                        "kind": format!("{:?}", d.kind)
                    })).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            });
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            // Token-optimized format - groups by module (DEDUP-207 enhanced)
            output.push_str(&super::toon_header("duplicate_results"));
            let showing_start = args.offset + 1;
            let showing_end = args.offset + paginated.len();
            output.push_str(&format!(
                "threshold: {:.0}% | clusters: {} | showing: {}-{} | sort: {} | min_lines: {}\n",
                args.threshold * 100.0,
                total_clusters,
                showing_start,
                showing_end,
                args.sort_by,
                args.min_lines
            ));

            if showing_end < total_clusters {
                output.push_str(&format!(
                    "hint: use --offset {} for next page\n",
                    showing_end
                ));
            }

            if paginated.is_empty() {
                output.push_str("result: No duplicate clusters found above threshold.\n");
                return Ok(output);
            }

            let page_duplicates: usize = paginated.iter().map(|c| c.duplicates.len()).sum();
            output.push_str(&format!("page_duplicates: {}\n\n", page_duplicates));

            for (i, cluster) in paginated.iter().enumerate() {
                let primary_module = get_module_name(&cluster.primary);
                let line_info = if cluster.primary.start_line > 0 {
                    format!(":{}", cluster.primary.start_line)
                } else {
                    String::new()
                };

                // Compact cluster header
                output.push_str(&format!(
                    "[{}] {} ({}{})\n",
                    i + 1,
                    cluster.primary.name,
                    primary_module,
                    line_info
                ));
                output.push_str(&format!("  hash: {}\n", cluster.primary.hash));
                output.push_str(&format!("  duplicates: {}\n", cluster.duplicates.len()));

                // Group duplicates by module (actual index module names)
                let mut by_module: BTreeMap<String, Vec<&crate::duplicate::DuplicateMatch>> =
                    BTreeMap::new();
                for dup in &cluster.duplicates {
                    let module = get_module_name(&dup.symbol);
                    by_module.entry(module).or_default().push(dup);
                }

                // Show modules with counts and similarity ranges (limit to top 5)
                output.push_str("  by_module:\n");
                let mut module_entries: Vec<_> = by_module.iter().collect();
                module_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

                let max_modules_shown = 5;
                let mut remaining_count = 0;
                let mut remaining_modules = 0;

                for (idx, (module, dups)) in module_entries.iter().enumerate() {
                    if idx >= max_modules_shown {
                        remaining_count += dups.len();
                        remaining_modules += 1;
                        continue;
                    }

                    let min_sim = dups
                        .iter()
                        .map(|d| d.similarity)
                        .fold(f64::INFINITY, f64::min);
                    let max_sim = dups.iter().map(|d| d.similarity).fold(0.0_f64, f64::max);

                    let exact = dups
                        .iter()
                        .filter(|d| matches!(d.kind, DuplicateKind::Exact))
                        .count();
                    let near = dups
                        .iter()
                        .filter(|d| matches!(d.kind, DuplicateKind::Near))
                        .count();

                    let kind_hint = if exact > 0 && near == 0 {
                        "exact"
                    } else if exact == 0 && near > 0 {
                        "near"
                    } else if exact > 0 {
                        "mixed"
                    } else {
                        "divergent"
                    };

                    output.push_str(&format!(
                        "    {}: {} ({:.0}-{:.0}%) [{}]\n",
                        module,
                        dups.len(),
                        min_sim * 100.0,
                        max_sim * 100.0,
                        kind_hint
                    ));
                }

                if remaining_modules > 0 {
                    output.push_str(&format!(
                        "    +{} more modules: {} dups\n",
                        remaining_modules, remaining_count
                    ));
                }

                // Show top 3 individual matches
                let mut top_matches: Vec<_> = cluster.duplicates.iter().collect();
                top_matches.sort_by(|a, b| {
                    b.similarity
                        .partial_cmp(&a.similarity)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                if !top_matches.is_empty() {
                    output.push_str("  top_matches:\n");
                    for dup in top_matches.iter().take(3) {
                        let dup_module = get_module_name(&dup.symbol);
                        output.push_str(&format!(
                            "    {}@{} {:.0}%\n",
                            dup.symbol.name,
                            dup_module,
                            dup.similarity * 100.0
                        ));
                    }
                }

                output.push_str("\n");
            }
        }
        OutputFormat::Text => {
            // Human-readable format for terminal (DEDUP-207 enhanced)
            output.push_str("═══════════════════════════════════════════\n");
            if let Some(ref target) = args.target {
                output.push_str(&format!("  DUPLICATE ANALYSIS: {}\n", target));
            } else {
                output.push_str("  DUPLICATE ANALYSIS\n");
            }
            output.push_str("═══════════════════════════════════════════\n\n");

            output.push_str(&format!("Threshold: {:.0}%\n", args.threshold * 100.0));
            output.push_str(&format!("Boilerplate excluded: {}\n", exclude_boilerplate));
            output.push_str(&format!("Min lines: {}\n", args.min_lines));
            output.push_str(&format!("Sort by: {}\n", args.sort_by));
            output.push_str(&format!("Signatures analyzed: {}\n", signatures.len()));
            output.push_str(&format!("Clusters found: {}\n", total_clusters));
            let showing_start = args.offset + 1;
            let showing_end = args.offset + paginated.len();
            output.push_str(&format!("Showing: {}-{}\n\n", showing_start, showing_end));

            if paginated.is_empty() {
                output.push_str("No duplicate clusters found above threshold.\n");
                return Ok(output);
            }

            let total_duplicates: usize = paginated.iter().map(|c| c.duplicates.len()).sum();
            output.push_str(&format!(
                "Total duplicate functions: {}\n\n",
                total_duplicates
            ));

            for (i, cluster) in paginated.iter().enumerate() {
                let primary_module = get_module_name(&cluster.primary);

                output.push_str("───────────────────────────────────────────\n");
                output.push_str(&format!(
                    "Cluster {} ({} duplicates)\n",
                    i + 1,
                    cluster.duplicates.len()
                ));
                output.push_str(&format!(
                    "Primary: {} ({})\n",
                    cluster.primary.name, primary_module
                ));
                output.push_str(&format!("  hash: {}\n", cluster.primary.hash));

                // Group by module (actual index module names)
                let mut by_module: BTreeMap<String, Vec<&crate::duplicate::DuplicateMatch>> =
                    BTreeMap::new();
                for dup in &cluster.duplicates {
                    let module = get_module_name(&dup.symbol);
                    by_module.entry(module).or_default().push(dup);
                }

                output.push_str("Duplicates by module:\n");
                let mut module_entries: Vec<_> = by_module.iter().collect();
                module_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

                for (module, dups) in module_entries.iter().take(5) {
                    let min_sim = dups
                        .iter()
                        .map(|d| d.similarity)
                        .fold(f64::INFINITY, f64::min);
                    let max_sim = dups.iter().map(|d| d.similarity).fold(0.0_f64, f64::max);
                    output.push_str(&format!(
                        "  {}: {} functions ({:.0}-{:.0}%)\n",
                        module,
                        dups.len(),
                        min_sim * 100.0,
                        max_sim * 100.0
                    ));
                }

                let shown_modules = module_entries.len().min(5);
                if module_entries.len() > shown_modules {
                    let remaining: usize = module_entries
                        .iter()
                        .skip(shown_modules)
                        .map(|(_, d)| d.len())
                        .sum();
                    output.push_str(&format!(
                        "  +{} more modules ({} duplicates)\n",
                        module_entries.len() - shown_modules,
                        remaining
                    ));
                }
                output.push('\n');
            }

            if args.offset + paginated.len() < total_clusters {
                output.push_str(&format!(
                    "\n... showing {} of {} clusters (use --offset {} for next page)\n",
                    paginated.len(),
                    total_clusters,
                    args.offset + paginated.len()
                ));
            }
        }
    }

    Ok(output)
}

/// Check duplicates for a specific symbol by hash
fn run_check_duplicates(
    hash: &str,
    threshold: f64,
    cache: &CacheDir,
    ctx: &CommandContext,
) -> Result<String> {
    // Load all signatures
    let signatures = load_signatures(cache)?;

    // Find the signature with matching hash
    let target_sig = signatures
        .iter()
        .find(|s| s.symbol_hash == hash || s.symbol_hash.starts_with(hash))
        .ok_or_else(|| McpDiffError::FileNotFound {
            path: format!("Symbol not found: {}", hash),
        })?;

    // Find duplicates for this specific symbol using DuplicateDetector
    let detector = DuplicateDetector::new(threshold);
    let mut duplicates = detector.find_duplicates(target_sig, &signatures);

    // Sort by similarity descending
    duplicates.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "check_duplicates",
        "symbol": target_sig.name,
        "hash": target_sig.symbol_hash,
        "file": target_sig.file,
        "threshold": threshold,
        "duplicates": duplicates.iter().map(|dup| serde_json::json!({
            "name": dup.symbol.name,
            "hash": dup.symbol.hash,
            "file": dup.symbol.file,
            "similarity": dup.similarity,
            "kind": format!("{:?}", dup.kind)
        })).collect::<Vec<_>>(),
        "count": duplicates.len()
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("symbol: {}\n", target_sig.name));
            output.push_str(&format!("hash: {}\n", target_sig.symbol_hash));
            output.push_str(&format!("file: {}\n", target_sig.file));
            output.push_str(&format!("threshold: {:.0}%\n\n", threshold * 100.0));
            output.push_str(&format!("duplicates[{}]:\n", duplicates.len()));

            if duplicates.is_empty() {
                output.push_str("  (no duplicates found)\n");
            } else {
                for dup in &duplicates {
                    output.push_str(&format!(
                        "  - {} ({:.0}%)\n    {}\n",
                        dup.symbol.name,
                        dup.similarity * 100.0,
                        dup.symbol.file
                    ));
                }
            }
        }
    }

    Ok(output)
}

// ============================================================================
// DEDUP-307: Public interface for MCP find_duplicates delegation
// ============================================================================

/// Find duplicates - unified CLI/MCP handler (DEDUP-307)
///
/// Two modes:
/// 1. Single symbol mode (symbol_hash provided): Find duplicates of a specific symbol
/// 2. Codebase scan mode (default): Find all duplicate clusters
#[allow(clippy::too_many_arguments)]
pub fn run_duplicates(
    path: Option<&std::path::PathBuf>,
    symbol_hash: Option<&str>,
    threshold: f64,
    module_filter: Option<&str>,
    exclude_boilerplate: bool,
    min_lines: usize,
    sort_by: &str,
    limit: usize,
    offset: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    // Single symbol mode: check specific symbol for duplicates
    if let Some(hash) = symbol_hash {
        return run_check_duplicates(hash, threshold, &cache, ctx);
    }

    // Codebase scan mode: build ValidateArgs and delegate to run_find_duplicates
    let args = ValidateArgs {
        path: Some(repo_dir),
        target: module_filter.map(String::from),
        threshold,
        duplicates: true,
        include_boilerplate: !exclude_boilerplate,
        min_lines,
        limit,
        offset,
        sort_by: sort_by.to_string(),
        // Not used for duplicates
        symbol_hash: None,
        file_path: None,
        line: None,
        module: None,
        include_source: false,
        kind: None,
        symbol_scope: SymbolScope::Functions,
    };

    run_find_duplicates(&args, &cache, ctx)
}
