//! VS Code MCP client configuration.
//!
//! VS Code uses a different configuration format than Claude/Cursor.
//! It uses `servers` instead of `mcpServers` and requires a `type` field.

use super::{ClientStatus, McpClient, McpServerConfig};
use crate::error::McpDiffError;
use crate::fs_utils;
use crate::installer::platform::{McpClientPaths, Platform};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::PathBuf;

/// VS Code MCP client
pub struct VSCodeClient;

impl VSCodeClient {
    /// Read VS Code JSON config (handles comments via json5)
    fn read_config(path: &std::path::Path) -> Result<JsonValue, McpDiffError> {
        let content = fs::read_to_string(path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        // Try standard JSON first
        if let Ok(value) = serde_json::from_str(&content) {
            return Ok(value);
        }

        // Fall back to json5 (VS Code configs often have comments)
        json5::from_str(&content).map_err(|e| McpDiffError::ConfigError {
            message: format!("Failed to parse VS Code config: {}", e),
        })
    }

    /// Write config (standard JSON, VS Code handles it)
    fn write_config(path: &std::path::Path, value: &JsonValue) -> Result<(), McpDiffError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| McpDiffError::IoError {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }

        let temp_path = path.with_extension("tmp");
        let content =
            serde_json::to_string_pretty(value).map_err(|e| McpDiffError::ConfigError {
                message: format!("Failed to serialize JSON: {}", e),
            })?;

        fs::write(&temp_path, &content).map_err(|e| McpDiffError::IoError {
            path: temp_path.clone(),
            message: e.to_string(),
        })?;

        fs_utils::atomic_rename(&temp_path, path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        Ok(())
    }

    /// Check if semfora-engine is in VS Code servers config
    fn has_semfora(config: &JsonValue) -> bool {
        config
            .get("servers")
            .and_then(|s| s.get("semfora-engine"))
            .is_some()
    }

    /// Add semfora-engine to VS Code servers
    fn add_server(config: &mut JsonValue, server_config: &JsonValue) -> Result<(), McpDiffError> {
        let servers = config
            .as_object_mut()
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "Config is not an object".to_string(),
            })?
            .entry("servers")
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));

        let servers_obj = servers
            .as_object_mut()
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "servers is not an object".to_string(),
            })?;

        servers_obj.insert("semfora-engine".to_string(), server_config.clone());
        Ok(())
    }

    /// Remove semfora-engine from VS Code servers
    fn remove_server(config: &mut JsonValue) -> bool {
        if let Some(obj) = config.as_object_mut() {
            if let Some(servers) = obj.get_mut("servers") {
                if let Some(servers_obj) = servers.as_object_mut() {
                    return servers_obj.remove("semfora-engine").is_some();
                }
            }
        }
        false
    }
}

impl McpClient for VSCodeClient {
    fn name(&self) -> &'static str {
        "vscode"
    }

    fn display_name(&self) -> &'static str {
        "VS Code"
    }

    fn detect(&self, platform: &Platform) -> ClientStatus {
        let paths = McpClientPaths::for_platform(platform);

        // Check for project-level .vscode/mcp.json first (more specific)
        let project_path = PathBuf::from(".vscode/mcp.json");
        if project_path.exists() {
            let has_semfora = Self::read_config(&project_path)
                .map(|c| Self::has_semfora(&c))
                .unwrap_or(false);

            return ClientStatus::Found {
                path: project_path,
                has_semfora,
            };
        }

        // Then check user settings
        if let Some(user_path) = &paths.vscode_user {
            if user_path.exists() {
                let has_semfora = Self::read_config(user_path)
                    .map(|c| Self::has_semfora(&c))
                    .unwrap_or(false);

                return ClientStatus::Found {
                    path: user_path.clone(),
                    has_semfora,
                };
            }
        }

        ClientStatus::NotFound
    }

    fn config_path(&self, platform: &Platform) -> Option<PathBuf> {
        McpClientPaths::for_platform(platform).vscode_user
    }

    fn project_config_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from(".vscode/mcp.json"))
    }

    fn configure(&self, config: &McpServerConfig, platform: &Platform) -> Result<(), McpDiffError> {
        // Prefer project-level config if .vscode directory exists
        let config_path = if PathBuf::from(".vscode").exists() {
            PathBuf::from(".vscode/mcp.json")
        } else {
            self.config_path(platform)
                .ok_or_else(|| McpDiffError::ConfigError {
                    message: "Could not determine VS Code config path".to_string(),
                })?
        };

        // Create backup
        if config_path.exists() {
            let backup_path = config_path.with_extension("backup.json");
            fs::copy(&config_path, &backup_path).map_err(|e| McpDiffError::IoError {
                path: config_path.clone(),
                message: format!("Failed to create backup: {}", e),
            })?;
        }

        // Load or create config
        let mut json_config = if config_path.exists() {
            Self::read_config(&config_path)?
        } else {
            serde_json::json!({})
        };

        // Add semfora-engine server (VS Code format)
        let server_json = config.to_vscode_servers_json();
        Self::add_server(&mut json_config, &server_json)?;

        // Write config
        Self::write_config(&config_path, &json_config)?;

        Ok(())
    }

    fn unconfigure(&self, platform: &Platform) -> Result<(), McpDiffError> {
        // Check both project and user config
        let configs_to_check: Vec<PathBuf> = vec![
            PathBuf::from(".vscode/mcp.json"),
            self.config_path(platform).unwrap_or_default(),
        ];

        for config_path in configs_to_check {
            if !config_path.exists() {
                continue;
            }

            // Create backup
            let backup_path = config_path.with_extension("backup.json");
            fs::copy(&config_path, &backup_path).ok();

            // Load and modify config
            if let Ok(mut json_config) = Self::read_config(&config_path) {
                Self::remove_server(&mut json_config);
                Self::write_config(&config_path, &json_config)?;
            }
        }

        Ok(())
    }

    fn backup_config(&self, platform: &Platform) -> Result<Option<PathBuf>, McpDiffError> {
        if let Some(config_path) = self.config_path(platform) {
            if config_path.exists() {
                let backup_path = config_path.with_extension("backup.json");
                fs::copy(&config_path, &backup_path).map_err(|e| McpDiffError::IoError {
                    path: config_path,
                    message: format!("Failed to create backup: {}", e),
                })?;
                return Ok(Some(backup_path));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_name() {
        let client = VSCodeClient;
        assert_eq!(client.name(), "vscode");
        assert_eq!(client.display_name(), "VS Code");
    }

    #[test]
    fn test_vscode_server_format() {
        let config = McpServerConfig::new(PathBuf::from("/usr/local/bin/semfora-engine-server"));
        let json = config.to_vscode_servers_json();

        assert_eq!(json.get("type").and_then(|v| v.as_str()), Some("stdio"));
        assert!(json.get("command").is_some());
    }
}
