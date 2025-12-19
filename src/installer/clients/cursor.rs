//! Cursor IDE MCP client configuration.

use super::{json_utils, ClientStatus, McpClient, McpServerConfig};
use crate::error::McpDiffError;
use crate::installer::platform::{McpClientPaths, Platform};
use std::path::PathBuf;

/// Cursor IDE MCP client
pub struct CursorClient;

impl McpClient for CursorClient {
    fn name(&self) -> &'static str {
        "cursor"
    }

    fn display_name(&self) -> &'static str {
        "Cursor"
    }

    fn detect(&self, platform: &Platform) -> ClientStatus {
        let paths = McpClientPaths::for_platform(platform);

        if let Some(config_path) = &paths.cursor_global {
            if config_path.exists() {
                let has_semfora = json_utils::read_json_config(config_path)
                    .map(|c| json_utils::has_semfora_server(&c))
                    .unwrap_or(false);

                return ClientStatus::Found {
                    path: config_path.clone(),
                    has_semfora,
                };
            }
        }

        ClientStatus::NotFound
    }

    fn config_path(&self, platform: &Platform) -> Option<PathBuf> {
        McpClientPaths::for_platform(platform).cursor_global
    }

    fn project_config_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from(".cursor/mcp.json"))
    }

    fn configure(&self, config: &McpServerConfig, platform: &Platform) -> Result<(), McpDiffError> {
        let config_path = self
            .config_path(platform)
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "Could not determine Cursor config path".to_string(),
            })?;

        // Create backup
        self.backup_config(platform)?;

        // Load or create config
        let mut json_config = if config_path.exists() {
            json_utils::read_json_config(&config_path)?
        } else {
            serde_json::json!({})
        };

        // Add semfora-engine server
        let server_json = config.to_mcp_servers_json();
        json_utils::add_mcp_server(&mut json_config, &server_json)?;

        // Write config
        json_utils::write_json_config(&config_path, &json_config)?;

        Ok(())
    }

    fn unconfigure(&self, platform: &Platform) -> Result<(), McpDiffError> {
        let config_path = self
            .config_path(platform)
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "Could not determine Cursor config path".to_string(),
            })?;

        if !config_path.exists() {
            return Ok(());
        }

        // Create backup
        self.backup_config(platform)?;

        // Load config
        let mut json_config = json_utils::read_json_config(&config_path)?;

        // Remove semfora-engine
        json_utils::remove_mcp_server(&mut json_config);

        // Write config
        json_utils::write_json_config(&config_path, &json_config)?;

        Ok(())
    }

    fn backup_config(&self, platform: &Platform) -> Result<Option<PathBuf>, McpDiffError> {
        if let Some(config_path) = self.config_path(platform) {
            json_utils::backup_config(&config_path)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_name() {
        let client = CursorClient;
        assert_eq!(client.name(), "cursor");
        assert_eq!(client.display_name(), "Cursor");
    }
}
