//! Semfora Daemon Binary
//!
//! A WebSocket server that provides multi-client, multi-repo support
//! for semantic code analysis with event subscriptions.
//!
//! # Usage
//!
//! ```bash
//! semfora-daemon --port 9847
//! semfora-daemon --port 9847 --host 127.0.0.1
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use semfora_engine::server::events::register_broadcast_listener;
use semfora_engine::socket_server::{handle_connection, RepoRegistry};

/// Semfora Socket Server Daemon
#[derive(Parser, Debug)]
#[command(name = "semfora-daemon")]
#[command(about = "Semfora semantic code analysis daemon")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "9847")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("semfora_engine=info".parse().unwrap())
                .add_directive("semfora_daemon=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;

    // Create the repo registry
    let registry = Arc::new(RepoRegistry::new());

    // Register to receive file watcher events (from std::sync::mpsc)
    let event_rx = register_broadcast_listener();

    // Create a tokio broadcast channel for distributing events to connections
    let (event_broadcast_tx, _) = broadcast::channel::<String>(100);
    let event_broadcast_tx_for_task = event_broadcast_tx.clone();

    // Store the broadcast sender in the registry for connections to subscribe
    registry.set_event_broadcaster(event_broadcast_tx.clone());

    // Spawn a task to bridge std::sync::mpsc -> tokio broadcast
    // Uses leading-edge throttle: broadcast immediately, then throttle for 500ms
    tokio::spawn(async move {
        let throttle_duration = std::time::Duration::from_millis(500);
        let mut last_broadcast_time: Option<std::time::Instant> = None;

        loop {
            // Check for events from file watcher (non-blocking)
            match event_rx.try_recv() {
                Ok(event) => {
                    // Check if we're in throttle period
                    let should_broadcast = match last_broadcast_time {
                        None => true, // First event ever
                        Some(last_time) => last_time.elapsed() >= throttle_duration,
                    };

                    if should_broadcast {
                        let json = serde_json::json!({
                            "type": "event",
                            "name": format!("layer_updated:{}", event.event_type),
                            "payload": serde_json::from_str::<serde_json::Value>(&event.payload_json)
                                .unwrap_or(serde_json::Value::Null)
                        });
                        tracing::info!("Broadcasting event: {}", event.event_type);
                        let _ = event_broadcast_tx_for_task.send(json.to_string());
                        last_broadcast_time = Some(std::time::Instant::now());
                    } else {
                        tracing::debug!("Throttling event (within 500ms window)");
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No new events, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!("Event broadcast channel disconnected");
                    break;
                }
            }
        }
    });

    // Start the TCP listener
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Semfora daemon listening on ws://{}", addr);
    tracing::info!("Connect with a WebSocket client to start");

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("Accepted connection from {}", addr);
                let registry = Arc::clone(&registry);
                tokio::spawn(async move {
                    handle_connection(stream, registry).await;
                });
            }
            Err(e) => {
                tracing::error!("Failed to accept connection: {}", e);
            }
        }
    }
}
