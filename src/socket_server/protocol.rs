//! Socket server protocol message types
//!
//! Defines the JSON message format for client-server communication.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Client-to-server message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Connect to a directory
    Connect {
        directory: PathBuf,
    },
    /// Subscribe to events
    Subscribe {
        events: Vec<String>,
    },
    /// Unsubscribe from events
    Unsubscribe {
        events: Vec<String>,
    },
    /// Query the server
    Query {
        id: u64,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    /// Ping to check connection
    Ping,
}

/// Server-to-client message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Connection established
    Connected(ConnectionInfo),
    /// Subscription confirmed
    Subscribed {
        events: Vec<String>,
    },
    /// Unsubscription confirmed
    Unsubscribed {
        events: Vec<String>,
    },
    /// Query response
    Response {
        id: u64,
        result: serde_json::Value,
    },
    /// Error response
    Error {
        id: Option<u64>,
        code: String,
        message: String,
    },
    /// Event notification
    Event {
        name: String,
        payload: serde_json::Value,
    },
    /// Pong response
    Pong,
}

/// Connection info returned after successful connect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub client_id: String,
    pub repo_id: String,
    pub base_repo_path: PathBuf,
    pub base_branch: String,
    pub feature_branch: Option<String>,
    pub worktrees: Vec<WorktreeInfo>,
    pub indexes: Vec<IndexInfo>,
}

/// Information about a worktree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
    pub is_semfora: bool,
    pub head_sha: String,
}

/// Information about an index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub id: String,
    pub scope: String,
    pub symbol_count: usize,
    pub status: IndexStatus,
}

/// Index status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexStatus {
    Ready,
    Updating,
    Stale,
    Error,
}

/// Event filter for subscriptions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventFilter {
    BaseBranch,
    FeatureBranch,
    ActiveWorktree,
    Worktree(PathBuf),
    Repo,
    All,
}

impl EventFilter {
    /// Parse a filter string into an EventFilter
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "base_branch" => Some(Self::BaseBranch),
            "feature_branch" => Some(Self::FeatureBranch),
            "active_worktree" => Some(Self::ActiveWorktree),
            "repo" => Some(Self::Repo),
            "*" | "all" => Some(Self::All),
            s if s.starts_with("worktree:") => {
                let path = s.strip_prefix("worktree:")?;
                Some(Self::Worktree(PathBuf::from(path)))
            }
            _ => None,
        }
    }

    /// Check if this filter matches an event name
    pub fn matches(&self, event_name: &str) -> bool {
        match self {
            Self::BaseBranch => event_name.starts_with("base_branch:"),
            Self::FeatureBranch => event_name.starts_with("feature_branch:"),
            Self::ActiveWorktree => event_name.starts_with("active_worktree:"),
            Self::Worktree(path) => {
                event_name.starts_with(&format!("worktree:{}:", path.display()))
            }
            Self::Repo => event_name.starts_with("repo:"),
            Self::All => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_parse() {
        let json = r#"{"type":"connect","directory":"/home/user/project"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Connect { directory } => {
                assert_eq!(directory, PathBuf::from("/home/user/project"));
            }
            _ => panic!("Expected Connect message"),
        }
    }

    #[test]
    fn test_event_filter_matches() {
        let filter = EventFilter::BaseBranch;
        assert!(filter.matches("base_branch:index_updated"));
        assert!(!filter.matches("feature_branch:index_updated"));

        let filter = EventFilter::Worktree(PathBuf::from("/tmp/worktree"));
        assert!(filter.matches("worktree:/tmp/worktree:file_changed"));
        assert!(!filter.matches("worktree:/other/path:file_changed"));
    }
}
