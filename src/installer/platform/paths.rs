//! Platform-specific path resolution for MCP client configurations.

use super::Platform;
use std::path::PathBuf;

/// Paths for MCP client configuration files
#[derive(Debug, Clone)]
pub struct McpClientPaths {
    /// Claude Desktop configuration path
    pub claude_desktop: Option<PathBuf>,
    /// Claude Code global configuration path
    pub claude_code_global: Option<PathBuf>,
    /// Claude Code project configuration (relative path)
    pub claude_code_project: PathBuf,
    /// Cursor global configuration path
    pub cursor_global: Option<PathBuf>,
    /// Cursor project configuration (relative path)
    pub cursor_project: PathBuf,
    /// VS Code user settings path
    pub vscode_user: Option<PathBuf>,
    /// VS Code project configuration (relative path)
    pub vscode_project: PathBuf,
    /// OpenAI Codex configuration path (if known)
    pub openai_codex: Option<PathBuf>,
}

impl McpClientPaths {
    /// Get MCP client paths for the current platform
    pub fn for_platform(platform: &Platform) -> Self {
        match platform {
            Platform::MacOS { .. } => Self::macos(),
            Platform::Linux { .. } => Self::linux(),
            Platform::Windows => Self::windows(),
        }
    }

    /// macOS-specific paths
    fn macos() -> Self {
        let home = dirs::home_dir();

        Self {
            claude_desktop: home
                .as_ref()
                .map(|h| h.join("Library/Application Support/Claude/claude_desktop_config.json")),
            claude_code_global: home.as_ref().map(|h| h.join(".claude/mcp.json")),
            claude_code_project: PathBuf::from(".claude/mcp.json"),
            cursor_global: home.as_ref().map(|h| h.join(".cursor/mcp.json")),
            cursor_project: PathBuf::from(".cursor/mcp.json"),
            vscode_user: home
                .as_ref()
                .map(|h| h.join("Library/Application Support/Code/User/settings.json")),
            vscode_project: PathBuf::from(".vscode/mcp.json"),
            openai_codex: None, // TBD - needs research
        }
    }

    /// Linux-specific paths
    fn linux() -> Self {
        let home = dirs::home_dir();
        let config = dirs::config_dir();

        Self {
            claude_desktop: config
                .as_ref()
                .map(|c| c.join("claude/claude_desktop_config.json")),
            claude_code_global: home.as_ref().map(|h| h.join(".claude/mcp.json")),
            claude_code_project: PathBuf::from(".claude/mcp.json"),
            cursor_global: home.as_ref().map(|h| h.join(".cursor/mcp.json")),
            cursor_project: PathBuf::from(".cursor/mcp.json"),
            vscode_user: config.as_ref().map(|c| c.join("Code/User/settings.json")),
            vscode_project: PathBuf::from(".vscode/mcp.json"),
            openai_codex: None, // TBD - needs research
        }
    }

    /// Windows-specific paths
    fn windows() -> Self {
        let home = dirs::home_dir();
        let appdata = std::env::var("APPDATA").ok().map(PathBuf::from);

        Self {
            claude_desktop: appdata
                .as_ref()
                .map(|a| a.join("Claude/claude_desktop_config.json")),
            claude_code_global: home.as_ref().map(|h| h.join(".claude/mcp.json")),
            claude_code_project: PathBuf::from(".claude/mcp.json"),
            cursor_global: home.as_ref().map(|h| h.join(".cursor/mcp.json")),
            cursor_project: PathBuf::from(".cursor/mcp.json"),
            vscode_user: appdata.as_ref().map(|a| a.join("Code/User/settings.json")),
            vscode_project: PathBuf::from(".vscode/mcp.json"),
            openai_codex: None, // TBD - needs research
        }
    }
}

/// Paths for Semfora's own configuration
#[derive(Debug, Clone)]
pub struct SemforaPaths {
    /// Semfora configuration file
    pub config_file: PathBuf,
    /// Semfora cache directory
    pub cache_dir: PathBuf,
    /// Recommended binary installation directory
    pub binary_dir: PathBuf,
    /// Full path to the engine binary
    pub engine_binary: PathBuf,
    /// Full path to the server binary
    pub server_binary: PathBuf,
}

impl SemforaPaths {
    /// Get Semfora paths for the current platform
    pub fn for_platform(platform: &Platform) -> Self {
        match platform {
            Platform::MacOS { .. } => Self::macos(platform),
            Platform::Linux { .. } => Self::linux(platform),
            Platform::Windows => Self::windows(platform),
        }
    }

    /// macOS-specific paths
    fn macos(platform: &Platform) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let cache = platform.cache_dir().unwrap_or_else(|| home.join(".cache"));

        // Prefer /usr/local/bin if writable, otherwise ~/.local/bin
        let binary_dir = if std::fs::metadata("/usr/local/bin")
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
        {
            home.join(".local/bin")
        } else {
            PathBuf::from("/usr/local/bin")
        };

        let ext = platform.binary_extension();

        Self {
            config_file: home.join(".config/semfora/config.toml"),
            cache_dir: cache.join("semfora"),
            binary_dir: binary_dir.clone(),
            engine_binary: binary_dir.join(format!("semfora-engine{}", ext)),
            server_binary: binary_dir.join(format!("semfora-engine-server{}", ext)),
        }
    }

    /// Linux-specific paths
    fn linux(platform: &Platform) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let cache = platform.cache_dir().unwrap_or_else(|| home.join(".cache"));

        // Prefer /usr/local/bin if writable, otherwise ~/.local/bin
        let binary_dir = if std::fs::metadata("/usr/local/bin")
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
        {
            home.join(".local/bin")
        } else {
            PathBuf::from("/usr/local/bin")
        };

        let ext = platform.binary_extension();

        Self {
            config_file: home.join(".config/semfora/config.toml"),
            cache_dir: cache.join("semfora"),
            binary_dir: binary_dir.clone(),
            engine_binary: binary_dir.join(format!("semfora-engine{}", ext)),
            server_binary: binary_dir.join(format!("semfora-engine-server{}", ext)),
        }
    }

    /// Windows-specific paths
    fn windows(platform: &Platform) -> Self {
        let local_appdata = std::env::var("LOCALAPPDATA")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("C:\\"))
                    .join("AppData\\Local")
            });

        let ext = platform.binary_extension();
        let binary_dir = local_appdata.join("semfora\\bin");

        Self {
            config_file: local_appdata.join("semfora\\config.toml"),
            cache_dir: local_appdata.join("semfora\\cache"),
            binary_dir: binary_dir.clone(),
            engine_binary: binary_dir.join(format!("semfora-engine{}", ext)),
            server_binary: binary_dir.join(format!("semfora-engine-server{}", ext)),
        }
    }

    /// Check if the binary directory is in PATH
    pub fn is_in_path(&self) -> bool {
        if let Ok(path) = std::env::var("PATH") {
            let binary_dir_str = self.binary_dir.to_string_lossy();
            #[cfg(windows)]
            {
                path.split(';')
                    .any(|p| p.eq_ignore_ascii_case(&binary_dir_str))
            }
            #[cfg(not(windows))]
            {
                path.split(':').any(|p| p == binary_dir_str.as_ref())
            }
        } else {
            false
        }
    }

    /// Get instructions for adding binary directory to PATH
    pub fn path_instructions(&self, platform: &Platform) -> String {
        let dir = self.binary_dir.display();
        match platform {
            Platform::MacOS { .. } | Platform::Linux { .. } => {
                format!(
                    "Add the following to your shell profile (~/.bashrc, ~/.zshrc, etc.):\n\n\
                     export PATH=\"{}:$PATH\"",
                    dir
                )
            }
            Platform::Windows => {
                format!(
                    "Add {} to your PATH:\n\n\
                     1. Open System Properties > Advanced > Environment Variables\n\
                     2. Edit the 'Path' variable and add: {}",
                    dir, dir
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_client_paths() {
        let platform = Platform::detect();
        let paths = McpClientPaths::for_platform(&platform);

        // Just ensure paths are generated
        println!("Claude Desktop: {:?}", paths.claude_desktop);
        println!("Claude Code: {:?}", paths.claude_code_global);
        println!("Cursor: {:?}", paths.cursor_global);
        println!("VS Code: {:?}", paths.vscode_user);
    }

    #[test]
    fn test_semfora_paths() {
        let platform = Platform::detect();
        let paths = SemforaPaths::for_platform(&platform);

        println!("Config: {:?}", paths.config_file);
        println!("Cache: {:?}", paths.cache_dir);
        println!("Binary dir: {:?}", paths.binary_dir);
        println!("In PATH: {}", paths.is_in_path());
    }
}
