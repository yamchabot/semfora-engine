//! Engine events for push notifications to CLI (SEM-98)
//!
//! This module defines JSON events that the engine server emits to stdout
//! when running in persistent mode. The CLI reads these events and forwards
//! them to the frontend via Tauri's event system.
//!
//! # Event Format
//!
//! All events are JSON objects on a single line (JSON Lines format):
//! ```json
//! {"type":"layer_updated","layer":"Working","symbols_added":5,...}
//! ```
//!
//! # Event Types
//!
//! - `layer_updated` - A layer's index was updated
//! - `layer_stale` - A layer became stale and needs update
//! - `server_status` - Server status changed (started, stopped)
//! - `indexing_progress` - Progress during full rebuild

use serde::Serialize;
use std::io::{self, Write};

use super::sync::LayerUpdateStats;
use crate::overlay::LayerKind;

/// Event emitter for sending JSON events to stdout
pub struct EventEmitter {
    enabled: bool,
}

impl EventEmitter {
    /// Create a new event emitter
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Emit an event to stdout as JSON
    pub fn emit<E: EngineEvent>(&self, event: &E) {
        if !self.enabled {
            return;
        }

        let wrapper = EventWrapper {
            event_type: E::event_type(),
            payload: event,
        };

        // Write JSON to stdout followed by newline
        if let Ok(json) = serde_json::to_string(&wrapper) {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            // Ignore write errors (CLI may have closed)
            let _ = writeln!(handle, "{}", json);
            let _ = handle.flush();
        }
    }
}

/// Wrapper for events with type field
#[derive(Serialize)]
struct EventWrapper<'a, P: Serialize> {
    #[serde(rename = "type")]
    event_type: &'static str,
    #[serde(flatten)]
    payload: &'a P,
}

/// Trait for engine events
pub trait EngineEvent: Serialize {
    fn event_type() -> &'static str;
}

// ============================================================================
// Event Types
// ============================================================================

/// Event emitted when a layer is updated
#[derive(Debug, Clone, Serialize)]
pub struct LayerUpdatedEvent {
    /// Which layer was updated
    pub layer: String,
    /// Update strategy used
    pub strategy: String,
    /// Number of files processed
    pub files_processed: usize,
    /// Number of symbols added
    pub symbols_added: usize,
    /// Number of symbols removed
    pub symbols_removed: usize,
    /// Number of symbols modified
    pub symbols_modified: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
}

impl EngineEvent for LayerUpdatedEvent {
    fn event_type() -> &'static str {
        "layer_updated"
    }
}

impl LayerUpdatedEvent {
    /// Create from layer kind and update stats
    pub fn from_stats(layer: LayerKind, stats: &LayerUpdateStats) -> Self {
        Self {
            layer: format!("{:?}", layer),
            strategy: stats.strategy.clone(),
            files_processed: stats.files_processed,
            symbols_added: stats.symbols_added,
            symbols_removed: stats.symbols_removed,
            symbols_modified: stats.symbols_modified,
            duration_ms: stats.duration_ms,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Event emitted when a layer becomes stale
#[derive(Debug, Clone, Serialize)]
pub struct LayerStaleEvent {
    /// Which layer is stale
    pub layer: String,
    /// Recommended update strategy
    pub strategy: String,
    /// Number of changed files (if known)
    pub changed_files: Option<usize>,
    /// Timestamp
    pub timestamp: String,
}

impl EngineEvent for LayerStaleEvent {
    fn event_type() -> &'static str {
        "layer_stale"
    }
}

/// Event emitted when server status changes
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusEvent {
    /// Server status (started, stopped, error)
    pub status: String,
    /// Repository path
    pub repo_path: String,
    /// Whether persistent mode is enabled
    pub persistent_mode: bool,
    /// Optional message
    pub message: Option<String>,
    /// Timestamp
    pub timestamp: String,
}

impl EngineEvent for ServerStatusEvent {
    fn event_type() -> &'static str {
        "server_status"
    }
}

impl ServerStatusEvent {
    pub fn started(repo_path: &str) -> Self {
        Self {
            status: "started".to_string(),
            repo_path: repo_path.to_string(),
            persistent_mode: true,
            message: Some("Engine server started with live layer updates".to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn stopped(repo_path: &str) -> Self {
        Self {
            status: "stopped".to_string(),
            repo_path: repo_path.to_string(),
            persistent_mode: true,
            message: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Event emitted during indexing progress
#[derive(Debug, Clone, Serialize)]
pub struct IndexingProgressEvent {
    /// Which layer is being indexed
    pub layer: String,
    /// Number of files processed so far
    pub files_processed: usize,
    /// Total files to process (if known)
    pub total_files: Option<usize>,
    /// Percentage complete (0-100)
    pub percent: Option<u8>,
    /// Current file being processed
    pub current_file: Option<String>,
    /// Timestamp
    pub timestamp: String,
}

impl EngineEvent for IndexingProgressEvent {
    fn event_type() -> &'static str {
        "indexing_progress"
    }
}

/// Event emitted when potential duplicate functions are detected
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateDetectedEvent {
    /// The new/modified function that triggered detection
    pub new_function: DuplicateFunctionInfo,
    /// Similar functions found in the codebase
    pub similar_to: Vec<DuplicateSimilarInfo>,
    /// Timestamp
    pub timestamp: String,
}

/// Information about a function in a duplicate detection event
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateFunctionInfo {
    /// Function name
    pub name: String,
    /// File path
    pub file: String,
    /// Line range (e.g., "10-25")
    pub lines: String,
    /// Symbol hash for lookup
    pub hash: String,
}

/// Information about a similar function
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateSimilarInfo {
    /// Function name
    pub name: String,
    /// File path
    pub file: String,
    /// Line range
    pub lines: String,
    /// Symbol hash
    pub hash: String,
    /// Similarity percentage (0-100)
    pub similarity: u8,
    /// Kind: "exact", "near", or "divergent"
    pub kind: String,
    /// Brief differences description
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
}

impl EngineEvent for DuplicateDetectedEvent {
    fn event_type() -> &'static str {
        "duplicate_detected"
    }
}

impl DuplicateDetectedEvent {
    /// Create a new duplicate detected event
    pub fn new(new_function: DuplicateFunctionInfo, similar_to: Vec<DuplicateSimilarInfo>) -> Self {
        Self {
            new_function,
            similar_to,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// ============================================================================
// Global Event Emitter
// ============================================================================

use parking_lot::Mutex;
use std::sync::mpsc;
use std::sync::OnceLock;

static GLOBAL_EMITTER: OnceLock<EventEmitter> = OnceLock::new();

/// Initialize the global event emitter
pub fn init_event_emitter(enabled: bool) {
    let _ = GLOBAL_EMITTER.set(EventEmitter::new(enabled));
}

/// Emit an event using the global emitter
pub fn emit_event<E: EngineEvent>(event: &E) {
    if let Some(emitter) = GLOBAL_EMITTER.get() {
        emitter.emit(event);
    }
    // Also send to any registered broadcast listeners
    broadcast_event(event);
}

// ============================================================================
// Global Broadcast Channel for Socket Server
// ============================================================================

/// Serialized event for broadcasting
#[derive(Debug, Clone)]
pub struct BroadcastEvent {
    pub event_type: String,
    pub payload_json: String,
}

type EventSender = mpsc::Sender<BroadcastEvent>;

static BROADCAST_SENDERS: OnceLock<Mutex<Vec<EventSender>>> = OnceLock::new();

fn get_broadcast_senders() -> &'static Mutex<Vec<EventSender>> {
    BROADCAST_SENDERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a sender to receive broadcast events
/// Returns the sender that should be passed to the socket server
pub fn register_broadcast_listener() -> mpsc::Receiver<BroadcastEvent> {
    let (tx, rx) = mpsc::channel();
    get_broadcast_senders().lock().push(tx);
    rx
}

/// Broadcast an event to all registered listeners
fn broadcast_event<E: EngineEvent>(event: &E) {
    let senders = get_broadcast_senders().lock();
    tracing::debug!(
        "[BROADCAST] broadcast_event called, {} listeners registered",
        senders.len()
    );

    if senders.is_empty() {
        tracing::debug!("[BROADCAST] No listeners, skipping broadcast");
        return;
    }

    let broadcast = BroadcastEvent {
        event_type: E::event_type().to_string(),
        payload_json: serde_json::to_string(event).unwrap_or_default(),
    };

    tracing::info!(
        "[BROADCAST] Sending {} event to {} listeners",
        broadcast.event_type,
        senders.len()
    );

    // Send to all listeners (ignore errors for disconnected receivers)
    let mut sent_count = 0;
    for sender in senders.iter() {
        match sender.send(broadcast.clone()) {
            Ok(_) => sent_count += 1,
            Err(e) => tracing::warn!("[BROADCAST] Failed to send to listener: {:?}", e),
        }
    }
    tracing::debug!(
        "[BROADCAST] Sent to {}/{} listeners",
        sent_count,
        senders.len()
    );
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_updated_event_serialization() {
        let event = LayerUpdatedEvent {
            layer: "Working".to_string(),
            strategy: "Incremental (3 files)".to_string(),
            files_processed: 3,
            symbols_added: 5,
            symbols_removed: 2,
            symbols_modified: 1,
            duration_ms: 150,
            timestamp: "2024-01-15T10:30:00Z".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"layer\":\"Working\""));
        assert!(json.contains("\"symbols_added\":5"));
    }

    #[test]
    fn test_server_status_event() {
        let event = ServerStatusEvent::started("/path/to/repo");
        assert_eq!(event.status, "started");
        assert!(event.persistent_mode);
    }
}
