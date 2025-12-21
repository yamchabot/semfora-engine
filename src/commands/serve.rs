//! MCP Server command handler
//!
//! Runs the semfora-engine MCP server using stdio transport,
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

use crate::cli::ServeArgs;
use crate::error::McpDiffError;
use crate::mcp_server::McpDiffServer;
use crate::server::{init_event_emitter, FileWatcher, GitPoller, ServerState};

use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::{self, EnvFilter};

/// Run the MCP server
///
/// This creates a tokio runtime and runs the async MCP server.
/// The server uses stdio transport for communication with AI assistants.
pub fn run_serve(args: &ServeArgs) -> crate::Result<String> {
    // Create a new tokio runtime for the async MCP server
    let runtime = tokio::runtime::Runtime::new().map_err(|e| McpDiffError::ConfigError {
        message: format!("Failed to create tokio runtime: {}", e),
    })?;

    // Run the async server in the runtime
    runtime.block_on(async { run_serve_async(args).await })?;

    // Server exits cleanly - no output needed
    Ok(String::new())
}

/// Async implementation of the MCP server
async fn run_serve_async(args: &ServeArgs) -> crate::Result<()> {
    // Initialize tracing for debugging (logs to stderr)
    // Note: This may fail if already initialized, which is fine
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("semfora_engine=info".parse().unwrap())
                .add_directive("rmcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .try_init();

    // Determine repository path
    let repo_path = args
        .repo
        .clone()
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

    // Start background services for automatic layer updates (unless disabled)
    let _watcher_handle = if !args.no_watch {
        let file_watcher = FileWatcher::new(repo_path.clone());
        let handle = file_watcher.start(Arc::clone(&server_state)).map_err(|e| {
            McpDiffError::ConfigError {
                message: format!("Failed to start file watcher: {}", e),
            }
        })?;
        tracing::info!("Started FileWatcher for live index updates");
        Some(handle)
    } else {
        tracing::info!("FileWatcher disabled");
        None
    };

    let _poller_handle = if !args.no_git_poll {
        let git_poller = GitPoller::new(repo_path.clone());
        let handle =
            git_poller
                .start(Arc::clone(&server_state))
                .map_err(|e| McpDiffError::ConfigError {
                    message: format!("Failed to start git poller: {}", e),
                })?;
        tracing::info!("Started GitPoller for branch/commit updates");
        Some(handle)
    } else {
        tracing::info!("GitPoller disabled");
        None
    };

    // Create MCP server with persistent state
    let server = McpDiffServer::with_server_state(repo_path, server_state);

    let service = server
        .serve(stdio())
        .await
        .map_err(|e| McpDiffError::ConfigError {
            message: format!("Failed to start MCP server: {}", e),
        })?;

    tracing::info!("MCP server initialized, waiting for requests...");

    // Wait for shutdown
    service
        .waiting()
        .await
        .map_err(|e| McpDiffError::ConfigError {
            message: format!("MCP server error: {}", e),
        })?;

    tracing::info!("MCP server shutting down");

    Ok(())
}
