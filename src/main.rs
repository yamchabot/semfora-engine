//! Semfora Engine CLI entry point
//!
//! This is the main entry point for the semfora-engine CLI. It uses a subcommand-based
//! architecture where each command is handled by a dedicated module in `commands/`.

use std::process::ExitCode;

use semfora_engine::analyze_repo_tokens;
use semfora_engine::cli::{Cli, Commands, ConfigOperation};
use semfora_engine::commands::{
    run_analyze, run_cache, run_commit, run_index, run_query, run_search, run_security, run_test,
    run_validate, CommandContext,
};
use semfora_engine::installer::{
    self, print_available_clients, ConfigArgs, SetupArgs, UninstallArgs,
};

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            if !output.is_empty() {
                print!("{}", output);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            e.exit_code()
        }
    }
}

fn run() -> semfora_engine::Result<String> {
    let cli = Cli::parse_args();

    // Create shared context for command handlers
    let ctx = CommandContext::from_cli(cli.format, cli.verbose, cli.progress);

    // Dispatch to appropriate command handler
    match cli.command {
        // ============================================
        // Core Analysis Commands
        // ============================================
        Commands::Analyze(args) => run_analyze(&ctx, &args),

        Commands::Search(args) => run_search(&args, &ctx),

        Commands::Query(args) => run_query(&args, &ctx),

        Commands::Validate(args) => run_validate(&args, &ctx),

        // ============================================
        // Index & Cache Management
        // ============================================
        Commands::Index(args) => run_index(&args, &ctx),

        Commands::Cache(args) => run_cache(&args, &ctx),

        // ============================================
        // Security & Testing
        // ============================================
        Commands::Security(args) => run_security(&args, &ctx),

        Commands::Test(args) => run_test(&args, &ctx),

        // ============================================
        // Git Integration
        // ============================================
        Commands::Commit(args) => run_commit(&args, &ctx),

        // ============================================
        // Installation & Configuration
        // ============================================
        Commands::Setup(args) => {
            // Handle --list-clients flag
            if args.list_clients {
                print_available_clients();
                return Ok(String::new());
            }

            let setup_args = SetupArgs {
                non_interactive: args.non_interactive,
                clients: args.clients.clone(),
                export_config: args.export_config.clone(),
                binary_path: args.binary_path.clone(),
                cache_dir: args.cache_dir.clone(),
                log_level: args.log_level.clone(),
                dry_run: args.dry_run,
            };

            installer::run_setup(setup_args)?;
            Ok(String::new())
        }

        Commands::Uninstall(args) => {
            let uninstall_args = UninstallArgs {
                target: args.target.clone(),
                client: args.client.clone(),
                keep_cache: args.keep_cache,
                force: args.force,
            };

            installer::run_uninstall(uninstall_args)?;
            Ok(String::new())
        }

        Commands::Config(args) => {
            let config_args = match &args.operation {
                ConfigOperation::Show => ConfigArgs {
                    command: "show".to_string(),
                    key: None,
                    value: None,
                },
                ConfigOperation::Set { key, value } => ConfigArgs {
                    command: "set".to_string(),
                    key: Some(key.clone()),
                    value: Some(value.clone()),
                },
                ConfigOperation::Reset => ConfigArgs {
                    command: "reset".to_string(),
                    key: None,
                    value: None,
                },
            };

            installer::run_config(config_args)?;
            Ok(String::new())
        }

        // ============================================
        // Utilities
        // ============================================
        Commands::Benchmark(args) => {
            let dir_path = args.path.clone().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            });
            run_benchmark(&dir_path)
        }
    }
}

/// Run token efficiency benchmark
fn run_benchmark(dir_path: &std::path::Path) -> semfora_engine::Result<String> {
    let metrics = analyze_repo_tokens(dir_path)?;

    let mut output = String::new();
    output.push_str("═══════════════════════════════════════════════════════\n");
    output.push_str("  SEMFORA TOKEN EFFICIENCY BENCHMARK\n");
    output.push_str("═══════════════════════════════════════════════════════\n\n");

    output.push_str(&format!("Repository: {}\n", dir_path.display()));
    output.push_str(&format!("Files analyzed: {}\n\n", metrics.files.len()));

    output.push_str("───────────────────────────────────────────────────────\n");
    output.push_str("  RAW FILE READS (baseline)\n");
    output.push_str("───────────────────────────────────────────────────────\n");
    output.push_str(&format!(
        "  Total tokens: {} ({:.2} MB equivalent)\n",
        metrics.total_source_tokens,
        metrics.total_source_tokens as f64 * 4.0 / (1024.0 * 1024.0)
    ));

    output.push_str("\n───────────────────────────────────────────────────────\n");
    output.push_str("  SEMANTIC QUERIES (semfora-engine)\n");
    output.push_str("───────────────────────────────────────────────────────\n");
    output.push_str(&format!(
        "  Total tokens: {} ({:.2} MB equivalent)\n",
        metrics.total_toon_tokens,
        metrics.total_toon_tokens as f64 * 4.0 / (1024.0 * 1024.0)
    ));

    output.push_str("\n───────────────────────────────────────────────────────\n");
    output.push_str("  EFFICIENCY\n");
    output.push_str("───────────────────────────────────────────────────────\n");
    let compression_ratio = if metrics.total_toon_tokens > 0 {
        metrics.total_source_tokens as f64 / metrics.total_toon_tokens as f64
    } else {
        1.0
    };
    output.push_str(&format!("  Compression ratio: {:.1}x\n", compression_ratio));
    output.push_str(&format!(
        "  Token savings: {:.1}%\n",
        metrics.total_token_savings * 100.0
    ));

    Ok(output)
}
