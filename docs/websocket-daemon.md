# Semfora WebSocket Daemon

The `semfora-daemon` is a WebSocket server that provides multi-client, multi-repo support for semantic code analysis with real-time event subscriptions.

## Quick Start

```bash
# Build the daemon
cargo build --release --bin semfora-daemon

# Start the daemon (default: localhost:9847)
./target/release/semfora-daemon

# Connect via WebSocket and send:
# {"type": "connect", "directory": "/path/to/your/project"}
```

## Overview

The daemon maintains persistent semantic indexes for connected repositories and broadcasts updates when files change. Multiple clients can connect to the same repository context, sharing indexes and receiving synchronized events.

**Key Features:**
- **Multi-repo**: Connect to multiple repositories simultaneously
- **Multi-client**: Share indexes across connected clients
- **Real-time**: File changes trigger automatic re-indexing and event broadcasts
- **Scoped queries**: Query base branch, feature branch, or specific worktrees

## Installation

```bash
cargo build --release --bin semfora-daemon
# Binary: target/release/semfora-daemon
```

## Starting the Daemon

```bash
# Default: listen on 127.0.0.1:9847
semfora-daemon

# Custom port
semfora-daemon --port 8080

# Bind to all interfaces
semfora-daemon --host 0.0.0.0 --port 9847
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    SEMFORA SOCKET SERVER (semfora-daemon)               │
│                     Single daemon, multi-repo, multi-client             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    RepoRegistry (Arc<RwLock>)                    │   │
│  │                                                                  │   │
│  │  repo_hash_1 ──► RepoContext {                                  │   │
│  │                    base_repo_path, base_branch, feature_branch, │   │
│  │                    worktrees, indexes, event_tx, client_count   │   │
│  │                  }                                              │   │
│  │                                                                  │   │
│  │  repo_hash_2 ──► RepoContext { ... }                           │   │
│  │                                                                  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  WebSocket Server ──► ConnectionState per client                        │
│    - Message routing                                                    │
│    - Event subscriptions                                                │
│    - Query handling                                                     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Protocol

All messages are JSON over WebSocket.

### Client → Server Messages

#### Connect

Connect to a repository directory:

```json
{
  "type": "connect",
  "directory": "/path/to/repo"
}
```

Response:

```json
{
  "type": "connected",
  "client_id": "cli_abc123",
  "repo_id": "hash_xyz789",
  "base_repo_path": "/path/to/repo",
  "base_branch": "main",
  "feature_branch": "feat/new-feature",
  "worktrees": [...],
  "indexes": [...]
}
```

#### Subscribe

Subscribe to event types:

```json
{
  "type": "subscribe",
  "events": ["base_branch", "active_worktree", "*"]
}
```

Event filters:
- `base_branch` - Events for the base branch index
- `feature_branch` - Events for the feature branch index
- `active_worktree` - Events for the currently active worktree
- `worktree:/path/to/wt` - Events for a specific worktree
- `repo` - Repository-level events
- `*` or `all` - All events

#### Unsubscribe

```json
{
  "type": "unsubscribe",
  "events": ["base_branch"]
}
```

#### Query

Execute a query method:

```json
{
  "type": "query",
  "id": 1,
  "method": "get_overview",
  "params": {}
}
```

Response:

```json
{
  "type": "response",
  "id": 1,
  "result": { ... }
}
```

#### Ping

```json
{ "type": "ping" }
```

Response: `{ "type": "pong" }`

### Server → Client Messages

#### Event

Pushed when subscribed events occur:

```json
{
  "type": "event",
  "name": "layer_updated:working",
  "payload": {
    "files_changed": 3,
    "symbols_updated": 15
  }
}
```

#### Error

```json
{
  "type": "error",
  "id": 1,
  "code": "query_error",
  "message": "Symbol not found"
}
```

## Query Methods

### Repository Info

#### `get_repo_info`

Get basic repository information.

```json
{ "method": "get_repo_info", "params": {} }
```

Response:
```json
{
  "repo_hash": "abc123",
  "base_repo_path": "/home/user/project",
  "base_branch": "main",
  "feature_branch": "feat/auth",
  "client_count": 2
}
```

#### `get_index_info`

Get information about all indexes.

```json
{ "method": "get_index_info", "params": {} }
```

### Index Queries

All index queries support an optional `scope` parameter to target a specific index:

- `"base_branch"` or `"base"` - Base branch index (default)
- `"feature_branch"` or `"feature"` - Feature branch index
- `"worktree:/path/to/wt"` - Specific worktree index
- `"worktree:dirname"` - Worktree by directory name

#### `get_overview`

Get the repository overview from the index.

```json
{ "method": "get_overview", "params": { "scope": "base_branch" } }
```

#### `search_symbols`

Search for symbols by name.

```json
{
  "method": "search_symbols",
  "params": {
    "query": "authenticate",
    "limit": 20,
    "scope": "base_branch"
  }
}
```

#### `list_all_symbols`

Get all symbols from the index (for call graph visualization).

```json
{
  "method": "list_all_symbols",
  "params": {
    "limit": 1000,
    "scope": "base_branch"
  }
}
```

#### `get_symbol`

Get detailed information for a specific symbol by hash.

```json
{
  "method": "get_symbol",
  "params": {
    "hash": "abc123def456",
    "scope": "base_branch"
  }
}
```

### Call Graph Queries

#### `get_call_graph`

Get the complete call graph.

```json
{ "method": "get_call_graph", "params": { "scope": "base_branch" } }
```

#### `get_symbol_callees`

Get what a symbol calls (fan-out).

```json
{
  "method": "get_symbol_callees",
  "params": {
    "hash": "abc123",
    "scope": "base_branch"
  }
}
```

#### `get_symbol_callers`

Get what calls a symbol (fan-in).

```json
{
  "method": "get_symbol_callers",
  "params": {
    "hash": "abc123",
    "scope": "base_branch"
  }
}
```

#### `get_call_graph_for_symbol`

Get bidirectional call graph centered on a symbol.

```json
{
  "method": "get_call_graph_for_symbol",
  "params": {
    "hash": "abc123",
    "depth": 2,
    "scope": "base_branch"
  }
}
```

Response:
```json
{
  "center": { "hash": "abc123", "name": "handleAuth", "kind": "fn", "module": "auth" },
  "upstream": [...],
  "downstream": [...],
  "depth": 2,
  "scope": "base_branch"
}
```

### Worktree Management

#### `list_worktrees`

List all discovered worktrees.

```json
{ "method": "list_worktrees", "params": {} }
```

#### `refresh_worktrees`

Re-scan for worktrees.

```json
{ "method": "refresh_worktrees", "params": {} }
```

#### `list_indexes`

List all available indexes.

```json
{ "method": "list_indexes", "params": {} }
```

## Auto-Indexing

When a client connects to a repository:

1. **Base Repository**: If no index exists, the daemon automatically indexes the base repository.
2. **Worktrees**: Each discovered worktree is auto-indexed if no cache exists.
3. **File Watchers**: File watchers are started for the base repo and all worktrees.
4. **Live Updates**: File changes trigger incremental re-indexing and event broadcasts.

## Multi-Repo Support

- Each repository gets a unique `RepoContext` identified by a hash of its git remote URL.
- Multiple clients can connect to the same repository and share the same `RepoContext`.
- When the last client disconnects, the `RepoContext` may be evicted to free resources.

## Event Throttling

File change events are throttled with a leading-edge 500ms window:
- The first event is broadcast immediately
- Subsequent events within 500ms are dropped
- After 500ms, the next event is broadcast immediately

This prevents event flooding during rapid file changes (e.g., during git operations).

## Client Example (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:9847');

ws.onopen = () => {
  // Connect to a repository
  ws.send(JSON.stringify({
    type: 'connect',
    directory: '/path/to/my/project'
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  switch (msg.type) {
    case 'connected':
      console.log('Connected to repo:', msg.repo_id);

      // Subscribe to events
      ws.send(JSON.stringify({
        type: 'subscribe',
        events: ['*']
      }));

      // Query the index
      ws.send(JSON.stringify({
        type: 'query',
        id: 1,
        method: 'get_overview',
        params: {}
      }));
      break;

    case 'response':
      console.log('Query response:', msg.result);
      break;

    case 'event':
      console.log('Event:', msg.name, msg.payload);
      break;

    case 'error':
      console.error('Error:', msg.message);
      break;
  }
};
```

## Logging

Control logging with the `RUST_LOG` environment variable:

```bash
# Default logging
semfora-daemon

# Debug logging
RUST_LOG=semfora_mcp=debug semfora-daemon

# Trace logging (very verbose)
RUST_LOG=semfora_mcp=trace semfora-daemon
```

## Cache Locations

Indexes are stored in `~/.cache/semfora-mcp/`:

- Base repository: `~/.cache/semfora-mcp/{repo_hash}/`
- Worktrees: `~/.cache/semfora-mcp/{worktree_path_hash}/`

Each cache contains:
- `repo_overview.toon` - Repository overview
- `modules/` - Per-module symbol data
- `symbols/` - Individual symbol details
- `symbol_index.jsonl` - Symbol lookup index (JSON Lines format)
- `call_graph.json` - Function call relationships

## See Also

- [Features](features.md) - Incremental indexing, layered indexes, risk assessment
- [CLI Reference](cli.md) - Command-line interface documentation
- [Main README](../README.md) - Supported languages and architecture
