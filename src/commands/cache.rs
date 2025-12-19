//! Cache command handler - Manage the semantic cache

use crate::cache::{get_cache_base_dir, list_cached_repos, prune_old_caches, CacheDir};
use crate::cli::{CacheArgs, CacheOperation, OutputFormat};
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};

/// Run the cache command
pub fn run_cache(args: &CacheArgs, ctx: &CommandContext) -> Result<String> {
    match &args.operation {
        CacheOperation::Info => run_cache_info(ctx),
        CacheOperation::Clear => run_cache_clear(ctx),
        CacheOperation::Prune { days } => run_cache_prune(*days, ctx),
    }
}

/// Show cache information
fn run_cache_info(ctx: &CommandContext) -> Result<String> {
    let base_dir = get_cache_base_dir();
    let cached_repos = list_cached_repos();

    let mut output = String::new();

    let total_size: u64 = cached_repos.iter().map(|(_, _, s)| *s).sum();
    let repos: Vec<serde_json::Value> = cached_repos
        .iter()
        .map(|(hash, path, size)| {
            serde_json::json!({
                "hash": hash,
                "path": path.to_string_lossy(),
                "size_bytes": size,
                "size_mb": *size as f64 / (1024.0 * 1024.0)
            })
        })
        .collect();

    let json_value = serde_json::json!({
        "_type": "cache_info",
        "cache_base": base_dir.to_string_lossy(),
        "cached_repos": cached_repos.len(),
        "total_size_bytes": total_size,
        "total_size_mb": total_size as f64 / (1024.0 * 1024.0),
        "repos": repos
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════════════════\n");
            output.push_str("  SEMFORA CACHE INFO\n");
            output.push_str("═══════════════════════════════════════════════════════\n\n");

            output.push_str(&format!("cache_base: {}\n", base_dir.display()));
            output.push_str(&format!("cached_repos: {}\n\n", cached_repos.len()));

            if cached_repos.is_empty() {
                output.push_str("No cached repositories found.\n");
            } else {
                output.push_str(&format!(
                    "total_size: {} bytes ({:.2} MB)\n\n",
                    total_size,
                    total_size as f64 / (1024.0 * 1024.0)
                ));

                output.push_str("repos:\n");
                for (hash, path, size) in &cached_repos {
                    output.push_str(&format!("  - hash: {}\n", hash));
                    output.push_str(&format!("    path: {}\n", path.display()));
                    output.push_str(&format!(
                        "    size: {} bytes ({:.2} MB)\n",
                        size,
                        *size as f64 / (1024.0 * 1024.0)
                    ));
                }
            }
        }
    }

    Ok(output)
}

/// Clear the cache for the current directory
fn run_cache_clear(ctx: &CommandContext) -> Result<String> {
    let current_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;

    let cache = CacheDir::for_repo(&current_dir)?;

    let mut output = String::new();

    let json_value = if cache.exists() {
        let size = cache.size();
        cache.clear()?;
        serde_json::json!({
            "_type": "cache_clear",
            "cleared": true,
            "path": current_dir.to_string_lossy(),
            "freed_bytes": size,
            "freed_mb": size as f64 / (1024.0 * 1024.0)
        })
    } else {
        serde_json::json!({
            "_type": "cache_clear",
            "cleared": false,
            "path": current_dir.to_string_lossy(),
            "message": "No cache exists for this directory"
        })
    };

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            if json_value
                .get("cleared")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let freed = json_value
                    .get("freed_bytes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                output.push_str(&format!("Cache cleared for: {}\n", current_dir.display()));
                output.push_str(&format!(
                    "Freed: {} bytes ({:.2} MB)\n",
                    freed,
                    freed as f64 / (1024.0 * 1024.0)
                ));
            } else {
                output.push_str(&format!("No cache exists for: {}\n", current_dir.display()));
            }
        }
    }

    Ok(output)
}

/// Prune caches older than specified days
fn run_cache_prune(days: u32, ctx: &CommandContext) -> Result<String> {
    let pruned_count = prune_old_caches(days)?;

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "cache_prune",
        "days": days,
        "pruned_count": pruned_count
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str(&format!("Pruning caches older than {} days...\n", days));
            if pruned_count == 0 {
                output.push_str("No caches pruned.\n");
            } else {
                output.push_str(&format!("Pruned {} cache(s).\n", pruned_count));
            }
        }
    }

    Ok(output)
}
