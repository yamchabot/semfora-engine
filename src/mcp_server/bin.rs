//! MCP Server binary entry point
//!
//! This binary runs the mcp-diff MCP server using stdio transport,
//! allowing AI assistants to call the semantic analysis tools.

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::{self, EnvFilter};

// Import the MCP server from the library
use mcp_diff::mcp_server::McpDiffServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for debugging (logs to stderr)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mcp_diff=info".parse()?)
                .add_directive("rmcp=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting mcp-diff MCP server v{}", env!("CARGO_PKG_VERSION"));

    // Create the server and serve via stdio
    let server = McpDiffServer::new();
    let service = server.serve(stdio()).await?;

    tracing::info!("MCP server initialized, waiting for requests...");

    // Wait for shutdown
    service.waiting().await?;

    tracing::info!("MCP server shutting down");

    Ok(())
}
