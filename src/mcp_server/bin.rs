//! MCP Server binary entry point
//!
//! This binary runs the semfora-engine MCP server using stdio transport,
//! allowing AI assistants to call the semantic analysis tools.
//!
//! # Live Index Updates (Default)
//!
//! The server automatically maintains a fresh index with:
//! - FileWatcher: Automatically updates Working layer on file changes
//! - GitPoller: Polls for Base/Branch layer updates (git state changes)
//! - Thread-safe access: Concurrent reads, exclusive writes
//!
//! This ensures the index stays fresh throughout long-running sessions.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::{self, EnvFilter};

// Import the MCP server and persistent server components
use semfora_engine::mcp_server::McpDiffServer;
use semfora_engine::server::{init_event_emitter, FileWatcher, GitPoller, ServerState};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for debugging (logs to stderr)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("semfora_engine=info".parse()?)
                .add_directive("rmcp=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let repo_path = args
        .iter()
        .position(|arg| arg == "--repo" || arg == "-r")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    tracing::info!(
        "Starting semfora-engine MCP server v{}",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("Repository path: {}", repo_path.display());

    // Disable event emitter for MCP mode - stdout must be pure JSON-RPC
    // Event emission to stdout breaks MCP protocol
    init_event_emitter(false);

    // Create persistent server state for live index updates
    let server_state = Arc::new(ServerState::new(repo_path.clone()));
    server_state.set_running(true);

    // Start background services for automatic layer updates
    let file_watcher = FileWatcher::new(repo_path.clone());
    let git_poller = GitPoller::new(repo_path.clone());

    // Start watchers (they run in background threads)
    let _watcher_handle = file_watcher.start(Arc::clone(&server_state))?;
    let _poller_handle = git_poller.start(Arc::clone(&server_state))?;

    tracing::info!("Started FileWatcher and GitPoller for live index updates");

    // Create MCP server with persistent state
    let server = McpDiffServer::with_server_state(repo_path, server_state);

    let service = server.serve(stdio()).await?;

    tracing::info!("MCP server initialized, waiting for requests...");

    // Wait for shutdown
    service.waiting().await?;

    tracing::info!("MCP server shutting down");

    Ok(())
}
