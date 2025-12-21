//! OpenAI Codex MCP client configuration.
//!
//! Note: OpenAI Codex MCP configuration is still being researched.
//! This implementation provides a best-effort based on common patterns.

use super::{json_utils, ClientStatus, McpClient, McpServerConfig};
use crate::error::McpDiffError;
use crate::installer::agents::{AgentSupport, AgentTemplate};
use crate::installer::platform::Platform;
use std::path::PathBuf;

/// OpenAI Codex MCP client
///
/// Configuration location is TBD as OpenAI Codex MCP support is evolving.
/// Currently tries common locations based on similar tools.
pub struct OpenAICodexClient;

impl OpenAICodexClient {
    /// Get potential config paths for OpenAI Codex
    fn potential_paths(platform: &Platform) -> Vec<PathBuf> {
        let home = platform.home_dir();
        let config = platform.config_dir();

        let mut paths = Vec::new();

        // Try common patterns based on other tools
        if let Some(h) = &home {
            paths.push(h.join(".codex/mcp.json"));
            paths.push(h.join(".openai/codex/mcp.json"));
            paths.push(h.join(".config/codex/mcp.json"));
        }

        if let Some(c) = &config {
            paths.push(c.join("codex/mcp.json"));
            paths.push(c.join("openai-codex/mcp.json"));
        }

        paths
    }
}

impl McpClient for OpenAICodexClient {
    fn name(&self) -> &'static str {
        "openai-codex"
    }

    fn display_name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn detect(&self, platform: &Platform) -> ClientStatus {
        // Check all potential paths
        for path in Self::potential_paths(platform) {
            if path.exists() {
                let has_semfora = json_utils::read_json_config(&path)
                    .map(|c| json_utils::has_semfora_server(&c))
                    .unwrap_or(false);

                return ClientStatus::Found { path, has_semfora };
            }
        }

        ClientStatus::NotFound
    }

    fn config_path(&self, platform: &Platform) -> Option<PathBuf> {
        // Return the first potential path that exists, or a default
        for path in Self::potential_paths(platform) {
            if path.exists() {
                return Some(path);
            }
        }

        // Default to ~/.codex/mcp.json
        platform.home_dir().map(|h| h.join(".codex/mcp.json"))
    }

    fn project_config_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from(".codex/mcp.json"))
    }

    fn configure(&self, config: &McpServerConfig, platform: &Platform) -> Result<(), McpDiffError> {
        let config_path = self
            .config_path(platform)
            .ok_or_else(|| McpDiffError::ConfigError {
                message: "Could not determine OpenAI Codex config path".to_string(),
            })?;

        // Create backup
        self.backup_config(platform)?;

        // Load or create config
        let mut json_config = if config_path.exists() {
            json_utils::read_json_config(&config_path)?
        } else {
            serde_json::json!({})
        };

        // Add semfora-engine server (using mcpServers format as default)
        let server_json = config.to_mcp_servers_json();
        json_utils::add_mcp_server(&mut json_config, &server_json)?;

        // Write config
        json_utils::write_json_config(&config_path, &json_config)?;

        Ok(())
    }

    fn unconfigure(&self, platform: &Platform) -> Result<(), McpDiffError> {
        // Check all potential paths
        for config_path in Self::potential_paths(platform) {
            if !config_path.exists() {
                continue;
            }

            // Create backup
            json_utils::backup_config(&config_path)?;

            // Load and modify config
            if let Ok(mut json_config) = json_utils::read_json_config(&config_path) {
                json_utils::remove_mcp_server(&mut json_config);
                json_utils::write_json_config(&config_path, &json_config)?;
            }
        }

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

impl AgentSupport for OpenAICodexClient {
    fn supports_agents(&self) -> bool {
        true
    }

    fn global_agents_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".codex"))
    }

    fn project_agents_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from(".codex"))
    }

    fn convert_agent_template(&self, template: &AgentTemplate) -> String {
        // Codex uses AGENTS.md format with markdown sections
        // Each agent becomes a section in the file
        let content = template.content();
        let body = extract_body_from_template(content);

        format!(
            r#"## Semfora {} Agent

{}
"#,
            capitalize(template.name()),
            body
        )
    }
}

/// Extract the body content from a template (after YAML frontmatter)
fn extract_body_from_template(content: &str) -> &str {
    if content.starts_with("---") {
        if let Some(end_idx) = content[3..].find("---") {
            let body_start = end_idx + 6;
            if body_start < content.len() {
                return content[body_start..].trim_start();
            }
        }
    }
    content
}

/// Capitalize first letter
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_name() {
        let client = OpenAICodexClient;
        assert_eq!(client.name(), "openai-codex");
        assert_eq!(client.display_name(), "OpenAI Codex");
    }

    #[test]
    fn test_agent_support() {
        let client = OpenAICodexClient;
        assert!(client.supports_agents());
    }
}
