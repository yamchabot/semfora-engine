//! Trace command handler - traverse usage across call graph.

use crate::cache::CacheDir;
use crate::cli::OutputFormat;
use crate::commands::CommandContext;
use crate::error::{McpDiffError, Result};
use crate::trace::{self, TraceOptions};

pub fn run_trace(options: TraceOptions, ctx: &CommandContext) -> Result<String> {
    let repo_dir = match options.path.clone() {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
            path: format!("current directory: {}", e),
        })?,
    };
    let cache = CacheDir::for_repo(&repo_dir)?;

    let result = trace::trace(&cache, options.clone())?;

    let json_value = serde_json::json!({
        "_type": "trace",
        "roots": result.roots,
        "nodes": result.nodes,
        "edges": result.edges,
        "stats": result.stats,
        "offset": options.offset,
        "limit": options.limit,
        "direction": match options.direction {
            trace::TraceDirection::Incoming => "incoming",
            trace::TraceDirection::Outgoing => "outgoing",
            trace::TraceDirection::Both => "both",
        },
        "include_escape_refs": options.include_escape_refs,
        "include_external": options.include_external,
    });

    let mut output = String::new();

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  TRACE\n");
            output.push_str("═══════════════════════════════════════════\n\n");
            output.push_str(&format!("roots: {}\n", json_value["roots"]));
            output.push_str(&format!("direction: {}\n", json_value["direction"]));
            output.push_str(&format!(
                "edges[{}] (offset: {}, limit: {}):\n",
                result.edges.len(),
                options.offset,
                options.limit
            ));
            for edge in &result.edges {
                output.push_str(&format!("  {} -{}-> {}\n", edge.from, edge.kind, edge.to));
            }
            output.push_str(&format!(
                "\nstats: nodes={}, edges={}, depth={}, roots_truncated={}\n",
                result.stats.total_nodes,
                result.stats.total_edges,
                result.stats.depth_reached,
                result.stats.roots_truncated
            ));
        }
    }

    Ok(output)
}
