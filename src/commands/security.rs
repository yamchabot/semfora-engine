//! Security command handler - CVE scanning and pattern management

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::cache::CacheDir;
use crate::cli::{OutputFormat, SecurityArgs, SecurityOperation};
use crate::commands::CommandContext;
use crate::duplicate::DuplicateDetector;
use crate::error::{McpDiffError, Result};
use crate::security::patterns::embedded::{load_embedded_patterns, pattern_stats};
use crate::security::{CVEMatch, Severity};
use crate::FunctionSignature;

/// Run the security command
pub fn run_security(args: &SecurityArgs, ctx: &CommandContext) -> Result<String> {
    match &args.operation {
        SecurityOperation::Scan {
            module,
            severity,
            cwe,
            min_similarity,
            limit,
        } => run_cve_scan(
            module.as_deref(),
            severity.as_ref(),
            cwe.as_ref(),
            *min_similarity,
            *limit,
            ctx,
        ),
        SecurityOperation::Update { url, file, force } => run_update_patterns(
            url.as_deref(),
            file.as_ref().map(|p| p.as_path()),
            *force,
            ctx,
        ),
        SecurityOperation::Stats => run_pattern_stats(ctx),
    }
}

/// Load function signatures from cache (same as MCP server)
fn load_signatures(cache: &CacheDir) -> Result<Vec<FunctionSignature>> {
    let sig_path = cache.signature_index_path();
    if !sig_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: "Signature index not found".to_string(),
        });
    }

    let file = fs::File::open(&sig_path)?;
    let reader = BufReader::new(file);

    let mut signatures = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(sig) = serde_json::from_str::<FunctionSignature>(&line) {
            signatures.push(sig);
        }
    }

    Ok(signatures)
}

/// Scan for CVE vulnerability patterns
fn run_cve_scan(
    module_filter: Option<&str>,
    severity_filter: Option<&Vec<String>>,
    cwe_filter: Option<&Vec<String>>,
    min_similarity: f32,
    limit: usize,
    ctx: &CommandContext,
) -> Result<String> {
    let repo_dir = std::env::current_dir().map_err(|e| McpDiffError::FileNotFound {
        path: format!("current directory: {}", e),
    })?;
    let cache = CacheDir::for_repo(&repo_dir)?;

    if !cache.exists() {
        return Err(McpDiffError::GitError {
            message: "No index found. Run `semfora index generate` first.".to_string(),
        });
    }

    // Load function signatures from index
    let signatures = load_signatures(&cache)?;

    // Load pattern database (embedded at build time)
    let pattern_db = load_embedded_patterns();

    if pattern_db.is_empty() {
        return Ok("No security patterns available.\nRun semfora-security-compiler to generate patterns, then rebuild with --features embedded-patterns.".to_string());
    }

    if ctx.verbose {
        eprintln!(
            "Loaded {} security patterns, scanning {} signatures",
            pattern_db.len(),
            signatures.len()
        );
    }

    // Filter signatures by module if specified
    let signatures_to_scan: Vec<_> = if let Some(module) = module_filter {
        signatures
            .iter()
            .filter(|s| {
                let path = Path::new(&s.file);
                path.components()
                    .filter_map(|c| c.as_os_str().to_str())
                    .any(|part| part == module)
            })
            .collect()
    } else {
        signatures.iter().collect()
    };

    // Run CVE pattern matching
    let detector = DuplicateDetector::new(min_similarity as f64);
    let mut all_matches: Vec<CVEMatch> = Vec::new();

    for sig in &signatures_to_scan {
        let matches = detector.match_cve_patterns(sig, &pattern_db, min_similarity);
        all_matches.extend(matches);
    }

    // Apply severity filter
    if let Some(severities) = severity_filter {
        let sev_set: Vec<Severity> = severities
            .iter()
            .filter_map(|s| match s.to_uppercase().as_str() {
                "CRITICAL" => Some(Severity::Critical),
                "HIGH" => Some(Severity::High),
                "MEDIUM" => Some(Severity::Medium),
                "LOW" => Some(Severity::Low),
                "NONE" => Some(Severity::None),
                _ => None,
            })
            .collect();
        all_matches.retain(|m| sev_set.contains(&m.severity));
    }

    // Apply CWE filter
    if let Some(cwes) = cwe_filter {
        all_matches.retain(|m| m.cwe_ids.iter().any(|c| cwes.contains(c)));
    }

    // Sort by severity then similarity
    all_matches.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then(
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    all_matches.truncate(limit);

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "cve_scan",
        "functions_scanned": signatures_to_scan.len(),
        "patterns_checked": pattern_db.len(),
        "matches": all_matches.iter().map(|m| serde_json::json!({
            "function": m.function,
            "file": m.file,
            "line": m.line,
            "cve_id": m.cve_id,
            "severity": format!("{:?}", m.severity),
            "cwe_ids": m.cwe_ids,
            "similarity": m.similarity,
            "description": m.description,
            "remediation": m.remediation
        })).collect::<Vec<_>>(),
        "count": all_matches.len(),
        "threshold": min_similarity
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  CVE VULNERABILITY SCAN\n");
            output.push_str("═══════════════════════════════════════════\n\n");

            output.push_str(&format!(
                "functions_scanned: {}\n",
                signatures_to_scan.len()
            ));
            output.push_str(&format!("patterns_checked: {}\n", pattern_db.len()));
            output.push_str(&format!("threshold: {:.0}%\n", min_similarity * 100.0));
            output.push_str(&format!("matches: {}\n\n", all_matches.len()));

            if all_matches.is_empty() {
                output.push_str("No vulnerability patterns detected.\n");
            } else {
                for m in &all_matches {
                    output.push_str("───────────────────────────────────────────\n");
                    output.push_str(&format!(
                        "[{:?}] {} ({:.0}% match)\n",
                        m.severity,
                        m.cve_id,
                        m.similarity * 100.0
                    ));
                    output.push_str(&format!("function: {}\n", m.function));
                    output.push_str(&format!("file: {}:{}\n", m.file, m.line));
                    output.push_str(&format!("cwes: {}\n", m.cwe_ids.join(", ")));
                    output.push_str(&format!("description: {}\n", m.description));
                    if let Some(ref rem) = m.remediation {
                        output.push_str(&format!("remediation: {}\n", rem));
                    }
                    output.push('\n');
                }
            }
        }
    }

    Ok(output)
}

/// Update security patterns
fn run_update_patterns(
    url: Option<&str>,
    file_path: Option<&std::path::Path>,
    force: bool,
    ctx: &CommandContext,
) -> Result<String> {
    use crate::security::patterns::{fetch_pattern_updates, update_patterns_from_file};

    let mut output = String::new();

    if let Some(path) = file_path {
        // Load from file
        if ctx.verbose {
            eprintln!("Loading patterns from: {}", path.display());
        }

        match update_patterns_from_file(path) {
            Ok(result) => {
                let json_value = serde_json::json!({
                    "_type": "pattern_update",
                    "success": true,
                    "updated": result.updated,
                    "previous_version": result.previous_version,
                    "current_version": result.current_version,
                    "pattern_count": result.pattern_count,
                    "message": result.message
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str("Security patterns updated successfully.\n\n");
                        output.push_str(&format!("version: {}\n", result.current_version));
                        output.push_str(&format!("patterns: {}\n", result.pattern_count));
                        output.push_str(&format!("message: {}\n", result.message));
                    }
                }
            }
            Err(e) => {
                let json_value = serde_json::json!({
                    "_type": "pattern_update",
                    "success": false,
                    "error": e.to_string()
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str(&format!("Failed to update patterns: {}\n", e));
                    }
                }
            }
        }
    } else {
        // Fetch from URL (async - needs runtime)
        // For CLI, we'll use a blocking approach
        let rt = tokio::runtime::Runtime::new().map_err(|e| McpDiffError::GitError {
            message: format!("Failed to create async runtime: {}", e),
        })?;

        if ctx.verbose {
            if let Some(u) = url {
                eprintln!("Fetching patterns from: {}", u);
            } else {
                eprintln!("Fetching patterns from default server...");
            }
        }

        match rt.block_on(fetch_pattern_updates(url, force)) {
            Ok(result) => {
                let json_value = serde_json::json!({
                    "_type": "pattern_update",
                    "success": true,
                    "updated": result.updated,
                    "previous_version": result.previous_version,
                    "current_version": result.current_version,
                    "pattern_count": result.pattern_count,
                    "message": result.message
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str("Security patterns updated successfully.\n\n");
                        output.push_str(&format!("version: {}\n", result.current_version));
                        output.push_str(&format!("patterns: {}\n", result.pattern_count));
                        output.push_str(&format!("message: {}\n", result.message));
                    }
                }
            }
            Err(e) => {
                let json_value = serde_json::json!({
                    "_type": "pattern_update",
                    "success": false,
                    "error": e.to_string()
                });

                match ctx.format {
                    OutputFormat::Json => {
                        output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                    }
                    OutputFormat::Toon => {
                        output = super::encode_toon(&json_value);
                    }
                    OutputFormat::Text => {
                        output.push_str(&format!("Failed to update patterns: {}\n", e));
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Show security pattern statistics
fn run_pattern_stats(ctx: &CommandContext) -> Result<String> {
    let stats = pattern_stats();

    let mut output = String::new();

    let json_value = serde_json::json!({
        "_type": "pattern_stats",
        "loaded": stats.loaded,
        "version": stats.version,
        "generated_at": stats.generated_at,
        "pattern_count": stats.pattern_count,
        "cwe_count": stats.cwe_count,
        "language_count": stats.language_count,
        "source": format!("{:?}", stats.source)
    });

    match ctx.format {
        OutputFormat::Json => {
            output = serde_json::to_string_pretty(&json_value).unwrap_or_default();
        }
        OutputFormat::Toon => {
            output = super::encode_toon(&json_value);
        }
        OutputFormat::Text => {
            output.push_str("═══════════════════════════════════════════\n");
            output.push_str("  SECURITY PATTERN STATISTICS\n");
            output.push_str("═══════════════════════════════════════════\n\n");

            output.push_str(&format!("loaded: {}\n", stats.loaded));
            if let Some(ref v) = stats.version {
                output.push_str(&format!("version: {}\n", v));
            }
            if let Some(ref g) = stats.generated_at {
                output.push_str(&format!("generated_at: {}\n", g));
            }
            output.push_str(&format!("patterns: {}\n", stats.pattern_count));
            output.push_str(&format!("cwes_covered: {}\n", stats.cwe_count));
            output.push_str(&format!("languages: {}\n", stats.language_count));
            output.push_str(&format!("source: {:?}\n", stats.source));
        }
    }

    Ok(output)
}
