//! Semfora Engine Installer System
//!
//! Provides cross-platform installation and configuration for semfora-engine,
//! including MCP client setup for AI coding assistants.
//!
//! # Supported MCP Clients
//!
//! - Claude Desktop
//! - Claude Code
//! - Cursor
//! - VS Code
//! - OpenAI Codex
//! - Custom export to any path
//!
//! # Usage
//!
//! Interactive setup:
//! ```bash
//! semfora-engine setup
//! ```
//!
//! Non-interactive setup:
//! ```bash
//! semfora-engine setup --non-interactive --clients claude-desktop,claude-code
//! ```
//!
//! Uninstall:
//! ```bash
//! semfora-engine uninstall mcp
//! ```

pub mod agents;
pub mod clients;
pub mod config;
pub mod platform;
pub mod uninstall;
pub mod wizard;

use crate::error::McpDiffError;
use agents::AgentScope;
use clients::{ClientRegistry, ClientStatus, McpClient, McpServerConfig};
use config::SemforaConfig;
use console::style;
use platform::{Platform, SemforaPaths};
use std::path::PathBuf;

/// Arguments for the setup command
#[derive(Debug, Clone)]
pub struct SetupArgs {
    /// Skip interactive prompts
    pub non_interactive: bool,
    /// MCP clients to configure (comma-separated names)
    pub clients: Option<Vec<String>>,
    /// Export config to custom path
    pub export_config: Option<PathBuf>,
    /// Binary path override
    pub binary_path: Option<PathBuf>,
    /// Cache directory override
    pub cache_dir: Option<PathBuf>,
    /// Log level (error, info, debug)
    pub log_level: String,
    /// Dry run - show what would be done
    pub dry_run: bool,
    /// Install Semfora workflow agents
    pub with_agents: bool,
    /// Agent installation scope (global, project, or both)
    pub agents_scope: AgentScope,
    /// Only install agents (skip MCP configuration)
    pub agents_only: bool,
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self {
            non_interactive: false,
            clients: None,
            export_config: None,
            binary_path: None,
            cache_dir: None,
            log_level: "info".to_string(),
            dry_run: false,
            with_agents: false,
            agents_scope: AgentScope::Global,
            agents_only: false,
        }
    }
}

/// Arguments for the uninstall command
#[derive(Debug, Clone)]
pub struct UninstallArgs {
    /// Target: mcp, engine, or all
    pub target: String,
    /// Specific client to remove
    pub client: Option<String>,
    /// Keep cache data
    pub keep_cache: bool,
    /// Skip confirmation
    pub force: bool,
}

impl Default for UninstallArgs {
    fn default() -> Self {
        Self {
            target: "mcp".to_string(),
            client: None,
            keep_cache: false,
            force: false,
        }
    }
}

/// Arguments for the config command
#[derive(Debug, Clone)]
pub struct ConfigArgs {
    /// Subcommand: show, set, reset
    pub command: String,
    /// Key for set command
    pub key: Option<String>,
    /// Value for set command
    pub value: Option<String>,
}

/// Run the setup command
pub fn run_setup(args: SetupArgs) -> Result<(), McpDiffError> {
    if args.non_interactive {
        run_setup_non_interactive(args)
    } else {
        run_setup_interactive(args)
    }
}

/// Run interactive setup
fn run_setup_interactive(args: SetupArgs) -> Result<(), McpDiffError> {
    let wizard = wizard::SetupWizard::new();
    let plan = wizard.run()?;

    if args.dry_run {
        println!();
        println!("{}", style("  Dry run - no changes made").yellow());
        println!();
        println!("  Would configure:");
        for client in &plan.clients {
            println!("    • {}", client);
        }
        return Ok(());
    }

    wizard::execute_plan(&plan)
}

/// Run non-interactive setup
fn run_setup_non_interactive(args: SetupArgs) -> Result<(), McpDiffError> {
    let platform = Platform::detect();
    let paths = SemforaPaths::for_platform(&platform);
    let registry = ClientRegistry::new();

    // Determine engine binary path
    let engine_binary = args.binary_path.unwrap_or_else(|| {
        // Try to find existing binary or use default path
        if let Ok(path) = which::which("semfora-engine") {
            path
        } else {
            paths.engine_binary.clone()
        }
    });

    // Build server config
    let mut server_config =
        McpServerConfig::new(engine_binary.clone()).with_log_level(&args.log_level);

    if let Some(cache_dir) = &args.cache_dir {
        server_config = server_config.with_cache_dir(cache_dir.clone());
    }

    // Determine which clients to configure
    let clients_to_configure: Vec<&str> = if let Some(ref client_list) = args.clients {
        client_list.iter().map(|s| s.as_str()).collect()
    } else {
        // Auto-detect installed clients
        registry
            .detect_all(&platform)
            .iter()
            .filter_map(|(client, status)| {
                if matches!(status, ClientStatus::Found { .. }) {
                    Some(client.name())
                } else {
                    None
                }
            })
            .collect()
    };

    if args.dry_run {
        println!("Dry run - would configure:");
        for name in &clients_to_configure {
            if args.with_agents {
                if let Some(client) = registry.find(name) {
                    if client.supports_agents() {
                        println!("  • {} + agents ({})", name, args.agents_scope);
                    } else {
                        println!("  • {}", name);
                    }
                }
            } else {
                println!("  • {}", name);
            }
        }
        if let Some(ref path) = args.export_config {
            println!("  • Export to: {}", path.display());
        }
        if args.with_agents {
            println!("  Agent scope: {}", args.agents_scope);
        }
        return Ok(());
    }

    // Configure each client
    let mut success_count = 0;
    let mut error_count = 0;

    for client_name in clients_to_configure {
        if let Some(client) = registry.find(client_name) {
            match client.configure(&server_config, &platform) {
                Ok(()) => {
                    println!(
                        "{} Configured {}",
                        style("✓").green(),
                        client.display_name()
                    );
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to configure {}: {}",
                        style("✗").red(),
                        client.display_name(),
                        e
                    );
                    error_count += 1;
                }
            }
        } else {
            eprintln!("{} Unknown client: {}", style("✗").red(), client_name);
            error_count += 1;
        }
    }

    // Handle custom export
    if let Some(export_path) = args.export_config {
        let client = clients::CustomExportClient::new(export_path.clone());
        match client.configure(&server_config, &platform) {
            Ok(()) => {
                println!(
                    "{} Exported to {}",
                    style("✓").green(),
                    export_path.display()
                );
                success_count += 1;
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to export to {}: {}",
                    style("✗").red(),
                    export_path.display(),
                    e
                );
                error_count += 1;
            }
        }
    }

    // Install agents if requested
    if args.with_agents {
        let clients_to_install_agents: Vec<&str> = if let Some(ref client_list) = args.clients {
            client_list.iter().map(|s| s.as_str()).collect()
        } else {
            registry
                .detect_all(&platform)
                .iter()
                .filter_map(|(client, status)| {
                    if matches!(status, ClientStatus::Found { .. }) && client.supports_agents() {
                        Some(client.name())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for client_name in clients_to_install_agents {
            if let Some(client) = registry.find(client_name) {
                if client.supports_agents() {
                    match agents::install_agents(client, args.agents_scope) {
                        Ok(result) => {
                            let total = result.total_installed();
                            if total > 0 {
                                println!(
                                    "{} Installed {} agents for {}",
                                    style("✓").green(),
                                    total,
                                    client.display_name()
                                );
                            } else if !result.skipped_paths.is_empty() {
                                println!(
                                    "{} Agents for {} already up-to-date",
                                    style("✓").dim(),
                                    client.display_name()
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "{} Failed to install agents for {}: {}",
                                style("✗").red(),
                                client.display_name(),
                                e
                            );
                            error_count += 1;
                        }
                    }
                }
            }
        }
    }

    // Update semfora config with configured clients
    if success_count > 0 {
        if let Ok(mut semfora_config) = SemforaConfig::load() {
            if let Some(ref client_list) = args.clients {
                semfora_config.mcp.configured_clients = client_list.clone();
            }
            if let Some(cache_dir) = args.cache_dir {
                semfora_config.cache.dir = Some(cache_dir);
            }
            semfora_config.logging.level = args.log_level;
            let _ = semfora_config.save();
        }
    }

    println!();
    if error_count == 0 {
        println!(
            "{} Setup complete! Configured {} client(s)",
            style("✓").green(),
            success_count
        );
    } else {
        println!(
            "{} Setup completed with errors. Configured {}, failed {}",
            style("⚠").yellow(),
            success_count,
            error_count
        );
    }

    Ok(())
}

/// Run the uninstall command
pub fn run_uninstall(args: UninstallArgs) -> Result<(), McpDiffError> {
    let target = match args.target.as_str() {
        "mcp" => uninstall::UninstallTarget::Mcp,
        "engine" => uninstall::UninstallTarget::Engine,
        "all" => uninstall::UninstallTarget::All,
        _ => {
            return Err(McpDiffError::ConfigError {
                message: format!(
                    "Unknown uninstall target: {}. Must be one of: mcp, engine, all",
                    args.target
                ),
            });
        }
    };

    if args.force {
        let options = uninstall::UninstallOptions {
            target,
            keep_cache: args.keep_cache,
            specific_client: args.client,
            force: true,
        };
        uninstall::execute_uninstall(&options, None)
    } else {
        uninstall::run_interactive_uninstall()
    }
}

/// Run the config command
pub fn run_config(args: ConfigArgs) -> Result<(), McpDiffError> {
    match args.command.as_str() {
        "show" => config::show_config(),
        "set" => {
            let key = args.key.ok_or_else(|| McpDiffError::ConfigError {
                message: "Key is required for 'config set'".to_string(),
            })?;
            let value = args.value.ok_or_else(|| McpDiffError::ConfigError {
                message: "Value is required for 'config set'".to_string(),
            })?;
            config::set_config(&key, &value)
        }
        "reset" => config::reset_config(),
        _ => Err(McpDiffError::ConfigError {
            message: format!(
                "Unknown config command: {}. Must be one of: show, set, reset",
                args.command
            ),
        }),
    }
}

/// Print available clients
pub fn print_available_clients() {
    let registry = ClientRegistry::new();
    let platform = Platform::detect();

    println!("Available MCP clients:");
    println!();

    for client in registry.all() {
        let status = client.detect(&platform);
        let status_str = match status {
            ClientStatus::Found {
                has_semfora: true, ..
            } => style("(configured)").green().to_string(),
            ClientStatus::Found {
                has_semfora: false, ..
            } => style("(detected)").cyan().to_string(),
            ClientStatus::NotFound => style("(not detected)").dim().to_string(),
        };

        println!(
            "  {} - {} {}",
            client.name(),
            client.display_name(),
            status_str
        );
    }

    println!();
    println!("  custom - Export to any path");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_args_default() {
        let args = SetupArgs::default();
        assert!(!args.non_interactive);
        assert_eq!(args.log_level, "info");
    }

    #[test]
    fn test_uninstall_args_default() {
        let args = UninstallArgs::default();
        assert_eq!(args.target, "mcp");
        assert!(!args.force);
    }
}
