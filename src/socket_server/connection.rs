//! WebSocket connection handler
//!
//! Manages individual client connections, message routing, and subscriptions.

use std::collections::HashSet;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

use crate::socket_server::protocol::{ClientMessage, EventFilter, ServerMessage};
use crate::socket_server::repo_registry::{RepoContext, RepoEvent, RepoRegistry};

/// Handle a single WebSocket connection
pub async fn handle_connection(
    stream: TcpStream,
    registry: Arc<RepoRegistry>,
) {
    let addr = stream.peer_addr().ok();
    tracing::info!("New connection from {:?}", addr);

    // Accept WebSocket handshake
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    // Create connection state
    let mut conn = ConnectionState::new(ws_stream, registry);
    conn.run().await;

    tracing::info!("Connection closed from {:?}", addr);
}

/// State for a single connection
struct ConnectionState {
    ws: WebSocketStream<TcpStream>,
    registry: Arc<RepoRegistry>,
    client_id: String,
    repo_context: Option<Arc<RepoContext>>,
    subscriptions: HashSet<EventFilter>,
    event_rx: Option<broadcast::Receiver<RepoEvent>>,
    /// Global event receiver (from file watchers)
    global_event_rx: Option<broadcast::Receiver<String>>,
}

impl ConnectionState {
    fn new(ws: WebSocketStream<TcpStream>, registry: Arc<RepoRegistry>) -> Self {
        let client_id = format!("cli_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
        // Subscribe to global events
        let global_event_rx = registry.subscribe_events();
        Self {
            ws,
            registry,
            client_id,
            repo_context: None,
            subscriptions: HashSet::new(),
            event_rx: None,
            global_event_rx,
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                msg = self.ws.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = self.handle_message(&text).await {
                                tracing::error!("Error handling message: {}", e);
                                let _ = self.send_error(None, "internal_error", &e.to_string()).await;
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Client {} requested close", self.client_id);
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = self.ws.send(Message::Pong(data)).await;
                        }
                        Some(Err(e)) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }

                // Handle events from repo context
                event = async {
                    if let Some(ref mut rx) = self.event_rx {
                        rx.recv().await.ok()
                    } else {
                        std::future::pending::<Option<RepoEvent>>().await
                    }
                } => {
                    if let Some(event) = event {
                        // Check if any subscription matches this event
                        if self.subscriptions.iter().any(|f| f.matches(&event.name)) {
                            let msg = ServerMessage::Event {
                                name: event.name,
                                payload: event.payload,
                            };
                            let _ = self.send(&msg).await;
                        }
                    }
                }

                // Handle global events from file watchers
                global_event = async {
                    if let Some(ref mut rx) = self.global_event_rx {
                        rx.recv().await.ok()
                    } else {
                        std::future::pending::<Option<String>>().await
                    }
                } => {
                    if let Some(event_json) = global_event {
                        // Forward global event if subscribed to any events
                        // (In future: check subscription filter against event type)
                        if !self.subscriptions.is_empty() {
                            // Event is already JSON formatted from the daemon bridge
                            let _ = self.ws.send(Message::Text(event_json)).await;
                        }
                    }
                }
            }
        }

        // Cleanup on disconnect
        self.cleanup().await;
    }

    async fn handle_message(&mut self, text: &str) -> anyhow::Result<()> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::Connect { directory } => {
                // Get or create repo context
                let ctx = self.registry.get_or_create(&directory).await?;
                ctx.add_client();

                // Subscribe to events from this repo
                self.event_rx = Some(ctx.subscribe());

                // Get connection info
                let info = ctx.connection_info(self.client_id.clone());
                self.repo_context = Some(ctx);

                // Send connected response
                self.send(&ServerMessage::Connected(info)).await?;
            }

            ClientMessage::Subscribe { events } => {
                let parsed: Vec<EventFilter> = events
                    .iter()
                    .filter_map(|s| EventFilter::parse(s))
                    .collect();

                for filter in &parsed {
                    self.subscriptions.insert(filter.clone());
                }

                let confirmed: Vec<String> = parsed.iter().map(|f| format!("{:?}", f)).collect();
                self.send(&ServerMessage::Subscribed { events: confirmed }).await?;
            }

            ClientMessage::Unsubscribe { events } => {
                let parsed: Vec<EventFilter> = events
                    .iter()
                    .filter_map(|s| EventFilter::parse(s))
                    .collect();

                for filter in &parsed {
                    self.subscriptions.remove(filter);
                }

                let confirmed: Vec<String> = parsed.iter().map(|f| format!("{:?}", f)).collect();
                self.send(&ServerMessage::Unsubscribed { events: confirmed }).await?;
            }

            ClientMessage::Query { id, method, params } => {
                let result = self.handle_query(&method, params).await;
                match result {
                    Ok(value) => {
                        self.send(&ServerMessage::Response { id, result: value }).await?;
                    }
                    Err(e) => {
                        self.send_error(Some(id), "query_error", &e.to_string()).await?;
                    }
                }
            }

            ClientMessage::Ping => {
                self.send(&ServerMessage::Pong).await?;
            }
        }

        Ok(())
    }

    async fn handle_query(&self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let ctx = self.repo_context.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to a repository"))?;

        // Extract scope parameter if provided (e.g., "base_branch", "worktree:/path/to/wt")
        let scope = params.get("scope").and_then(|v| v.as_str());

        match method {
            "list_indexes" => {
                let indexes = ctx.indexes.read();
                let list: Vec<serde_json::Value> = indexes
                    .iter()
                    .map(|(id, _state)| {
                        serde_json::json!({
                            "id": id.to_string(),
                            "scope": id.to_string(),
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "indexes": list }))
            }

            "list_worktrees" => {
                let worktrees = ctx.worktrees.read().clone();
                Ok(serde_json::json!({ "worktrees": worktrees }))
            }

            "get_overview" => {
                // Get the cache for the requested scope
                let cache = ctx.get_cache_for_scope(scope);
                let overview_path = cache.repo_overview_path();
                if overview_path.exists() {
                    let content = std::fs::read_to_string(&overview_path)?;
                    // Try to parse as JSON, otherwise return as string
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(json)
                    } else {
                        Ok(serde_json::json!({ "raw": content, "scope": scope.unwrap_or("base_branch") }))
                    }
                } else {
                    Err(anyhow::anyhow!("No index found for scope '{}'. Run semfora-mcp --shard first.", scope.unwrap_or("base_branch")))
                }
            }

            "search_symbols" => {
                let query = params.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

                let limit = params.get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(20);

                // Get the cache for the requested scope
                let cache = ctx.get_cache_for_scope(scope);
                let results = cache.search_symbols_with_fallback(
                    query,
                    None,
                    None,
                    None,
                    limit,
                )?;

                if results.fallback_used {
                    Ok(serde_json::json!({
                        "results": results.ripgrep_results,
                        "fallback": true,
                        "scope": scope.unwrap_or("base_branch")
                    }))
                } else {
                    Ok(serde_json::json!({
                        "results": results.indexed_results,
                        "scope": scope.unwrap_or("base_branch")
                    }))
                }
            }

            "get_symbol" => {
                let hash = params.get("hash")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'hash' parameter"))?;

                // Get the cache for the requested scope
                let cache = ctx.get_cache_for_scope(scope);
                let symbol_path = cache.symbol_path(hash);
                if symbol_path.exists() {
                    let content = std::fs::read_to_string(&symbol_path)?;
                    Ok(serde_json::json!({ "symbol": content, "scope": scope.unwrap_or("base_branch") }))
                } else {
                    Err(anyhow::anyhow!("Symbol not found: {} in scope '{}'", hash, scope.unwrap_or("base_branch")))
                }
            }

            "get_call_graph" => {
                // Get the cache for the requested scope
                let cache = ctx.get_cache_for_scope(scope);
                let graph = cache.load_call_graph()?;
                Ok(serde_json::json!({ "graph": graph, "scope": scope.unwrap_or("base_branch") }))
            }

            "refresh_worktrees" => {
                ctx.refresh_worktrees()?;
                let worktrees = ctx.worktrees.read().clone();
                Ok(serde_json::json!({ "worktrees": worktrees }))
            }

            "get_repo_info" => {
                Ok(serde_json::json!({
                    "repo_hash": ctx.repo_hash,
                    "base_repo_path": ctx.base_repo_path,
                    "base_branch": ctx.base_branch,
                    "feature_branch": ctx.feature_branch,
                    "client_count": ctx.client_count(),
                }))
            }

            "get_index_info" => {
                // Get fresh index info (reads from disk caches)
                let info = ctx.connection_info(self.client_id.clone());
                Ok(serde_json::json!({
                    "indexes": info.indexes,
                    "worktrees": info.worktrees,
                }))
            }

            _ => Err(anyhow::anyhow!("Unknown method: {}", method)),
        }
    }

    async fn send(&mut self, msg: &ServerMessage) -> anyhow::Result<()> {
        let json = serde_json::to_string(msg)?;
        self.ws.send(Message::Text(json)).await?;
        Ok(())
    }

    async fn send_error(&mut self, id: Option<u64>, code: &str, message: &str) -> anyhow::Result<()> {
        self.send(&ServerMessage::Error {
            id,
            code: code.to_string(),
            message: message.to_string(),
        }).await
    }

    async fn cleanup(&mut self) {
        if let Some(ctx) = self.repo_context.take() {
            let is_last = ctx.remove_client();
            if is_last {
                self.registry.maybe_evict(&ctx.repo_hash);
            }
        }
    }
}
