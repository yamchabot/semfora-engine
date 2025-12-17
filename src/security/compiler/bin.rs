//! Security Pattern Compiler Binary
//!
//! Offline tool that compiles CVE patterns from GHSA and NVD into
//! an embeddable binary database for air-gapped security scanning.
//!
//! Usage:
//!   semfora-security-compiler --output patterns.bin
//!   semfora-security-compiler --fetch-new --since 7d
//!   semfora-security-compiler --manual-only

use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand};

use semfora_engine::security::compiler::{CompilerConfig, PatternCompiler};
use semfora_engine::security::patterns::manual;
use semfora_engine::security::{PatternDatabase, Severity};

#[derive(Parser)]
#[command(name = "semfora-security-compiler")]
#[command(about = "Compile CVE patterns into binary database for air-gapped scanning")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile patterns from all sources (GHSA + NVD + manual)
    Compile {
        /// Output file path for compiled patterns
        #[arg(short, long, default_value = "security_patterns.bin")]
        output: PathBuf,

        /// Skip fetching from GHSA (use cached or manual only)
        #[arg(long)]
        skip_ghsa: bool,

        /// Skip NVD metadata enrichment
        #[arg(long)]
        skip_nvd: bool,

        /// Only compile manual patterns (no network access)
        #[arg(long)]
        manual_only: bool,

        /// Search GitHub for fix commits when not in advisory (slower but more thorough)
        #[arg(long)]
        search_commits: bool,

        /// GitHub token for GHSA API (or set GITHUB_TOKEN env var)
        #[arg(long, env = "GITHUB_TOKEN")]
        github_token: Option<String>,

        /// NVD API key for higher rate limits (or set NVD_API_KEY env var)
        #[arg(long, env = "NVD_API_KEY")]
        nvd_api_key: Option<String>,
    },

    /// Fetch new advisories since a date
    FetchNew {
        /// Time period to fetch (e.g., "7d", "30d", "1y")
        #[arg(long, default_value = "7d")]
        since: String,

        /// Output file path
        #[arg(short, long, default_value = "new_patterns.bin")]
        output: PathBuf,

        /// GitHub token
        #[arg(long, env = "GITHUB_TOKEN")]
        github_token: Option<String>,
    },

    /// List statistics about compiled patterns
    Stats {
        /// Path to compiled pattern database
        #[arg(short, long, default_value = "security_patterns.bin")]
        input: PathBuf,
    },

    /// Validate pattern database integrity
    Validate {
        /// Path to compiled pattern database
        #[arg(short, long, default_value = "security_patterns.bin")]
        input: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            output,
            skip_ghsa,
            skip_nvd,
            manual_only,
            search_commits,
            github_token,
            nvd_api_key,
        } => {
            compile_patterns(
                output,
                skip_ghsa || manual_only,
                skip_nvd || manual_only,
                search_commits && !manual_only,
                github_token,
                nvd_api_key,
            )
            .await?;
        }

        Commands::FetchNew {
            since,
            output,
            github_token,
        } => {
            fetch_new_patterns(since, output, github_token).await?;
        }

        Commands::Stats { input } => {
            show_stats(input)?;
        }

        Commands::Validate { input } => {
            validate_database(input)?;
        }
    }

    Ok(())
}

async fn compile_patterns(
    output: PathBuf,
    skip_ghsa: bool,
    skip_nvd: bool,
    search_commits: bool,
    github_token: Option<String>,
    nvd_api_key: Option<String>,
) -> anyhow::Result<()> {
    let start = Instant::now();

    eprintln!("Compiling security patterns...");

    // Start with manual patterns (always included)
    let mut all_patterns = manual::all_patterns();
    eprintln!("  Loaded {} manual patterns", all_patterns.len());

    // Fetch from GHSA if not skipped
    if !skip_ghsa {
        if let Some(token) = github_token {
            eprintln!("  Fetching from GitHub Security Advisories...");
            if search_commits {
                eprintln!("    (with enhanced commit search - this may take a while)");
            }
            let config = CompilerConfig::default();
            let compiler = PatternCompiler::new(config, Some(token), nvd_api_key.clone());

            match compiler.compile_from_ghsa_with_options(search_commits).await {
                Ok(ghsa_patterns) => {
                    eprintln!("    Fetched {} patterns from GHSA", ghsa_patterns.len());
                    all_patterns.extend(ghsa_patterns);
                }
                Err(e) => {
                    eprintln!("    Warning: GHSA fetch failed: {}", e);
                }
            }
        } else {
            eprintln!("  Skipping GHSA (no GitHub token provided)");
        }
    }

    // Enrich with NVD metadata if not skipped
    if !skip_nvd {
        if let Some(api_key) = nvd_api_key {
            eprintln!("  Enriching with NVD metadata...");
            let config = CompilerConfig::default();
            let compiler = PatternCompiler::new(config, None, Some(api_key));

            for pattern in &mut all_patterns {
                if let Err(e) = compiler.enrich_with_nvd(pattern).await {
                    tracing::debug!("NVD enrichment failed for {}: {}", pattern.cve_id, e);
                }
            }
        } else {
            eprintln!("  Skipping NVD enrichment (no API key provided)");
        }
    }

    // Build database with indices
    let database = PatternDatabase::from_patterns(all_patterns);

    // Serialize to binary
    let encoded = bincode::serialize(&database)?;
    std::fs::write(&output, &encoded)?;

    let elapsed = start.elapsed();
    eprintln!("\nCompilation complete:");
    eprintln!("  Total patterns: {}", database.patterns.len());
    eprintln!("  CWE categories: {}", database.cwe_index.len());
    eprintln!("  Languages: {}", database.lang_index.len());
    eprintln!("  Output size: {} bytes", encoded.len());
    eprintln!("  Time: {:?}", elapsed);
    eprintln!("\nWritten to: {}", output.display());

    Ok(())
}

async fn fetch_new_patterns(
    since: String,
    output: PathBuf,
    github_token: Option<String>,
) -> anyhow::Result<()> {
    let token = github_token.ok_or_else(|| anyhow::anyhow!("GitHub token required for --fetch-new"))?;

    eprintln!("Fetching new advisories since {}...", since);

    let config = CompilerConfig::default();
    let compiler = PatternCompiler::new(config, Some(token), None);

    let patterns = compiler.compile_from_ghsa().await?;

    let database = PatternDatabase::from_patterns(patterns);
    let encoded = bincode::serialize(&database)?;
    std::fs::write(&output, &encoded)?;

    eprintln!("Fetched {} new patterns", database.patterns.len());
    eprintln!("Written to: {}", output.display());

    Ok(())
}

fn show_stats(input: PathBuf) -> anyhow::Result<()> {
    let data = std::fs::read(&input)?;
    let database: PatternDatabase = bincode::deserialize(&data)?;

    println!("Security Pattern Database Statistics");
    println!("====================================");
    println!();
    println!("Total patterns: {}", database.patterns.len());
    println!("Version: {}", database.version);
    println!("Generated: {}", database.generated_at);
    println!();

    // Count by severity
    let mut by_severity = std::collections::HashMap::new();
    for pattern in &database.patterns {
        *by_severity.entry(pattern.severity).or_insert(0) += 1;
    }
    println!("By Severity:");
    for severity in [Severity::Critical, Severity::High, Severity::Medium, Severity::Low] {
        if let Some(count) = by_severity.get(&severity) {
            println!("  {:?}: {}", severity, count);
        }
    }
    println!();

    // Count by CWE
    println!("By CWE Category:");
    let mut cwe_counts: Vec<_> = database.cwe_index.iter().collect();
    cwe_counts.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (cwe, indices) in cwe_counts.iter().take(10) {
        println!("  {}: {} patterns", cwe, indices.len());
    }
    if cwe_counts.len() > 10 {
        println!("  ... and {} more CWE categories", cwe_counts.len() - 10);
    }
    println!();

    // Count by language
    println!("By Language:");
    let mut lang_counts: Vec<_> = database.lang_index.iter().collect();
    lang_counts.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (lang, indices) in lang_counts {
        println!("  {:?}: {} patterns", lang, indices.len());
    }

    Ok(())
}

fn validate_database(input: PathBuf) -> anyhow::Result<()> {
    let data = std::fs::read(&input)?;
    let database: PatternDatabase = bincode::deserialize(&data)?;

    println!("Validating pattern database...");

    let mut errors = 0;
    let mut warnings = 0;

    for (i, pattern) in database.patterns.iter().enumerate() {
        // Check CVE ID format
        if !pattern.cve_id.starts_with("CVE-") && !pattern.cve_id.starts_with("CWE-") {
            eprintln!("  Error: Pattern {} has invalid CVE ID: {}", i, pattern.cve_id);
            errors += 1;
        }

        // Check fingerprints are non-zero (at least one should have data)
        if pattern.call_fingerprint == 0
            && pattern.control_flow_fingerprint == 0
            && pattern.state_fingerprint == 0
        {
            eprintln!("  Warning: Pattern {} ({}) has all zero fingerprints", i, pattern.cve_id);
            warnings += 1;
        }

        // Check confidence is valid
        if pattern.confidence < 0.0 || pattern.confidence > 1.0 {
            eprintln!(
                "  Error: Pattern {} ({}) has invalid confidence: {}",
                i, pattern.cve_id, pattern.confidence
            );
            errors += 1;
        }

        // Check has at least one vulnerable call
        if pattern.vulnerable_calls.is_empty() {
            eprintln!(
                "  Warning: Pattern {} ({}) has no vulnerable calls defined",
                i, pattern.cve_id
            );
            warnings += 1;
        }

        // Check has at least one language
        if pattern.languages.is_empty() {
            eprintln!(
                "  Warning: Pattern {} ({}) has no languages defined",
                i, pattern.cve_id
            );
            warnings += 1;
        }
    }

    // Validate indices
    for (cwe, indices) in &database.cwe_index {
        for &idx in indices {
            if idx >= database.patterns.len() {
                eprintln!("  Error: CWE index {} references invalid pattern {}", cwe, idx);
                errors += 1;
            }
        }
    }

    for (lang, indices) in &database.lang_index {
        for &idx in indices {
            if idx >= database.patterns.len() {
                eprintln!("  Error: Language index {:?} references invalid pattern {}", lang, idx);
                errors += 1;
            }
        }
    }

    println!();
    if errors == 0 && warnings == 0 {
        println!("Validation passed: {} patterns OK", database.patterns.len());
    } else {
        println!(
            "Validation complete: {} errors, {} warnings",
            errors, warnings
        );
    }

    if errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}
