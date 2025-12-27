//! Linter execution and result collection.
//!
//! This module provides functions to run linters and collect their output.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use rayon::prelude::*;

use crate::error::{McpDiffError, Result};
use crate::lint::cache::collect_config_hashes;
use crate::lint::detection::detect_linters;
use crate::lint::parsers::parse_linter_output;
use crate::lint::types::{
    DetectedLinter, LintCache, LintResults, LintRunOptions, LintSeverity, SingleLinterResult,
};

/// Run linters with the given options
pub fn run_lint(dir: &Path, options: &LintRunOptions) -> Result<LintResults> {
    let start = std::time::Instant::now();

    // Try to load cached linter detection
    let (mut detected, _used_cache) = if let Some(cache) = LintCache::load(dir) {
        (cache.detected_linters, true)
    } else {
        // Detect available linters and save to cache
        let linters = detect_linters(dir);
        let cache = LintCache {
            schema_version: "1.0".to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            detected_linters: linters.clone(),
            custom_commands: HashMap::new(),
            config_hashes: collect_config_hashes(dir),
        };
        // Best-effort save - don't fail if we can't write cache
        let _ = cache.save(dir);
        (linters, false)
    };

    // Filter to specific linter if requested
    if let Some(ref target_linter) = options.linter {
        detected.retain(|d| d.linter == *target_linter);
    }

    // Filter to only available linters
    detected.retain(|d| d.available);

    if detected.is_empty() {
        return Ok(LintResults {
            success: true,
            error_count: 0,
            warning_count: 0,
            files_with_issues: 0,
            duration_ms: start.elapsed().as_millis() as u64,
            linters: Vec::new(),
            issues: Vec::new(),
        });
    }

    // Run linters in parallel using rayon
    let dir_owned = dir.to_path_buf();
    let options_clone = options.clone();

    let parallel_results: Vec<Result<SingleLinterResult>> = detected
        .par_iter()
        .map(|linter| run_single_linter(linter, &dir_owned, &options_clone))
        .collect();

    // Collect results, handling any errors
    let mut linter_results = Vec::new();
    let mut all_issues = Vec::new();

    for result in parallel_results {
        match result {
            Ok(r) => {
                all_issues.extend(r.issues.clone());
                linter_results.push(r);
            }
            Err(e) => {
                // Log error but continue with other linters
                eprintln!("Warning: Linter failed: {}", e);
            }
        }
    }

    // Apply severity filter
    if let Some(min_severity) = options.severity_filter {
        all_issues.retain(|issue| issue.severity >= min_severity);
    }

    // Apply fixable filter
    if options.fixable_only {
        all_issues.retain(|issue| issue.fix.is_some());
    }

    // Sort issues by file, then line
    all_issues.sort_by(|a, b| match a.file.cmp(&b.file) {
        std::cmp::Ordering::Equal => a.line.cmp(&b.line),
        other => other,
    });

    // Apply limit
    if let Some(limit) = options.limit {
        all_issues.truncate(limit);
    }

    // Calculate totals
    let error_count = all_issues
        .iter()
        .filter(|i| i.severity == LintSeverity::Error)
        .count();
    let warning_count = all_issues
        .iter()
        .filter(|i| i.severity == LintSeverity::Warning)
        .count();
    let files_with_issues: std::collections::HashSet<&str> =
        all_issues.iter().map(|i| i.file.as_str()).collect();

    Ok(LintResults {
        success: error_count == 0,
        error_count,
        warning_count,
        files_with_issues: files_with_issues.len(),
        duration_ms: start.elapsed().as_millis() as u64,
        linters: linter_results,
        issues: all_issues,
    })
}

/// Run a single linter and parse its output
pub fn run_single_linter(
    linter: &DetectedLinter,
    dir: &Path,
    options: &LintRunOptions,
) -> Result<SingleLinterResult> {
    let start = std::time::Instant::now();

    // Choose args based on fix mode
    let args = if options.fix && !options.dry_run {
        linter
            .run_command
            .fix_args
            .as_ref()
            .unwrap_or(&linter.run_command.args)
    } else {
        &linter.run_command.args
    };

    // Build and execute command
    let mut cmd = Command::new(&linter.run_command.program);
    cmd.args(args);

    // Set working directory
    let dir_buf = dir.to_path_buf();
    let cwd = linter.run_command.cwd.as_ref().unwrap_or(&dir_buf);
    cmd.current_dir(cwd);

    // Capture output
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| McpDiffError::IoError {
        path: dir.to_path_buf(),
        message: format!("Failed to run {}: {}", linter.linter.display_name(), e),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    // Parse output based on linter type
    let issues = parse_linter_output(linter.linter, &stdout, &stderr, dir);

    let error_count = issues
        .iter()
        .filter(|i| i.severity == LintSeverity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|i| i.severity == LintSeverity::Warning)
        .count();

    Ok(SingleLinterResult {
        linter: linter.linter,
        success: exit_code == Some(0) && error_count == 0,
        error_count,
        warning_count,
        duration_ms: start.elapsed().as_millis() as u64,
        issues,
        stdout,
        stderr,
        exit_code,
    })
}
