//! Semfora configuration management.
//!
//! Handles the semfora-engine configuration file at:
//! - Linux/macOS: ~/.config/semfora/config.toml
//! - Windows: %LOCALAPPDATA%\semfora\config.toml

use crate::error::McpDiffError;
use crate::fs_utils;
use crate::installer::platform::{Platform, SemforaPaths};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Semfora configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemforaConfig {
    /// Cache settings
    #[serde(default)]
    pub cache: CacheConfig,

    /// Logging settings
    #[serde(default)]
    pub logging: LoggingConfig,

    /// MCP configuration
    #[serde(default)]
    pub mcp: McpConfig,

    /// Security pattern settings
    #[serde(default)]
    pub patterns: PatternConfig,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache directory path
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { dir: None }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (error, info, debug)
    #[serde(default = "default_log_level")]
    pub level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// List of configured MCP clients
    #[serde(default)]
    pub configured_clients: Vec<String>,
}

/// Security pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfig {
    /// Pattern server URL
    #[serde(default = "default_pattern_url")]
    pub url: String,
}

fn default_pattern_url() -> String {
    "https://patterns.semfora.dev/security_patterns.bin".to_string()
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            url: default_pattern_url(),
        }
    }
}

impl SemforaConfig {
    /// Load configuration from the default path
    pub fn load() -> Result<Self, McpDiffError> {
        let platform = Platform::detect();
        let paths = SemforaPaths::for_platform(&platform);
        Self::load_from(&paths.config_file)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &std::path::Path) -> Result<Self, McpDiffError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).map_err(|e| McpDiffError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        toml::from_str(&content).map_err(|e| McpDiffError::ConfigError {
            message: format!("Failed to parse config: {}", e),
        })
    }

    /// Save configuration to the default path
    pub fn save(&self) -> Result<(), McpDiffError> {
        let platform = Platform::detect();
        let paths = SemforaPaths::for_platform(&platform);
        self.save_to(&paths.config_file)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), McpDiffError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| McpDiffError::IoError {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| McpDiffError::ConfigError {
            message: format!("Failed to serialize config: {}", e),
        })?;

        // Atomic write
        let temp_path = path.with_extension("tmp");
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

    /// Get a configuration value by key path (e.g., "cache.dir")
    pub fn get(&self, key: &str) -> Option<String> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["cache", "dir"] => self.cache.dir.as_ref().map(|p| p.display().to_string()),
            ["logging", "level"] => Some(self.logging.level.clone()),
            ["patterns", "url"] => Some(self.patterns.url.clone()),
            ["mcp", "configured_clients"] => Some(self.mcp.configured_clients.join(", ")),
            _ => None,
        }
    }

    /// Set a configuration value by key path
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), McpDiffError> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["cache", "dir"] => {
                if value.is_empty() {
                    self.cache.dir = None;
                } else {
                    self.cache.dir = Some(PathBuf::from(value));
                }
            }
            ["logging", "level"] => {
                if !["error", "info", "debug"].contains(&value) {
                    return Err(McpDiffError::ConfigError {
                        message: format!(
                            "Invalid log level: {}. Must be one of: error, info, debug",
                            value
                        ),
                    });
                }
                self.logging.level = value.to_string();
            }
            ["patterns", "url"] => {
                self.patterns.url = value.to_string();
            }
            _ => {
                return Err(McpDiffError::ConfigError {
                    message: format!("Unknown configuration key: {}", key),
                });
            }
        }
        Ok(())
    }

    /// Reset configuration to defaults
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Display configuration as formatted text
    pub fn display(&self) -> String {
        let mut output = String::new();

        output.push_str("[cache]\n");
        if let Some(dir) = &self.cache.dir {
            output.push_str(&format!("dir = \"{}\"\n", dir.display()));
        } else {
            output.push_str("# dir = \"~/.cache/semfora\" (default)\n");
        }

        output.push_str("\n[logging]\n");
        output.push_str(&format!("level = \"{}\"\n", self.logging.level));

        output.push_str("\n[mcp]\n");
        if self.mcp.configured_clients.is_empty() {
            output.push_str("configured_clients = []\n");
        } else {
            output.push_str(&format!(
                "configured_clients = {:?}\n",
                self.mcp.configured_clients
            ));
        }

        output.push_str("\n[patterns]\n");
        output.push_str(&format!("url = \"{}\"\n", self.patterns.url));

        output
    }
}

/// Show current configuration
pub fn show_config() -> Result<(), McpDiffError> {
    let config = SemforaConfig::load()?;
    println!("{}", config.display());
    Ok(())
}

/// Set a configuration value
pub fn set_config(key: &str, value: &str) -> Result<(), McpDiffError> {
    let mut config = SemforaConfig::load()?;
    config.set(key, value)?;
    config.save()?;
    println!("Set {} = {}", key, value);
    Ok(())
}

/// Reset configuration to defaults
pub fn reset_config() -> Result<(), McpDiffError> {
    let mut config = SemforaConfig::load()?;
    config.reset();
    config.save()?;
    println!("Configuration reset to defaults");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = SemforaConfig::default();
        assert_eq!(config.logging.level, "info");
        assert!(config.cache.dir.is_none());
    }

    #[test]
    fn test_config_get_set() {
        let mut config = SemforaConfig::default();

        config.set("logging.level", "debug").unwrap();
        assert_eq!(config.get("logging.level"), Some("debug".to_string()));

        config.set("cache.dir", "/custom/path").unwrap();
        assert_eq!(config.get("cache.dir"), Some("/custom/path".to_string()));
    }

    #[test]
    fn test_config_save_load() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");

        let mut config = SemforaConfig::default();
        config.logging.level = "debug".to_string();
        config.save_to(&config_path).unwrap();

        let loaded = SemforaConfig::load_from(&config_path).unwrap();
        assert_eq!(loaded.logging.level, "debug");
    }
}
