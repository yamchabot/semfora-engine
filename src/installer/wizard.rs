//! Interactive setup wizard for semfora-engine installation.

use crate::error::McpDiffError;
use crate::installer::agents::{install_agents, AgentScope};
use crate::installer::clients::{ClientRegistry, ClientStatus, McpClient, McpServerConfig};
use crate::installer::platform::{Platform, SemforaPaths};
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

/// Installation mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    /// CLI commands only
    CliOnly,
    /// MCP server for AI assistants
    McpServer,
    /// Reconfigure existing installation
    Configure,
    /// Uninstall
    Uninstall,
}

/// Result of the setup wizard
#[derive(Debug)]
pub struct SetupPlan {
    /// Installation mode
    pub mode: InstallMode,
    /// Selected MCP clients to configure
    pub clients: Vec<String>,
    /// Custom export paths
    pub custom_paths: Vec<PathBuf>,
    /// Engine binary path (CLI + MCP server)
    pub engine_binary: PathBuf,
    /// Log level
    pub log_level: String,
    /// Custom cache directory
    pub cache_dir: Option<PathBuf>,
    /// Whether to proceed (user confirmed)
    pub confirmed: bool,
    /// Clients to install agents for
    pub agents_for_clients: Vec<String>,
    /// Agent installation scope
    pub agents_scope: AgentScope,
}

/// Interactive setup wizard
pub struct SetupWizard {
    platform: Platform,
    client_registry: ClientRegistry,
    term: Term,
    theme: ColorfulTheme,
}

impl SetupWizard {
    /// Create a new setup wizard
    pub fn new() -> Self {
        Self {
            platform: Platform::detect(),
            client_registry: ClientRegistry::new(),
            term: Term::stderr(),
            theme: ColorfulTheme::default(),
        }
    }

    /// Run the interactive wizard
    pub fn run(&self) -> Result<SetupPlan, McpDiffError> {
        self.show_welcome()?;

        let mode = self.select_mode()?;

        match mode {
            InstallMode::Uninstall => self.plan_uninstall(),
            InstallMode::CliOnly => self.plan_cli_only(),
            InstallMode::McpServer => self.plan_mcp_server(),
            InstallMode::Configure => self.plan_configure(),
        }
    }

    /// Show welcome message
    fn show_welcome(&self) -> Result<(), McpDiffError> {
        self.term.clear_screen().ok();
        println!();
        println!(
            "{}",
            style("  Welcome to Semfora Engine Setup!").bold().cyan()
        );
        println!();
        println!("  Platform: {}", style(&self.platform).dim());
        println!();

        Ok(())
    }

    /// Select installation mode
    fn select_mode(&self) -> Result<InstallMode, McpDiffError> {
        let options = &[
            "CLI only (semfora-engine commands)",
            "MCP Server (for AI coding assistants) ← Recommended",
            "Configure existing installation",
            "Uninstall",
        ];

        let selection = Select::with_theme(&self.theme)
            .with_prompt("What would you like to do?")
            .items(options)
            .default(1)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        Ok(match selection {
            0 => InstallMode::CliOnly,
            1 => InstallMode::McpServer,
            2 => InstallMode::Configure,
            3 => InstallMode::Uninstall,
            _ => InstallMode::McpServer,
        })
    }

    /// Plan CLI-only installation
    fn plan_cli_only(&self) -> Result<SetupPlan, McpDiffError> {
        let paths = SemforaPaths::for_platform(&self.platform);

        println!();
        println!("  {} CLI-only mode selected", style("✓").green());
        println!();

        let confirmed = Confirm::with_theme(&self.theme)
            .with_prompt("Proceed with CLI installation?")
            .default(true)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Confirmation cancelled: {}", e),
            })?;

        Ok(SetupPlan {
            mode: InstallMode::CliOnly,
            clients: vec![],
            custom_paths: vec![],
            engine_binary: paths.engine_binary,
            log_level: "info".to_string(),
            cache_dir: None,
            confirmed,
            agents_for_clients: vec![],
            agents_scope: AgentScope::Global,
        })
    }

    /// Plan MCP server installation
    fn plan_mcp_server(&self) -> Result<SetupPlan, McpDiffError> {
        let paths = SemforaPaths::for_platform(&self.platform);

        // Detect installed clients
        println!();
        println!("  {} Detecting installed AI tools...", style("⠋").cyan());

        let detected = self.client_registry.detect_all(&self.platform);

        // Show detection results
        println!();
        for (client, status) in &detected {
            match status {
                ClientStatus::Found { path, has_semfora } => {
                    let semfora_status = if *has_semfora {
                        style("(already configured)").dim()
                    } else {
                        style("").dim()
                    };
                    println!(
                        "  {} Found {} at {}  {}",
                        style("✓").green(),
                        client.display_name(),
                        style(path.display()).dim(),
                        semfora_status
                    );
                }
                ClientStatus::NotFound => {
                    println!(
                        "  {} {} not detected",
                        style("○").dim(),
                        client.display_name()
                    );
                }
            }
        }
        println!();

        // Select clients to configure
        let client_names: Vec<&str> = self.client_registry.display_names();
        let defaults: Vec<bool> = detected
            .iter()
            .map(|(_, status)| matches!(status, ClientStatus::Found { .. }))
            .collect();

        let selections = MultiSelect::with_theme(&self.theme)
            .with_prompt(
                "Which AI tools would you like to configure? (Space to toggle, Enter to confirm)",
            )
            .items(&client_names)
            .defaults(&defaults)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        let selected_clients: Vec<String> = selections
            .iter()
            .filter_map(|&i| self.client_registry.all().get(i))
            .map(|c| c.name().to_string())
            .collect();

        // Ask about custom export
        let mut custom_paths = Vec::new();
        if Confirm::with_theme(&self.theme)
            .with_prompt("Export config to a custom path as well?")
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            let path: String = Input::with_theme(&self.theme)
                .with_prompt("Enter the path for custom export")
                .interact_text()
                .map_err(|e| McpDiffError::ConfigError {
                    message: format!("Input cancelled: {}", e),
                })?;
            custom_paths.push(PathBuf::from(path));
        }

        // Configure options
        let (log_level, cache_dir) = self.configure_options()?;

        // Ask about workflow agents for clients that support them
        let (agents_for_clients, agents_scope) =
            self.configure_agents(&selected_clients)?;

        // Show summary
        self.show_summary(
            &selected_clients,
            &custom_paths,
            &paths,
            &log_level,
            &cache_dir,
            &agents_for_clients,
            &agents_scope,
        )?;

        let confirmed = Confirm::with_theme(&self.theme)
            .with_prompt("Proceed with installation?")
            .default(true)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Confirmation cancelled: {}", e),
            })?;

        Ok(SetupPlan {
            mode: InstallMode::McpServer,
            clients: selected_clients,
            custom_paths,
            engine_binary: paths.engine_binary,
            log_level,
            cache_dir,
            confirmed,
            agents_for_clients,
            agents_scope,
        })
    }

    /// Plan reconfiguration
    fn plan_configure(&self) -> Result<SetupPlan, McpDiffError> {
        // Same as MCP server but with different messaging
        self.plan_mcp_server()
    }

    /// Plan uninstallation
    fn plan_uninstall(&self) -> Result<SetupPlan, McpDiffError> {
        let paths = SemforaPaths::for_platform(&self.platform);

        println!();
        println!("  {} Uninstall mode", style("⚠").yellow());
        println!();

        let options = &[
            "Remove MCP configs only",
            "Remove engine binary and cache",
            "Remove everything",
        ];

        let selection = Select::with_theme(&self.theme)
            .with_prompt("What would you like to remove?")
            .items(options)
            .default(0)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        let mode = match selection {
            0 => InstallMode::Uninstall,
            1 | 2 => InstallMode::Uninstall,
            _ => InstallMode::Uninstall,
        };

        let confirmed = Confirm::with_theme(&self.theme)
            .with_prompt("Are you sure you want to uninstall?")
            .default(false)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Confirmation cancelled: {}", e),
            })?;

        Ok(SetupPlan {
            mode,
            clients: vec![],
            custom_paths: vec![],
            engine_binary: paths.engine_binary,
            log_level: "info".to_string(),
            cache_dir: None,
            confirmed,
            agents_for_clients: vec![],
            agents_scope: AgentScope::Global,
        })
    }

    /// Configure workflow agents for selected clients
    fn configure_agents(
        &self,
        selected_clients: &[String],
    ) -> Result<(Vec<String>, AgentScope), McpDiffError> {
        // Find which clients support agents
        let clients_with_agent_support: Vec<&str> = selected_clients
            .iter()
            .filter_map(|name| {
                self.client_registry.find(name).and_then(|client| {
                    if client.supports_agents() {
                        Some(name.as_str())
                    } else {
                        None
                    }
                })
            })
            .collect();

        if clients_with_agent_support.is_empty() {
            return Ok((vec![], AgentScope::Global));
        }

        println!();
        println!(
            "  {} Semfora provides workflow agents (subagents) for AI assistants:",
            style("ℹ").cyan()
        );
        println!("    • semfora-audit   - Full codebase audits");
        println!("    • semfora-search  - Code search workflows");
        println!("    • semfora-review  - PR/diff review");
        println!("    • semfora-impact  - Impact analysis");
        println!("    • semfora-quality - Quality validation");
        println!();

        let install_agents = Confirm::with_theme(&self.theme)
            .with_prompt("Install Semfora workflow agents?")
            .default(true)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        if !install_agents {
            return Ok((vec![], AgentScope::Global));
        }

        // Ask which clients to install agents for
        let defaults: Vec<bool> = clients_with_agent_support.iter().map(|_| true).collect();

        let agent_selections = MultiSelect::with_theme(&self.theme)
            .with_prompt("Install agents for which tools?")
            .items(&clients_with_agent_support)
            .defaults(&defaults)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        let agents_for: Vec<String> = agent_selections
            .iter()
            .filter_map(|&i| clients_with_agent_support.get(i).map(|s| s.to_string()))
            .collect();

        if agents_for.is_empty() {
            return Ok((vec![], AgentScope::Global));
        }

        // Ask about scope
        let scope_options = &[
            "Global only (~/.claude/agents/, etc.)",
            "Project only (.claude/agents/, etc.)",
            "Both global and project",
        ];

        let scope_selection = Select::with_theme(&self.theme)
            .with_prompt("Where should agents be installed?")
            .items(scope_options)
            .default(0)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        let scope = match scope_selection {
            0 => AgentScope::Global,
            1 => AgentScope::Project,
            2 => AgentScope::Both,
            _ => AgentScope::Global,
        };

        Ok((agents_for, scope))
    }

    /// Configure logging and cache options
    fn configure_options(&self) -> Result<(String, Option<PathBuf>), McpDiffError> {
        let use_defaults = Select::with_theme(&self.theme)
            .with_prompt("Configure Semfora settings?")
            .items(&["Use defaults", "Customize (cache dir, log level)"])
            .default(0)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        if use_defaults == 0 {
            return Ok(("info".to_string(), None));
        }

        // Log level selection
        let log_levels = &["error", "info (recommended)", "debug"];
        let log_selection = Select::with_theme(&self.theme)
            .with_prompt("Log level")
            .items(log_levels)
            .default(1)
            .interact()
            .map_err(|e| McpDiffError::ConfigError {
                message: format!("Selection cancelled: {}", e),
            })?;

        let log_level = match log_selection {
            0 => "error",
            1 => "info",
            2 => "debug",
            _ => "info",
        }
        .to_string();

        // Cache directory
        let cache_dir = if Confirm::with_theme(&self.theme)
            .with_prompt("Use custom cache directory?")
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            let path: String = Input::with_theme(&self.theme)
                .with_prompt("Enter cache directory path")
                .interact_text()
                .map_err(|e| McpDiffError::ConfigError {
                    message: format!("Input cancelled: {}", e),
                })?;
            Some(PathBuf::from(path))
        } else {
            None
        };

        Ok((log_level, cache_dir))
    }

    /// Show installation summary
    fn show_summary(
        &self,
        clients: &[String],
        custom_paths: &[PathBuf],
        paths: &SemforaPaths,
        log_level: &str,
        cache_dir: &Option<PathBuf>,
        agents_for_clients: &[String],
        agents_scope: &AgentScope,
    ) -> Result<(), McpDiffError> {
        println!();
        println!("{}", style("  Ready to configure:").bold());
        println!();

        for client in clients {
            let has_agents = agents_for_clients.contains(client);
            if has_agents {
                println!("  • {} {}", client, style("+ agents").cyan());
            } else {
                println!("  • {}", client);
            }
        }
        for path in custom_paths {
            println!("  • Custom: {}", path.display());
        }

        println!();
        println!(
            "  Engine binary: {}",
            style(paths.engine_binary.display()).dim()
        );
        println!("  Log level: {}", style(log_level).dim());
        println!(
            "  Cache: {}",
            style(
                cache_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "default".to_string())
            )
            .dim()
        );

        if !agents_for_clients.is_empty() {
            let scope_str = match agents_scope {
                AgentScope::Global => "global",
                AgentScope::Project => "project",
                AgentScope::Both => "global + project",
            };
            println!("  Agents scope: {}", style(scope_str).dim());
        }

        println!();

        Ok(())
    }
}

impl Default for SetupWizard {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a setup plan
pub fn execute_plan(plan: &SetupPlan) -> Result<(), McpDiffError> {
    if !plan.confirmed {
        println!();
        println!("  {} Setup cancelled", style("✗").red());
        return Ok(());
    }

    let platform = Platform::detect();
    let registry = ClientRegistry::new();

    // Create progress indicator
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(80));

    // Configure MCP server settings
    let mut server_config =
        McpServerConfig::new(plan.engine_binary.clone()).with_log_level(&plan.log_level);

    if let Some(cache_dir) = &plan.cache_dir {
        server_config = server_config.with_cache_dir(cache_dir.clone());
    }

    // Configure each client
    for client_name in &plan.clients {
        if let Some(client) = registry.find(client_name) {
            pb.set_message(format!("Configuring {}...", client.display_name()));

            match client.configure(&server_config, &platform) {
                Ok(()) => {
                    pb.println(format!(
                        "  {} {} configured",
                        style("✓").green(),
                        client.display_name()
                    ));
                }
                Err(e) => {
                    pb.println(format!(
                        "  {} Failed to configure {}: {}",
                        style("✗").red(),
                        client.display_name(),
                        e
                    ));
                }
            }
        }
    }

    // Handle custom exports
    for custom_path in &plan.custom_paths {
        pb.set_message(format!("Exporting to {}...", custom_path.display()));

        let client = crate::installer::clients::CustomExportClient::new(custom_path.clone());
        match client.configure(&server_config, &platform) {
            Ok(()) => {
                pb.println(format!(
                    "  {} Exported to {}",
                    style("✓").green(),
                    custom_path.display()
                ));
            }
            Err(e) => {
                pb.println(format!(
                    "  {} Failed to export to {}: {}",
                    style("✗").red(),
                    custom_path.display(),
                    e
                ));
            }
        }
    }

    // Install workflow agents if requested
    if !plan.agents_for_clients.is_empty() {
        pb.set_message("Installing workflow agents...");

        for client_name in &plan.agents_for_clients {
            if let Some(client) = registry.find(client_name) {
                pb.set_message(format!(
                    "Installing agents for {}...",
                    client.display_name()
                ));

                match install_agents(client, plan.agents_scope) {
                    Ok(result) => {
                        let total = result.total_installed();
                        if total > 0 {
                            pb.println(format!(
                                "  {} Installed {} agents for {}",
                                style("✓").green(),
                                total,
                                client.display_name()
                            ));
                        } else if !result.skipped_paths.is_empty() {
                            pb.println(format!(
                                "  {} Agents for {} already up-to-date",
                                style("✓").dim(),
                                client.display_name()
                            ));
                        }
                    }
                    Err(e) => {
                        pb.println(format!(
                            "  {} Failed to install agents for {}: {}",
                            style("✗").red(),
                            client.display_name(),
                            e
                        ));
                    }
                }
            }
        }
    }

    pb.finish_and_clear();

    // Show completion message
    println!();
    println!("{}", style("  Setup complete!").bold().green());
    println!();
    println!(
        "  {} Restart your AI tools for changes to take effect",
        style("→").cyan()
    );
    println!(
        "  {} Look for \"semfora-engine\" in your tool's MCP servers",
        style("→").cyan()
    );

    if !plan.agents_for_clients.is_empty() {
        println!(
            "  {} Use agents like 'semfora-audit' to run specialized workflows",
            style("→").cyan()
        );
    }

    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_plan_defaults() {
        let plan = SetupPlan {
            mode: InstallMode::McpServer,
            clients: vec!["claude-desktop".to_string()],
            custom_paths: vec![],
            engine_binary: PathBuf::from("/usr/local/bin/semfora-engine"),
            log_level: "info".to_string(),
            cache_dir: None,
            confirmed: true,
            agents_for_clients: vec![],
            agents_scope: AgentScope::Global,
        };

        assert!(plan.confirmed);
        assert_eq!(plan.clients.len(), 1);
    }

    #[test]
    fn test_setup_plan_with_agents() {
        let plan = SetupPlan {
            mode: InstallMode::McpServer,
            clients: vec!["claude-code".to_string()],
            custom_paths: vec![],
            engine_binary: PathBuf::from("/usr/local/bin/semfora-engine"),
            log_level: "info".to_string(),
            cache_dir: None,
            confirmed: true,
            agents_for_clients: vec!["claude-code".to_string()],
            agents_scope: AgentScope::Both,
        };

        assert!(plan.confirmed);
        assert_eq!(plan.agents_for_clients.len(), 1);
        assert_eq!(plan.agents_scope, AgentScope::Both);
    }
}
