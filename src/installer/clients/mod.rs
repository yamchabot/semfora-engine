//! MCP client detection and configuration.
//!
//! Supports Claude Desktop, Claude Code, Cursor, VS Code, OpenAI Codex,
//! and custom export to user-specified paths.

mod claude_code;
mod claude_desktop;
mod cursor;
mod custom;
mod openai_codex;
mod vscode;

pub use claude_code::ClaudeCodeClient;
pub use claude_desktop::ClaudeDesktopClient;
pub use cursor::CursorClient;
pub use custom::CustomExportClient;
pub use openai_codex::OpenAICodexClient;
pub use vscode::VSCodeClient;

use crate::error::McpDiffError;
use crate::installer::platform::Platform;
use serde_json::Value as JsonValue;
use std::path::PathBuf;

/// Status of an MCP client detection
#[derive(Debug, Clone)]
pub enum ClientStatus {
    /// Client found with configuration file
    Found {
        /// Path to the configuration file
        path: PathBuf,
        /// Whether semfora-engine is already configured
        has_semfora: bool,
    },
    /// Client configuration not found
    NotFound,
}

/// Configuration options for MCP server entry
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Path to the semfora-engine binary (unified CLI + MCP server)
    pub engine_binary: PathBuf,
    /// Log level (error, info, debug)
    pub log_level: String,
    /// Custom cache directory (if any)
    pub cache_dir: Option<PathBuf>,
    /// Additional environment variables
    pub extra_env: std::collections::HashMap<String, String>,
}

impl McpServerConfig {
    /// Create a new MCP server configuration
    pub fn new(engine_binary: PathBuf) -> Self {
        Self {
            engine_binary,
            log_level: "info".to_string(),
            cache_dir: None,
            extra_env: std::collections::HashMap::new(),
        }
    }

    /// Set the log level
    pub fn with_log_level(mut self, level: &str) -> Self {
        self.log_level = level.to_string();
        self
    }

    /// Set a custom cache directory
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = Some(dir);
        self
    }

    /// Generate environment variables for the MCP server
    pub fn env_vars(&self) -> serde_json::Map<String, JsonValue> {
        let mut env = serde_json::Map::new();
        env.insert(
            "RUST_LOG".to_string(),
            JsonValue::String(self.log_level.clone()),
        );

        if let Some(cache_dir) = &self.cache_dir {
            env.insert(
                "XDG_CACHE_HOME".to_string(),
                JsonValue::String(
                    cache_dir
                        .parent()
                        .unwrap_or(cache_dir)
                        .to_string_lossy()
                        .to_string(),
                ),
            );
        }

        for (key, value) in &self.extra_env {
            env.insert(key.clone(), JsonValue::String(value.clone()));
        }

        env
    }

    /// Generate JSON for mcpServers format (Claude, Cursor)
    pub fn to_mcp_servers_json(&self) -> JsonValue {
        serde_json::json!({
            "command": self.engine_binary.to_string_lossy(),
            "args": ["serve"],
            "env": self.env_vars()
        })
    }

    /// Generate JSON for VS Code servers format
    pub fn to_vscode_servers_json(&self) -> JsonValue {
        serde_json::json!({
            "type": "stdio",
            "command": self.engine_binary.to_string_lossy(),
            "args": ["serve"]
        })
    }
}

/// Trait for MCP client implementations
pub trait McpClient: Send + Sync {
    /// Get the client identifier (lowercase, hyphenated)
    fn name(&self) -> &'static str;

    /// Get the human-readable display name
    fn display_name(&self) -> &'static str;

    /// Detect if the client is installed and has configuration
    fn detect(&self, platform: &Platform) -> ClientStatus;

    /// Get the global configuration file path
    fn config_path(&self, platform: &Platform) -> Option<PathBuf>;

    /// Get the project-level configuration file path (if supported)
    fn project_config_path(&self) -> Option<PathBuf>;

    /// Configure this client with semfora-engine
    fn configure(&self, config: &McpServerConfig, platform: &Platform) -> Result<(), McpDiffError>;

    /// Remove semfora-engine from this client's configuration
    fn unconfigure(&self, platform: &Platform) -> Result<(), McpDiffError>;

    /// Create a backup of the current configuration
    fn backup_config(&self, platform: &Platform) -> Result<Option<PathBuf>, McpDiffError>;
}

/// Registry of all available MCP clients
pub struct ClientRegistry {
    clients: Vec<Box<dyn McpClient>>,
}

impl ClientRegistry {
    /// Create a new registry with all supported clients
    pub fn new() -> Self {
        Self {
            clients: vec![
                Box::new(ClaudeDesktopClient),
                Box::new(ClaudeCodeClient),
                Box::new(CursorClient),
                Box::new(VSCodeClient),
                Box::new(OpenAICodexClient),
            ],
        }
    }

    /// Get all clients
    pub fn all(&self) -> &[Box<dyn McpClient>] {
        &self.clients
    }

    /// Find a client by name
    pub fn find(&self, name: &str) -> Option<&dyn McpClient> {
        self.clients
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.as_ref())
    }

    /// Detect all installed clients
    pub fn detect_all(&self, platform: &Platform) -> Vec<(&dyn McpClient, ClientStatus)> {
        self.clients
            .iter()
            .map(|c| (c.as_ref(), c.detect(platform)))
            .collect()
    }

    /// Get all client names
    pub fn names(&self) -> Vec<&'static str> {
        self.clients.iter().map(|c| c.name()).collect()
    }

    /// Get all client display names
    pub fn display_names(&self) -> Vec<&'static str> {
        self.clients.iter().map(|c| c.display_name()).collect()
    }
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for JSON config manipulation
pub(crate) mod json_utils {
    use crate::error::McpDiffError;
    use crate::fs_utils;
    use serde_json::Value as JsonValue;
    use std::fs;
    use std::path::Path;

    /// Read a JSON configuration file, handling comments (json5)
    pub fn read_json_config(path: &Path) -> Result<JsonValue, McpDiffError> {
        let content = fs::read_to_string(path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        // Try standard JSON first
        if let Ok(value) = serde_json::from_str(&content) {
            return Ok(value);
        }

        // Fall back to json5 (handles comments)
        json5::from_str(&content).map_err(|e| McpDiffError::ConfigError {
            message: format!("Failed to parse {}: {}", path.display(), e),
        })
    }

    /// Write JSON to a file atomically
    pub fn write_json_config(path: &Path, value: &JsonValue) -> Result<(), McpDiffError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| McpDiffError::IoError {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }

        // Write to temp file first
        let temp_path = path.with_extension("tmp");
        let content =
            serde_json::to_string_pretty(value).map_err(|e| McpDiffError::ConfigError {
                message: format!("Failed to serialize JSON: {}", e),
            })?;

        fs::write(&temp_path, &content).map_err(|e| McpDiffError::IoError {
            path: temp_path.clone(),
            message: e.to_string(),
        })?;

        // Atomic rename (cross-platform)
        fs_utils::atomic_rename(&temp_path, path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        Ok(())
    }

    /// Create a backup of a configuration file
    pub fn backup_config(path: &Path) -> Result<Option<std::path::PathBuf>, McpDiffError> {
        if !path.exists() {
            return Ok(None);
        }

        let backup_path = path.with_extension("backup.json");
        fs::copy(path, &backup_path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: format!("Failed to create backup: {}", e),
        })?;

        Ok(Some(backup_path))
    }

    /// Add semfora-engine to mcpServers in a config
    pub fn add_mcp_server(
        config: &mut JsonValue,
        server_config: &JsonValue,
    ) -> Result<(), McpDiffError> {
        let servers = config
            .as_object_mut()
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "Config is not an object".to_string(),
            })?
            .entry("mcpServers")
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));

        let servers_obj = servers
            .as_object_mut()
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "mcpServers is not an object".to_string(),
            })?;

        servers_obj.insert("semfora-engine".to_string(), server_config.clone());
        Ok(())
    }

    /// Remove semfora-engine from mcpServers in a config
    pub fn remove_mcp_server(config: &mut JsonValue) -> bool {
        if let Some(obj) = config.as_object_mut() {
            if let Some(servers) = obj.get_mut("mcpServers") {
                if let Some(servers_obj) = servers.as_object_mut() {
                    return servers_obj.remove("semfora-engine").is_some();
                }
            }
        }
        false
    }

    /// Check if semfora-engine is configured in mcpServers
    pub fn has_semfora_server(config: &JsonValue) -> bool {
        config
            .get("mcpServers")
            .and_then(|s| s.get("semfora-engine"))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_registry() {
        let registry = ClientRegistry::new();
        assert!(!registry.all().is_empty());

        // Check all expected clients are present
        assert!(registry.find("claude-desktop").is_some());
        assert!(registry.find("claude-code").is_some());
        assert!(registry.find("cursor").is_some());
        assert!(registry.find("vscode").is_some());
    }

    #[test]
    fn test_mcp_server_config() {
        let config = McpServerConfig::new(PathBuf::from("/usr/local/bin/semfora-engine-server"))
            .with_log_level("debug")
            .with_cache_dir(PathBuf::from("/custom/cache"));

        let json = config.to_mcp_servers_json();
        assert!(json.get("command").is_some());
        assert!(json.get("env").is_some());
    }
}
