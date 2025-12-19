//! Uninstall functionality for semfora-engine.

use crate::error::McpDiffError;
use crate::installer::clients::{ClientRegistry, ClientStatus};
use crate::installer::platform::{Platform, SemforaPaths};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use std::fs;

/// Uninstall target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallTarget {
    /// Remove MCP configurations only
    Mcp,
    /// Remove engine binary and cache
    Engine,
    /// Remove everything
    All,
}

/// Options for uninstallation
#[derive(Debug, Clone)]
pub struct UninstallOptions {
    /// What to uninstall
    pub target: UninstallTarget,
    /// Keep cache data when removing engine
    pub keep_cache: bool,
    /// Specific client to remove (if Mcp target)
    pub specific_client: Option<String>,
    /// Skip confirmation prompts
    pub force: bool,
}

impl Default for UninstallOptions {
    fn default() -> Self {
        Self {
            target: UninstallTarget::Mcp,
            keep_cache: false,
            specific_client: None,
            force: false,
        }
    }
}

/// Run interactive uninstall
pub fn run_interactive_uninstall() -> Result<(), McpDiffError> {
    let platform = Platform::detect();
    let registry = ClientRegistry::new();
    let theme = ColorfulTheme::default();

    println!();
    println!("{}", style("  Semfora Engine Uninstall").bold().yellow());
    println!();

    // Detect configured clients
    let detected = registry.detect_all(&platform);
    let configured: Vec<(&str, &std::path::Path)> = detected
        .iter()
        .filter_map(|(client, status)| {
            if let ClientStatus::Found {
                path,
                has_semfora: true,
            } = status
            {
                Some((client.display_name(), path.as_path()))
            } else {
                None
            }
        })
        .collect();

    if configured.is_empty() {
        println!("  No semfora-engine configurations found.");
        println!();
        return Ok(());
    }

    println!("  Found semfora-engine configured in:");
    for (name, path) in &configured {
        println!("    • {} ({})", name, style(path.display()).dim());
    }
    println!();

    // Select what to uninstall
    let options = &[
        "Remove MCP configs only",
        "Remove engine cache",
        "Remove everything (configs + cache)",
    ];

    let selection = dialoguer::Select::with_theme(&theme)
        .with_prompt("What would you like to remove?")
        .items(options)
        .default(0)
        .interact()
        .map_err(|e| McpDiffError::ConfigError {
            message: format!("Selection cancelled: {}", e),
        })?;

    let target = match selection {
        0 => UninstallTarget::Mcp,
        1 => UninstallTarget::Engine,
        2 => UninstallTarget::All,
        _ => UninstallTarget::Mcp,
    };

    // If MCP only, let user select which clients
    let clients_to_remove = if target == UninstallTarget::Mcp || target == UninstallTarget::All {
        let client_names: Vec<&str> = configured.iter().map(|(n, _)| *n).collect();
        let defaults: Vec<bool> = vec![true; client_names.len()];

        let selections = MultiSelect::with_theme(&theme)
            .with_prompt("Which configurations to remove?")
            .items(&client_names)
            .defaults(&defaults)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        selections
            .iter()
            .filter_map(|&i| configured.get(i).map(|(n, _)| n.to_string()))
            .collect()
    } else {
        vec![]
    };

    // Confirm
    let confirm_msg = match target {
        UninstallTarget::Mcp => "Remove selected MCP configurations?",
        UninstallTarget::Engine => "Remove engine cache?",
        UninstallTarget::All => "Remove all semfora-engine data?",
    };

    if !Confirm::with_theme(&theme)
        .with_prompt(confirm_msg)
        .default(false)
        .interact()
        .unwrap_or(false)
    {
        println!();
        println!("  {} Uninstall cancelled", style("✗").red());
        return Ok(());
    }

    // Execute uninstall
    let options = UninstallOptions {
        target,
        keep_cache: false,
        specific_client: None,
        force: true,
    };

    execute_uninstall(&options, Some(&clients_to_remove))?;

    println!();
    println!("{}", style("  Uninstall complete!").bold().green());
    println!();
    println!(
        "  {} Restart your AI tools for changes to take effect",
        style("→").cyan()
    );
    println!();

    Ok(())
}

/// Execute uninstall with given options
pub fn execute_uninstall(
    options: &UninstallOptions,
    clients_to_remove: Option<&[String]>,
) -> Result<(), McpDiffError> {
    let platform = Platform::detect();

    match options.target {
        UninstallTarget::Mcp => {
            uninstall_mcp_configs(
                &platform,
                clients_to_remove,
                options.specific_client.as_deref(),
            )?;
        }
        UninstallTarget::Engine => {
            uninstall_engine(&platform, options.keep_cache)?;
        }
        UninstallTarget::All => {
            uninstall_mcp_configs(
                &platform,
                clients_to_remove,
                options.specific_client.as_deref(),
            )?;
            uninstall_engine(&platform, options.keep_cache)?;
        }
    }

    Ok(())
}

/// Remove MCP configurations
fn uninstall_mcp_configs(
    platform: &Platform,
    clients_to_remove: Option<&[String]>,
    specific_client: Option<&str>,
) -> Result<(), McpDiffError> {
    let registry = ClientRegistry::new();

    for client in registry.all() {
        // Check if we should process this client
        let should_process = match (clients_to_remove, specific_client) {
            (_, Some(specific)) => client.name() == specific || client.display_name() == specific,
            (Some(list), _) => list
                .iter()
                .any(|n| n == client.name() || n == client.display_name()),
            (None, None) => true,
        };

        if !should_process {
            continue;
        }

        // Check if client has semfora configured
        if let ClientStatus::Found {
            has_semfora: true, ..
        } = client.detect(platform)
        {
            match client.unconfigure(platform) {
                Ok(()) => {
                    println!(
                        "  {} Removed from {}",
                        style("✓").green(),
                        client.display_name()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "  {} Failed to remove from {}: {}",
                        style("✗").red(),
                        client.display_name(),
                        e
                    );
                }
            }
        }
    }

    Ok(())
}

/// Remove engine binary and cache
fn uninstall_engine(platform: &Platform, keep_cache: bool) -> Result<(), McpDiffError> {
    let paths = SemforaPaths::for_platform(platform);

    // Remove cache directory
    if !keep_cache && paths.cache_dir.exists() {
        match fs::remove_dir_all(&paths.cache_dir) {
            Ok(()) => {
                println!(
                    "  {} Removed cache at {}",
                    style("✓").green(),
                    paths.cache_dir.display()
                );
            }
            Err(e) => {
                eprintln!("  {} Failed to remove cache: {}", style("✗").red(), e);
            }
        }
    }

    // Remove config file
    if paths.config_file.exists() {
        match fs::remove_file(&paths.config_file) {
            Ok(()) => {
                println!(
                    "  {} Removed config at {}",
                    style("✓").green(),
                    paths.config_file.display()
                );
            }
            Err(e) => {
                eprintln!("  {} Failed to remove config: {}", style("✗").red(), e);
            }
        }
    }

    // Note: We don't remove the binary itself as it's what's running this command
    // Users can remove it manually or via their package manager

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uninstall_options_default() {
        let options = UninstallOptions::default();
        assert_eq!(options.target, UninstallTarget::Mcp);
        assert!(!options.keep_cache);
        assert!(!options.force);
    }
}
