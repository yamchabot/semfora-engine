//! MCP-Diff CLI entry point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use mcp_diff::cli::{OperationMode, TokenAnalysisMode};
use mcp_diff::git::{
    get_changed_files, get_commit_changed_files, get_commits_since, get_file_at_ref,
    get_merge_base, get_repo_root, is_git_repo, ChangedFile, ChangeType,
};
use mcp_diff::{
    encode_toon, extract, format_analysis_compact, format_analysis_report, Cli, Lang, McpDiffError,
    OutputFormat, SemanticSummary, TokenAnalyzer,
};

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{}", output);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            e.exit_code()
        }
    }
}

fn run() -> mcp_diff::Result<String> {
    let cli = Cli::parse();
    let mode = cli.operation_mode()?;

    match mode {
        OperationMode::SingleFile(path) => run_single_file(&cli, &path),
        OperationMode::Directory { path, max_depth } => run_directory(&cli, &path, max_depth),
        OperationMode::DiffBranch { base_ref } => run_diff_branch(&cli, &base_ref),
        OperationMode::SingleCommit { sha } => run_single_commit(&cli, &sha),
        OperationMode::AllCommits { base_ref } => run_all_commits(&cli, &base_ref),
    }
}

/// Run single file analysis (Phase 1 mode)
fn run_single_file(cli: &Cli, file_path: &Path) -> mcp_diff::Result<String> {
    // 1. Check file exists
    if !file_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: file_path.display().to_string(),
        });
    }

    // 2. Detect language from file extension
    let lang = Lang::from_path(file_path)?;

    if cli.verbose {
        eprintln!(
            "Detected language: {} ({})",
            lang.name(),
            lang.family().name()
        );
    }

    // 3. Read source file
    let source = fs::read_to_string(file_path)?;

    if cli.verbose {
        eprintln!("Read {} bytes from {}", source.len(), file_path.display());
    }

    // 4. Parse and extract
    let summary = parse_and_extract(file_path, &source, lang, cli)?;

    // 5. Encode output in all formats for token analysis
    let toon_output = encode_toon(&summary);
    let json_pretty =
        serde_json::to_string_pretty(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;
    let json_compact =
        serde_json::to_string(&summary).map_err(|e| McpDiffError::ExtractionFailure {
            message: format!("JSON serialization failed: {}", e),
        })?;

    // 6. Run token analysis if requested
    if let Some(mode) = cli.analyze_tokens {
        let analyzer = TokenAnalyzer::new();
        let analysis = analyzer.analyze(&source, &json_pretty, &json_compact, &toon_output);

        let report = match mode {
            TokenAnalysisMode::Full => format_analysis_report(&analysis, cli.compare_compact),
            TokenAnalysisMode::Compact => format_analysis_compact(&analysis, cli.compare_compact),
        };
        eprintln!("{}", report);
    }

    // 7. Return output in requested format
    let output = match cli.format {
        OutputFormat::Toon => toon_output,
        OutputFormat::Json => json_pretty,
    };

    Ok(format!("{}\n", output))
}

/// Run directory analysis mode
fn run_directory(cli: &Cli, dir_path: &Path, max_depth: usize) -> mcp_diff::Result<String> {
    if !dir_path.exists() {
        return Err(McpDiffError::FileNotFound {
            path: dir_path.display().to_string(),
        });
    }

    if !dir_path.is_dir() {
        return Err(McpDiffError::GitError {
            message: format!("{} is not a directory", dir_path.display()),
        });
    }

    // Collect all supported files
    let files = collect_files(dir_path, max_depth, cli);

    if files.is_empty() {
        return Ok(format!("directory: {}\nfiles_found: 0\n", dir_path.display()));
    }

    if cli.verbose {
        eprintln!("Found {} files to analyze", files.len());
    }

    let mut output = String::new();
    let mut all_toon_output = String::new();
    let mut all_source_len = 0usize;
    let mut stats = DirectoryStats::default();

    for file_path in &files {
        let relative_path = file_path
            .strip_prefix(dir_path)
            .unwrap_or(file_path)
            .display()
            .to_string();

        // Try to detect language
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Read and analyze file
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Skipping {}: {}", relative_path, e);
                }
                continue;
            }
        };

        all_source_len += source.len();

        let summary = match parse_and_extract_string(file_path, &source, lang) {
            Ok(s) => s,
            Err(e) => {
                if cli.verbose {
                    eprintln!("Failed to analyze {}: {}", relative_path, e);
                }
                stats.failed += 1;
                continue;
            }
        };

        // Update stats
        stats.total += 1;
        stats.total_lines += source.lines().count();
        stats.total_control_flow += summary.control_flow_changes.len();
        stats.total_dependencies += summary.added_dependencies.len();
        stats.total_calls += summary.calls.len();

        match summary.behavioral_risk {
            mcp_diff::RiskLevel::High => stats.high_risk += 1,
            mcp_diff::RiskLevel::Medium => stats.medium_risk += 1,
            mcp_diff::RiskLevel::Low => stats.low_risk += 1,
        }

        // Track by language
        *stats.by_language.entry(lang.name().to_string()).or_insert(0) += 1;

        // Output file analysis (unless summary-only) - pure TOON format
        if !cli.summary_only {
            let toon = encode_toon(&summary);
            output.push_str("---\n");  // TOON record separator
            output.push_str(&toon);
            all_toon_output.push_str(&toon);
        }
    }

    // Summary record - pure TOON format
    output.push_str("---\n");
    output.push_str("_type: summary\n");
    output.push_str(&format!("directory: {}\n", dir_path.display()));
    output.push_str(&format!("files_analyzed: {}\n", stats.total));
    if stats.failed > 0 {
        output.push_str(&format!("files_failed: {}\n", stats.failed));
    }
    output.push_str(&format!("total_lines: {}\n", stats.total_lines));
    output.push_str(&format!(
        "risk_breakdown: high:{},medium:{},low:{}\n",
        stats.high_risk, stats.medium_risk, stats.low_risk
    ));
    output.push_str(&format!("total_control_flow: {}\n", stats.total_control_flow));
    output.push_str(&format!("total_dependencies: {}\n", stats.total_dependencies));
    output.push_str(&format!("total_calls: {}\n", stats.total_calls));

    // Language breakdown
    if !stats.by_language.is_empty() {
        let lang_summary: Vec<String> = stats
            .by_language
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect();
        output.push_str(&format!("by_language: {}\n", lang_summary.join(",")));
    }

    // Run token analysis if requested
    if let Some(mode) = cli.analyze_tokens {
        // Calculate compression ratio: source chars vs TOON chars
        let toon_len = all_toon_output.len();
        let compression_ratio = if all_source_len > 0 {
            ((all_source_len as f64 - toon_len as f64) / all_source_len as f64) * 100.0
        } else {
            0.0
        };

        let report = match mode {
            TokenAnalysisMode::Full => format!(
                "=== Token Analysis (Directory) ===\n\
                 source_chars: {}\n\
                 toon_chars: {}\n\
                 compression: {:.1}%\n\
                 files: {}\n\
                 ================================",
                all_source_len, toon_len, compression_ratio, stats.total
            ),
            TokenAnalysisMode::Compact => format!(
                "compression: {:.1}% ({} source → {} toon, {} files)",
                compression_ratio, all_source_len, toon_len, stats.total
            ),
        };
        eprintln!("{}", report);
    }

    Ok(output)
}

/// Recursively collect supported files from a directory
fn collect_files(dir: &Path, max_depth: usize, cli: &Cli) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, max_depth, 0, cli, &mut files);
    files
}

fn collect_files_recursive(
    dir: &Path,
    max_depth: usize,
    current_depth: usize,
    cli: &Cli,
    files: &mut Vec<PathBuf>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories and common non-source directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
                || name == ".next"
                || name == "coverage"
                || name == "__pycache__"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, current_depth + 1, cli, files);
        } else if path.is_file() {
            // Check if it's a supported extension
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Check extension filter
                if !cli.should_process_extension(ext) {
                    continue;
                }

                // Check if language is supported
                if Lang::from_extension(ext).is_ok() {
                    files.push(path);
                }
            }
        }
    }
}

/// Statistics for directory analysis
#[derive(Default)]
struct DirectoryStats {
    total: usize,
    failed: usize,
    total_lines: usize,
    total_control_flow: usize,
    total_dependencies: usize,
    total_calls: usize,
    high_risk: usize,
    medium_risk: usize,
    low_risk: usize,
    by_language: std::collections::HashMap<String, usize>,
}

/// Run diff branch analysis (Phase 2 mode)
fn run_diff_branch(cli: &Cli, base_ref: &str) -> mcp_diff::Result<String> {
    // Verify we're in a git repo
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root = get_repo_root(None)?;

    // Find the merge base for accurate diff
    let merge_base = get_merge_base(base_ref, "HEAD", None).unwrap_or_else(|_| base_ref.to_string());

    if cli.verbose {
        eprintln!("Base ref: {} (merge-base: {})", base_ref, merge_base);
    }

    // Get changed files
    let changed_files = get_changed_files(&merge_base, "HEAD", None)?;

    if changed_files.is_empty() {
        return Ok("No files changed.\n".to_string());
    }

    if cli.verbose {
        eprintln!("Found {} changed files", changed_files.len());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  DIFF: {} → HEAD ({} files)\n",
        base_ref,
        changed_files.len()
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    let mut total_stats = DiffStats::default();

    for changed_file in &changed_files {
        // Check extension filter
        if let Some(ext) = Path::new(&changed_file.path).extension() {
            if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                continue;
            }
        }

        let file_output = process_changed_file(cli, &repo_root, changed_file, &merge_base)?;
        if let Some((summary_output, stats)) = file_output {
            output.push_str(&summary_output);
            total_stats.merge(&stats);
        }
    }

    // Summary statistics
    if !cli.summary_only {
        output.push_str(&format!(
            "\n═══════════════════════════════════════════════════════\n"
        ));
        output.push_str(&format!("  SUMMARY\n"));
        output.push_str(&format!(
            "═══════════════════════════════════════════════════════\n"
        ));
    }
    output.push_str(&format!(
        "files: {} added, {} modified, {} deleted\n",
        total_stats.added, total_stats.modified, total_stats.deleted
    ));
    output.push_str(&format!(
        "risk: {} high, {} medium, {} low\n",
        total_stats.high_risk, total_stats.medium_risk, total_stats.low_risk
    ));

    Ok(output)
}

/// Run single commit analysis
fn run_single_commit(cli: &Cli, sha: &str) -> mcp_diff::Result<String> {
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let repo_root = get_repo_root(None)?;
    let changed_files = get_commit_changed_files(sha, None)?;

    if changed_files.is_empty() {
        return Ok(format!("No files changed in commit {}.\n", sha));
    }

    let parent = mcp_diff::git::get_parent_commit(sha, None)?;

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  COMMIT: {} ({} files)\n",
        &sha[..7.min(sha.len())],
        changed_files.len()
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    for changed_file in &changed_files {
        if let Some(ext) = Path::new(&changed_file.path).extension() {
            if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                continue;
            }
        }

        let file_output = process_changed_file(cli, &repo_root, changed_file, &parent)?;
        if let Some((summary_output, _)) = file_output {
            output.push_str(&summary_output);
        }
    }

    Ok(output)
}

/// Run all commits analysis since base
fn run_all_commits(cli: &Cli, base_ref: &str) -> mcp_diff::Result<String> {
    if !is_git_repo(None) {
        return Err(McpDiffError::NotGitRepo);
    }

    let merge_base = get_merge_base(base_ref, "HEAD", None).unwrap_or_else(|_| base_ref.to_string());
    let commits = get_commits_since(&merge_base, None)?;

    if commits.is_empty() {
        return Ok(format!("No commits since {}.\n", base_ref));
    }

    let mut output = String::new();
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n"
    ));
    output.push_str(&format!(
        "  {} COMMITS since {}\n",
        commits.len(),
        base_ref
    ));
    output.push_str(&format!(
        "═══════════════════════════════════════════════════════\n\n"
    ));

    for commit in commits {
        output.push_str(&format!(
            "───────────────────────────────────────────────────────\n"
        ));
        output.push_str(&format!(
            "{} {}\n",
            commit.short_sha, commit.subject
        ));
        output.push_str(&format!("by {} on {}\n", commit.author, commit.date));
        output.push_str(&format!(
            "───────────────────────────────────────────────────────\n"
        ));

        // Get files changed in this commit
        let changed_files = get_commit_changed_files(&commit.sha, None)?;
        let parent = mcp_diff::git::get_parent_commit(&commit.sha, None)
            .unwrap_or_else(|_| commit.sha.clone());
        let repo_root = get_repo_root(None)?;

        for changed_file in &changed_files {
            if let Some(ext) = Path::new(&changed_file.path).extension() {
                if !cli.should_process_extension(ext.to_str().unwrap_or("")) {
                    continue;
                }
            }

            if let Some((summary_output, _)) =
                process_changed_file(cli, &repo_root, changed_file, &parent)?
            {
                output.push_str(&summary_output);
            }
        }
        output.push('\n');
    }

    Ok(output)
}

/// Process a single changed file and return its summary
fn process_changed_file(
    cli: &Cli,
    repo_root: &str,
    changed_file: &ChangedFile,
    base_ref: &str,
) -> mcp_diff::Result<Option<(String, DiffStats)>> {
    let full_path = PathBuf::from(repo_root).join(&changed_file.path);
    let mut stats = DiffStats::default();

    // Update stats
    match changed_file.change_type {
        ChangeType::Added => stats.added += 1,
        ChangeType::Modified => stats.modified += 1,
        ChangeType::Deleted => stats.deleted += 1,
        ChangeType::Renamed => stats.modified += 1,
        _ => {}
    }

    let mut output = String::new();

    // Header with change type
    output.push_str(&format!(
        "━━━ {} [{}] ━━━\n",
        changed_file.path,
        changed_file.change_type.as_str()
    ));

    // Skip deleted files (can't analyze)
    if changed_file.change_type == ChangeType::Deleted {
        output.push_str("(file deleted)\n\n");
        return Ok(Some((output, stats)));
    }

    // Try to detect language
    let lang = match Lang::from_path(&full_path) {
        Ok(l) => l,
        Err(_) => {
            if cli.verbose {
                output.push_str("(unsupported language, skipping)\n\n");
            }
            return Ok(Some((output, stats)));
        }
    };

    // Read current file content
    let current_source = match fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            output.push_str(&format!("(could not read: {})\n\n", e));
            return Ok(Some((output, stats)));
        }
    };

    // Parse and extract current version
    let current_summary = match parse_and_extract(&full_path, &current_source, lang, cli) {
        Ok(s) => s,
        Err(e) => {
            output.push_str(&format!("(extraction failed: {})\n\n", e));
            return Ok(Some((output, stats)));
        }
    };

    // Update risk stats
    match current_summary.behavioral_risk {
        mcp_diff::RiskLevel::High => stats.high_risk += 1,
        mcp_diff::RiskLevel::Medium => stats.medium_risk += 1,
        mcp_diff::RiskLevel::Low => stats.low_risk += 1,
    }

    // For added files, just show the current state
    if changed_file.change_type == ChangeType::Added {
        let toon = encode_toon(&current_summary);
        output.push_str(&toon);
        output.push_str("\n\n");
        return Ok(Some((output, stats)));
    }

    // For modified files, try to get the base version for comparison
    let base_source = get_file_at_ref(&changed_file.path, base_ref, None)?;

    if let Some(base_src) = base_source {
        // Parse base version
        if let Ok(base_summary) = parse_and_extract_string(&full_path, &base_src, lang) {
            // Show diff summary
            output.push_str(&format_diff_summary(&base_summary, &current_summary));
        } else {
            // Just show current if base fails to parse
            let toon = encode_toon(&current_summary);
            output.push_str(&toon);
        }
    } else {
        // No base version, show current
        let toon = encode_toon(&current_summary);
        output.push_str(&toon);
    }

    output.push_str("\n\n");
    Ok(Some((output, stats)))
}

/// Parse source and extract semantic summary
fn parse_and_extract(
    file_path: &Path,
    source: &str,
    lang: Lang,
    cli: &Cli,
) -> mcp_diff::Result<SemanticSummary> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language: {:?}", e),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: "Failed to parse file".to_string(),
        })?;

    if cli.verbose {
        eprintln!("Parsed AST with {} nodes", count_nodes(&tree.root_node()));
    }

    if cli.print_ast {
        eprintln!("\n=== AST ===");
        print_ast(&tree.root_node(), source, 0);
        eprintln!("=== END AST ===\n");
    }

    extract(file_path, source, &tree, lang)
}

/// Parse and extract without CLI context (for base version comparison)
fn parse_and_extract_string(
    file_path: &Path,
    source: &str,
    lang: Lang,
) -> mcp_diff::Result<SemanticSummary> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| McpDiffError::ParseFailure {
            message: format!("Failed to set language: {:?}", e),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| McpDiffError::ParseFailure {
            message: "Failed to parse file".to_string(),
        })?;

    extract(file_path, source, &tree, lang)
}

/// Format a diff summary comparing base and current versions
fn format_diff_summary(base: &SemanticSummary, current: &SemanticSummary) -> String {
    let mut output = String::new();

    // Basic info
    output.push_str(&format!("symbol: {}\n", current.symbol.as_deref().unwrap_or("_")));
    output.push_str(&format!(
        "symbol_kind: {}\n",
        current
            .symbol_kind
            .map(|k| k.as_str())
            .unwrap_or("_")
    ));

    // Check for symbol changes
    if base.symbol != current.symbol {
        output.push_str(&format!(
            "CHANGED: symbol {} → {}\n",
            base.symbol.as_deref().unwrap_or("_"),
            current.symbol.as_deref().unwrap_or("_")
        ));
    }

    // Dependencies diff
    let added_deps: Vec<_> = current
        .added_dependencies
        .iter()
        .filter(|d| !base.added_dependencies.contains(d))
        .collect();
    let removed_deps: Vec<_> = base
        .added_dependencies
        .iter()
        .filter(|d| !current.added_dependencies.contains(d))
        .collect();

    if !added_deps.is_empty() {
        output.push_str(&format!("deps_added[{}]: {}\n", added_deps.len(), added_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }
    if !removed_deps.is_empty() {
        output.push_str(&format!("deps_removed[{}]: {}\n", removed_deps.len(), removed_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }

    // State changes diff
    let base_state_names: Vec<_> = base.state_changes.iter().map(|s| &s.name).collect();
    let current_state_names: Vec<_> = current.state_changes.iter().map(|s| &s.name).collect();

    let added_states: Vec<_> = current_state_names
        .iter()
        .filter(|n| !base_state_names.contains(n))
        .collect();
    let removed_states: Vec<_> = base_state_names
        .iter()
        .filter(|n| !current_state_names.contains(n))
        .collect();

    if !added_states.is_empty() {
        output.push_str(&format!("state_added[{}]: {}\n", added_states.len(), added_states.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }
    if !removed_states.is_empty() {
        output.push_str(&format!("state_removed[{}]: {}\n", removed_states.len(), removed_states.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")));
    }

    // Control flow diff
    let cf_base = base.control_flow_changes.len();
    let cf_current = current.control_flow_changes.len();
    if cf_base != cf_current {
        output.push_str(&format!(
            "control_flow: {} → {} ({:+})\n",
            cf_base,
            cf_current,
            cf_current as i64 - cf_base as i64
        ));
    }

    // Risk level
    output.push_str(&format!("behavioral_risk: {}\n", current.behavioral_risk.as_str()));
    if base.behavioral_risk != current.behavioral_risk {
        output.push_str(&format!(
            "CHANGED: risk {} → {}\n",
            base.behavioral_risk.as_str(),
            current.behavioral_risk.as_str()
        ));
    }

    output
}

/// Statistics for diff output
#[derive(Default)]
struct DiffStats {
    added: usize,
    modified: usize,
    deleted: usize,
    high_risk: usize,
    medium_risk: usize,
    low_risk: usize,
}

impl DiffStats {
    fn merge(&mut self, other: &DiffStats) {
        self.added += other.added;
        self.modified += other.modified;
        self.deleted += other.deleted;
        self.high_risk += other.high_risk;
        self.medium_risk += other.medium_risk;
        self.low_risk += other.low_risk;
    }
}

/// Count total nodes in the AST
fn count_nodes(node: &tree_sitter::Node) -> usize {
    let mut count = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_nodes(&child);
    }
    count
}

/// Print AST for debugging
fn print_ast(node: &tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = node
        .utf8_text(source.as_bytes())
        .unwrap_or("<invalid utf8>");
    let text_preview: String = text.chars().take(50).collect();
    let text_preview = text_preview.replace('\n', "\\n");

    eprintln!(
        "{}{}:{} [{}-{}] \"{}\"{}",
        indent,
        node.kind(),
        if node.is_named() { "" } else { " (anonymous)" },
        node.start_position().row,
        node.end_position().row,
        text_preview,
        if text.len() > 50 { "..." } else { "" }
    );

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_ast(&child, source, depth + 1);
    }
}
