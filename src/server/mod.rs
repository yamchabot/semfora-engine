//! Persistent Semantic Index Server (SEM-98)
//!
//! This module implements the persistent server infrastructure for maintaining
//! a live semantic index that updates automatically as files change.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         ServerState                                  │
//! │  ┌─────────────────────────────────────────────────────────────┐    │
//! │  │              Arc<RwLock<LayeredIndex>>                       │    │
//! │  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐            │    │
//! │  │  │  Base   │ │ Branch  │ │ Working │ │   AI    │            │    │
//! │  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘            │    │
//! │  └─────────────────────────────────────────────────────────────┘    │
//! │                                                                      │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               │
//! │  │ FileWatcher  │  │  GitPoller   │  │  LayerSync   │               │
//! │  │  (SEM-101)   │  │  (SEM-102)   │  │  (SEM-104)   │               │
//! │  └──────────────┘  └──────────────┘  └──────────────┘               │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Locking Order (SEM-99)
//!
//! To prevent deadlocks, always acquire locks in this order:
//! 1. ServerState::index (RwLock)
//! 2. ServerState::cache_dir (Mutex, if needed)
//! 3. ServerState::status (Mutex)
//!
//! Never hold a lock while performing I/O operations. Instead:
//! - Read data under lock
//! - Release lock
//! - Perform I/O
//! - Re-acquire lock to update state
//!
//! # Modules
//!
//! - `state` - Thread-safe state management (SEM-99)
//! - `watcher` - File system watching (SEM-101)
//! - `git_poller` - Git state polling (SEM-102)
//! - `sync` - Layer synchronization (SEM-104)

pub mod ast_cache;
pub mod events;
pub mod git_poller;
pub mod state;
pub mod sync;
pub mod watcher;

pub use ast_cache::{AstCache, AstCacheStats, ParseResult};
pub use events::{
    emit_event, init_event_emitter, IndexingProgressEvent, LayerStaleEvent, LayerUpdatedEvent,
    ServerStatusEvent,
};
pub use git_poller::GitPoller;
pub use state::{LayerStatus, ServerState, ServerStatus};
pub use sync::{LayerSynchronizer, LayerUpdateStats};
pub use watcher::FileWatcher;
