//! Custom export client for exporting MCP configuration to user-specified paths.
//!
//! This allows users to export the semfora-engine MCP configuration to any
//! location, supporting use cases we haven't anticipated.

#![allow(dead_code)]

use super::{json_utils, ClientStatus, McpClient, McpServerConfig};
use crate::error::McpDiffError;
use crate::installer::platform::Platform;
use std::path::PathBuf;

/// Custom export client that writes MCP config to a user-specified path
pub struct CustomExportClient {
    /// The output path for the configuration
    output_path: PathBuf,
}

impl CustomExportClient {
    /// Create a new custom export client with the specified output path
    pub fn new(output_path: PathBuf) -> Self {
        Self { output_path }
    }

    /// Get the output path
    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}

impl McpClient for CustomExportClient {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn display_name(&self) -> &'static str {
        "Custom Export"
    }

    fn detect(&self, _platform: &Platform) -> ClientStatus {
        if self.output_path.exists() {
            let has_semfora = json_utils::read_json_config(&self.output_path)
                .map(|c| json_utils::has_semfora_server(&c))
                .unwrap_or(false);

            ClientStatus::Found {
                path: self.output_path.clone(),
                has_semfora,
            }
        } else {
            ClientStatus::NotFound
        }
    }

    fn config_path(&self, _platform: &Platform) -> Option<PathBuf> {
        Some(self.output_path.clone())
    }

    fn project_config_path(&self) -> Option<PathBuf> {
        None
    }

    fn configure(
        &self,
        config: &McpServerConfig,
        _platform: &Platform,
    ) -> Result<(), McpDiffError> {
        // Create backup if file exists
        if self.output_path.exists() {
            json_utils::backup_config(&self.output_path)?;
        }

        // Load or create config
        let mut json_config = if self.output_path.exists() {
            json_utils::read_json_config(&self.output_path)?
        } else {
            serde_json::json!({})
        };

        // Add semfora-engine server
        let server_json = config.to_mcp_servers_json();
        json_utils::add_mcp_server(&mut json_config, &server_json)?;

        // Write config
        json_utils::write_json_config(&self.output_path, &json_config)?;

        Ok(())
    }

    fn unconfigure(&self, _platform: &Platform) -> Result<(), McpDiffError> {
        if !self.output_path.exists() {
            return Ok(());
        }

        // Create backup
        json_utils::backup_config(&self.output_path)?;

        // Load and modify config
        let mut json_config = json_utils::read_json_config(&self.output_path)?;
        json_utils::remove_mcp_server(&mut json_config);
        json_utils::write_json_config(&self.output_path, &json_config)?;

        Ok(())
    }

    fn backup_config(&self, _platform: &Platform) -> Result<Option<PathBuf>, McpDiffError> {
        json_utils::backup_config(&self.output_path)
    }
}

/// Export only the semfora-engine MCP server configuration without merging
/// into an existing file.
pub fn export_standalone_config(
    config: &McpServerConfig,
    output_path: &std::path::Path,
) -> Result<(), McpDiffError> {
    let json_config = serde_json::json!({
        "mcpServers": {
            "semfora-engine": config.to_mcp_servers_json()
        }
    });

    json_utils::write_json_config(output_path, &json_config)
}

/// Export only the server entry (for manual merging)
pub fn export_server_entry_only(
    config: &McpServerConfig,
    output_path: &std::path::Path,
) -> Result<(), McpDiffError> {
    let json_config = config.to_mcp_servers_json();
    json_utils::write_json_config(output_path, &json_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_custom_client() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("custom-mcp.json");

        let client = CustomExportClient::new(path.clone());
        assert_eq!(client.name(), "custom");
        assert_eq!(client.output_path(), &path);
    }

    #[test]
    fn test_standalone_export() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("standalone.json");

        let config = McpServerConfig::new(PathBuf::from("/usr/local/bin/semfora-engine-server"));
        export_standalone_config(&config, &path).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("semfora-engine"));
        assert!(content.contains("mcpServers"));
    }
}
