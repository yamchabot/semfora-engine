//! Semfora Socket Server
//!
//! A standalone daemon that provides multi-client, multi-repo support for
//! semantic code analysis with event subscriptions.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    SEMFORA SOCKET SERVER (semfora-daemon)               │
//! │                     Single daemon, multi-repo, multi-client             │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  ┌─────────────────────────────────────────────────────────────────┐   │
//! │  │                    RepoRegistry (Arc<RwLock>)                    │   │
//! │  │                                                                  │   │
//! │  │  repo_hash_1 ──► RepoContext {                                  │   │
//! │  │                    base_repo_path, base_branch, feature_branch, │   │
//! │  │                    worktrees, indexes, event_tx, client_count   │   │
//! │  │                  }                                              │   │
//! │  │                                                                  │   │
//! │  │  repo_hash_2 ──► RepoContext { ... }                           │   │
//! │  │                                                                  │   │
//! │  └─────────────────────────────────────────────────────────────────┘   │
//! │                                                                         │
//! │  WebSocket Server ──► ConnectionState per client                        │
//! │    - Message routing                                                    │
//! │    - Event subscriptions                                                │
//! │    - Query handling                                                     │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Protocol
//!
//! All messages are JSON over WebSocket:
//!
//! ```json
//! // Client -> Server
//! {"type": "connect", "directory": "/path/to/repo"}
//! {"type": "subscribe", "events": ["base_branch", "active_worktree"]}
//! {"type": "query", "id": 1, "method": "get_overview", "params": {}}
//!
//! // Server -> Client
//! {"type": "connected", "client_id": "...", "repo_id": "...", ...}
//! {"type": "event", "name": "base_branch:index_updated", "payload": {...}}
//! {"type": "response", "id": 1, "result": {...}}
//! ```

pub mod protocol;
pub mod worktree;
pub mod repo_registry;
pub mod connection;
pub mod indexer;

pub use protocol::{ClientMessage, ServerMessage, ConnectionInfo, WorktreeInfo, IndexInfo};
pub use repo_registry::{RepoRegistry, RepoContext};
pub use connection::handle_connection;
pub use indexer::{index_directory, needs_indexing, IndexOptions, IndexResult};
