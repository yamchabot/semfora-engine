//! Semfora workflow agent installation.
//!
//! Provides installation of pre-built workflow agents (audit, search, review,
//! impact, quality) for supported AI coding assistants.
//!
//! # Supported Platforms
//!
//! - Claude Code: Native markdown agents in `~/.claude/agents/`
//! - Cursor: Rules in `~/.cursor/rules/`
//! - Continue.dev (VS Code): Custom commands in `~/.continue/`
//! - OpenAI Codex: AGENTS.md sections
//!
//! # Usage
//!
//! ```bash
//! semfora-engine setup --with-agents --agents-scope global
//! ```

mod templates;

use crate::error::McpDiffError;
use crate::fs_utils;
use std::fs;
use std::path::PathBuf;

/// Scope for agent installation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentScope {
    /// Install to user's global config directory
    #[default]
    Global,
    /// Install to current project directory
    Project,
    /// Install to both global and project directories
    Both,
}

impl std::fmt::Display for AgentScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentScope::Global => write!(f, "global"),
            AgentScope::Project => write!(f, "project"),
            AgentScope::Both => write!(f, "both"),
        }
    }
}

impl std::str::FromStr for AgentScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "global" => Ok(AgentScope::Global),
            "project" => Ok(AgentScope::Project),
            "both" => Ok(AgentScope::Both),
            _ => Err(format!("Invalid scope: {}. Must be global, project, or both", s)),
        }
    }
}

/// Configuration for agent installation
#[derive(Debug, Clone)]
pub struct AgentInstallConfig {
    /// Whether agents should be installed
    pub enabled: bool,
    /// Installation scope
    pub scope: AgentScope,
}

impl Default for AgentInstallConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Default OFF
            scope: AgentScope::Global,
        }
    }
}

/// Result of agent installation
#[derive(Debug, Clone)]
pub struct AgentInstallResult {
    /// Paths where agents were installed
    pub installed_paths: Vec<PathBuf>,
    /// Paths that were skipped (already exist with same content)
    pub skipped_paths: Vec<PathBuf>,
    /// Paths that were updated (existed with different content)
    pub updated_paths: Vec<PathBuf>,
}

impl AgentInstallResult {
    pub fn new() -> Self {
        Self {
            installed_paths: Vec::new(),
            skipped_paths: Vec::new(),
            updated_paths: Vec::new(),
        }
    }

    pub fn total_installed(&self) -> usize {
        self.installed_paths.len() + self.updated_paths.len()
    }
}

impl Default for AgentInstallResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Available agent templates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTemplate {
    Audit,
    Search,
    Review,
    Impact,
    Quality,
}

impl AgentTemplate {
    /// Get all available templates
    pub fn all() -> &'static [AgentTemplate] {
        &[
            AgentTemplate::Audit,
            AgentTemplate::Search,
            AgentTemplate::Review,
            AgentTemplate::Impact,
            AgentTemplate::Quality,
        ]
    }

    /// Get the template name (used in filenames)
    pub fn name(&self) -> &'static str {
        match self {
            AgentTemplate::Audit => "audit",
            AgentTemplate::Search => "search",
            AgentTemplate::Review => "review",
            AgentTemplate::Impact => "impact",
            AgentTemplate::Quality => "quality",
        }
    }

    /// Get the template content (Claude Code native format)
    pub fn content(&self) -> &'static str {
        match self {
            AgentTemplate::Audit => templates::SEMFORA_AUDIT,
            AgentTemplate::Search => templates::SEMFORA_SEARCH,
            AgentTemplate::Review => templates::SEMFORA_REVIEW,
            AgentTemplate::Impact => templates::SEMFORA_IMPACT,
            AgentTemplate::Quality => templates::SEMFORA_QUALITY,
        }
    }

    /// Get the filename for this agent
    pub fn filename(&self) -> String {
        format!("semfora-{}.md", self.name())
    }
}

/// Platform-specific agent support
pub trait AgentSupport {
    /// Check if this platform supports agents
    fn supports_agents(&self) -> bool;

    /// Get the global agents directory for this platform
    fn global_agents_dir(&self) -> Option<PathBuf>;

    /// Get the project agents directory for this platform
    fn project_agents_dir(&self) -> Option<PathBuf>;

    /// Get the file extension for agent files on this platform
    fn agent_file_extension(&self) -> &'static str {
        "md"
    }

    /// Convert a Claude Code agent template to this platform's format
    fn convert_agent_template(&self, template: &AgentTemplate) -> String {
        // Default: pass through unchanged (Claude Code native format)
        template.content().to_string()
    }
}

/// Install agents for a platform that implements AgentSupport
pub fn install_agents<T: AgentSupport + ?Sized>(
    platform_support: &T,
    scope: AgentScope,
) -> Result<AgentInstallResult, McpDiffError> {
    if !platform_support.supports_agents() {
        return Ok(AgentInstallResult::new());
    }

    let mut result = AgentInstallResult::new();

    let dirs_to_install: Vec<PathBuf> = match scope {
        AgentScope::Global => platform_support.global_agents_dir().into_iter().collect(),
        AgentScope::Project => platform_support.project_agents_dir().into_iter().collect(),
        AgentScope::Both => {
            let mut dirs = Vec::new();
            if let Some(global) = platform_support.global_agents_dir() {
                dirs.push(global);
            }
            if let Some(project) = platform_support.project_agents_dir() {
                dirs.push(project);
            }
            dirs
        }
    };

    for dir in dirs_to_install {
        install_agents_to_dir(platform_support, &dir, &mut result)?;
    }

    Ok(result)
}

/// Install all agent templates to a specific directory
fn install_agents_to_dir<T: AgentSupport + ?Sized>(
    platform_support: &T,
    dir: &PathBuf,
    result: &mut AgentInstallResult,
) -> Result<(), McpDiffError> {
    // Create directory if needed
    fs::create_dir_all(dir).map_err(|e| McpDiffError::IoError {
        path: dir.clone(),
        message: e.to_string(),
    })?;

    for template in AgentTemplate::all() {
        let content = platform_support.convert_agent_template(template);
        let filename = format!(
            "semfora-{}.{}",
            template.name(),
            platform_support.agent_file_extension()
        );
        let path = dir.join(&filename);

        install_single_agent(&path, &content, result)?;
    }

    Ok(())
}

/// Install a single agent file
fn install_single_agent(
    path: &PathBuf,
    content: &str,
    result: &mut AgentInstallResult,
) -> Result<(), McpDiffError> {
    if path.exists() {
        // Check if content is the same
        let existing = fs::read_to_string(path).map_err(|e| McpDiffError::IoError {
            path: path.clone(),
            message: e.to_string(),
        })?;

        if existing.trim() == content.trim() {
            result.skipped_paths.push(path.clone());
            return Ok(());
        }

        // Content differs - backup and update
        let backup_path = path.with_extension("md.backup");
        fs::copy(path, &backup_path).map_err(|e| McpDiffError::IoError {
            path: path.clone(),
            message: format!("Failed to create backup: {}", e),
        })?;

        atomic_write(path, content)?;
        result.updated_paths.push(path.clone());
    } else {
        // New file
        atomic_write(path, content)?;
        result.installed_paths.push(path.clone());
    }

    Ok(())
}

/// Write content to a file atomically
fn atomic_write(path: &PathBuf, content: &str) -> Result<(), McpDiffError> {
    let temp_path = path.with_extension("tmp");

    fs::write(&temp_path, content).map_err(|e| McpDiffError::IoError {
        path: temp_path.clone(),
        message: e.to_string(),
    })?;

    fs_utils::atomic_rename(&temp_path, path).map_err(|e| McpDiffError::IoError {
        path: path.clone(),
        message: e.to_string(),
    })?;

    Ok(())
}

/// Uninstall agents from a platform
pub fn uninstall_agents<T: AgentSupport + ?Sized>(
    platform_support: &T,
    scope: AgentScope,
) -> Result<Vec<PathBuf>, McpDiffError> {
    if !platform_support.supports_agents() {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    let prefix = "semfora-";

    let dirs: Vec<PathBuf> = match scope {
        AgentScope::Global => platform_support.global_agents_dir().into_iter().collect(),
        AgentScope::Project => platform_support.project_agents_dir().into_iter().collect(),
        AgentScope::Both => {
            let mut dirs = Vec::new();
            if let Some(global) = platform_support.global_agents_dir() {
                dirs.push(global);
            }
            if let Some(project) = platform_support.project_agents_dir() {
                dirs.push(project);
            }
            dirs
        }
    };

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        let entries = fs::read_dir(&dir).map_err(|e| McpDiffError::IoError {
            path: dir.clone(),
            message: e.to_string(),
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".md") {
                fs::remove_file(entry.path()).map_err(|e| McpDiffError::IoError {
                    path: entry.path(),
                    message: e.to_string(),
                })?;
                removed.push(entry.path());
            }
        }
    }

    Ok(removed)
}

/// Check if agents are installed for a platform
pub fn agents_installed<T: AgentSupport + ?Sized>(platform_support: &T, scope: AgentScope) -> bool {
    if !platform_support.supports_agents() {
        return false;
    }

    let dirs: Vec<PathBuf> = match scope {
        AgentScope::Global => platform_support.global_agents_dir().into_iter().collect(),
        AgentScope::Project => platform_support.project_agents_dir().into_iter().collect(),
        AgentScope::Both => {
            let mut dirs = Vec::new();
            if let Some(global) = platform_support.global_agents_dir() {
                dirs.push(global);
            }
            if let Some(project) = platform_support.project_agents_dir() {
                dirs.push(project);
            }
            dirs
        }
    };

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        // Check if at least one semfora agent exists
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("semfora-") && name.ends_with(".md") {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_scope_display() {
        assert_eq!(AgentScope::Global.to_string(), "global");
        assert_eq!(AgentScope::Project.to_string(), "project");
        assert_eq!(AgentScope::Both.to_string(), "both");
    }

    #[test]
    fn test_agent_scope_parse() {
        assert_eq!("global".parse::<AgentScope>().unwrap(), AgentScope::Global);
        assert_eq!("project".parse::<AgentScope>().unwrap(), AgentScope::Project);
        assert_eq!("both".parse::<AgentScope>().unwrap(), AgentScope::Both);
        assert!("invalid".parse::<AgentScope>().is_err());
    }

    #[test]
    fn test_agent_templates() {
        for template in AgentTemplate::all() {
            assert!(!template.content().is_empty());
            assert!(template.filename().starts_with("semfora-"));
            assert!(template.filename().ends_with(".md"));
        }
    }

    #[test]
    fn test_agent_install_config_default() {
        let config = AgentInstallConfig::default();
        assert!(!config.enabled); // Default OFF
        assert_eq!(config.scope, AgentScope::Global);
    }
}
