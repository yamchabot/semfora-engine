//! Benchmark utilities for measuring token efficiency
//!
//! Provides tools to compare semantic summaries vs raw file reads
//! for proving the token efficiency of the semantic approach.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::{encode_toon, extract, generate_repo_overview, Lang};

/// Approximate token count from text
/// Uses the ~4 chars per token heuristic (accurate within 10-20% for code)
pub fn estimate_tokens(text: &str) -> usize {
    // More accurate estimation:
    // - Whitespace-heavy code: ~5 chars/token
    // - Dense code: ~3.5 chars/token
    // - Average: ~4 chars/token
    (text.len() as f64 / 3.8).ceil() as usize
}

/// Detailed token breakdown for a file
#[derive(Debug, Clone, Default)]
pub struct TokenMetrics {
    /// Path to the file
    pub file: String,

    /// Raw source size in bytes
    pub source_bytes: usize,

    /// Estimated tokens if raw source was sent
    pub source_tokens: usize,

    /// Semantic TOON output size in bytes
    pub toon_bytes: usize,

    /// Estimated tokens for TOON output
    pub toon_tokens: usize,

    /// Compression ratio (1 - toon/source)
    pub compression_ratio: f64,

    /// Token savings ratio
    pub token_savings: f64,
}

impl TokenMetrics {
    pub fn new(file: &str, source: &str, toon: &str) -> Self {
        let source_bytes = source.len();
        let source_tokens = estimate_tokens(source);
        let toon_bytes = toon.len();
        let toon_tokens = estimate_tokens(toon);

        let compression_ratio = if source_bytes > 0 {
            1.0 - (toon_bytes as f64 / source_bytes as f64)
        } else {
            0.0
        };

        let token_savings = if source_tokens > 0 {
            1.0 - (toon_tokens as f64 / source_tokens as f64)
        } else {
            0.0
        };

        Self {
            file: file.to_string(),
            source_bytes,
            source_tokens,
            toon_bytes,
            toon_tokens,
            compression_ratio,
            token_savings,
        }
    }
}

/// Aggregate metrics for a repository
#[derive(Debug, Clone, Default)]
pub struct RepoTokenMetrics {
    /// Individual file metrics
    pub files: Vec<TokenMetrics>,

    /// Total source bytes
    pub total_source_bytes: usize,

    /// Total source tokens (if all files were read)
    pub total_source_tokens: usize,

    /// Total TOON bytes
    pub total_toon_bytes: usize,

    /// Total TOON tokens
    pub total_toon_tokens: usize,

    /// Overview tokens (repo_overview.toon)
    pub overview_tokens: usize,

    /// Total compression ratio (1 - toon_bytes/source_bytes)
    pub total_compression: f64,

    /// Total token savings ratio (1 - toon_tokens/source_tokens)
    pub total_token_savings: f64,

    /// Estimated re-reads in typical workflow
    pub estimated_reread_factor: usize,

    /// Estimated tokens without semantic (source * reread factor)
    pub estimated_raw_workflow_tokens: usize,

    /// Estimated tokens with semantic (overview + modules on demand)
    pub estimated_semantic_workflow_tokens: usize,
}

impl RepoTokenMetrics {
    /// Calculate aggregate metrics from file metrics
    pub fn from_files(files: Vec<TokenMetrics>, overview_toon: &str) -> Self {
        let total_source_bytes: usize = files.iter().map(|f| f.source_bytes).sum();
        let total_source_tokens: usize = files.iter().map(|f| f.source_tokens).sum();
        let total_toon_bytes: usize = files.iter().map(|f| f.toon_bytes).sum();
        let total_toon_tokens: usize = files.iter().map(|f| f.toon_tokens).sum();
        let overview_tokens = estimate_tokens(overview_toon);

        // Use total-based compression (not per-file average) for accurate results
        let total_compression = if total_source_bytes > 0 {
            1.0 - (total_toon_bytes as f64 / total_source_bytes as f64)
        } else {
            0.0
        };

        let total_token_savings = if total_source_tokens > 0 {
            1.0 - (total_toon_tokens as f64 / total_source_tokens as f64)
        } else {
            0.0
        };

        // Typical workflow re-reads files 3-5 times during exploration
        let estimated_reread_factor = 4;
        let estimated_raw_workflow_tokens = total_source_tokens * estimated_reread_factor;

        // Semantic workflow: overview once + ~30% of modules on demand
        let estimated_semantic_workflow_tokens =
            overview_tokens + (total_toon_tokens as f64 * 0.3) as usize;

        Self {
            files,
            total_source_bytes,
            total_source_tokens,
            total_toon_bytes,
            total_toon_tokens,
            overview_tokens,
            total_compression,
            total_token_savings,
            estimated_reread_factor,
            estimated_raw_workflow_tokens,
            estimated_semantic_workflow_tokens,
        }
    }

    /// Generate a human-readable report
    pub fn report(&self) -> String {
        let mut output = String::new();

        output.push_str("═══════════════════════════════════════════════════════\n");
        output.push_str("  TOKEN EFFICIENCY BENCHMARK\n");
        output.push_str("═══════════════════════════════════════════════════════\n\n");

        output.push_str("RAW SOURCE ANALYSIS:\n");
        output.push_str(&format!("  Files analyzed:     {}\n", self.files.len()));
        output.push_str(&format!(
            "  Total source:       {} bytes\n",
            self.total_source_bytes
        ));
        output.push_str(&format!(
            "  Est. source tokens: {} tokens\n",
            self.total_source_tokens
        ));
        output.push('\n');

        output.push_str("SEMANTIC SUMMARY ANALYSIS:\n");
        output.push_str(&format!(
            "  Total TOON:         {} bytes\n",
            self.total_toon_bytes
        ));
        output.push_str(&format!(
            "  Est. TOON tokens:   {} tokens\n",
            self.total_toon_tokens
        ));
        output.push_str(&format!(
            "  Overview tokens:    {} tokens\n",
            self.overview_tokens
        ));
        output.push('\n');

        output.push_str("COMPRESSION:\n");
        output.push_str(&format!(
            "  Byte compression:   {:.1}%\n",
            self.total_compression * 100.0
        ));
        output.push_str(&format!(
            "  Token savings:      {:.1}%\n",
            self.total_token_savings * 100.0
        ));
        output.push('\n');

        output.push_str("WORKFLOW COMPARISON (estimated):\n");
        output.push_str(&format!(
            "  Re-read factor:     {}x (typical exploration)\n",
            self.estimated_reread_factor
        ));
        output.push_str(&format!(
            "  Raw file workflow:  {} tokens\n",
            self.estimated_raw_workflow_tokens
        ));
        output.push_str(&format!(
            "  Semantic workflow:  {} tokens\n",
            self.estimated_semantic_workflow_tokens
        ));

        let efficiency = if self.estimated_semantic_workflow_tokens > 0 {
            self.estimated_raw_workflow_tokens as f64
                / self.estimated_semantic_workflow_tokens as f64
        } else {
            0.0
        };
        output.push_str(&format!(
            "  Efficiency gain:    {:.1}x fewer tokens\n",
            efficiency
        ));
        output.push('\n');

        // Top 5 largest token savings
        let mut sorted_files = self.files.clone();
        sorted_files.sort_by(|a, b| {
            let a_saved = a.source_tokens.saturating_sub(a.toon_tokens);
            let b_saved = b.source_tokens.saturating_sub(b.toon_tokens);
            b_saved.cmp(&a_saved)
        });

        output.push_str("TOP 5 FILES BY TOKEN SAVINGS:\n");
        for (i, f) in sorted_files.iter().take(5).enumerate() {
            let saved = f.source_tokens.saturating_sub(f.toon_tokens);
            output.push_str(&format!(
                "  {}. {} ({} → {} tokens, saved {})\n",
                i + 1,
                f.file,
                f.source_tokens,
                f.toon_tokens,
                saved
            ));
        }

        output
    }
}

/// Analyze a repository and generate token metrics
pub fn analyze_repo_tokens(dir_path: &Path) -> Result<RepoTokenMetrics> {
    let files = collect_source_files(dir_path, 10)?;
    let mut file_metrics = Vec::new();
    let mut summaries = Vec::new();

    for file_path in &files {
        let lang = match Lang::from_path(file_path) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Parse and extract
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&lang.tree_sitter_language()).is_err() {
            continue;
        }

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => continue,
        };

        let summary = match extract(file_path, &source, &tree, lang) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let toon = encode_toon(&summary);
        let relative_path = file_path
            .strip_prefix(dir_path)
            .unwrap_or(file_path)
            .display()
            .to_string();

        file_metrics.push(TokenMetrics::new(&relative_path, &source, &toon));
        summaries.push(summary);
    }

    // Generate overview
    let dir_str = dir_path.display().to_string();
    let overview = generate_repo_overview(&summaries, &dir_str);
    let overview_toon = crate::encode_toon_directory(&overview, &[]);

    Ok(RepoTokenMetrics::from_files(file_metrics, &overview_toon))
}

/// Collect source files from a directory
fn collect_source_files(dir: &Path, max_depth: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(dir, max_depth, 0, &mut files);
    Ok(files)
}

fn collect_files_recursive(dir: &Path, max_depth: usize, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "dist"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(&path, max_depth, depth + 1, files);
        } else if path.is_file() {
            if Lang::from_path(&path).is_ok() {
                files.push(path);
            }
        }
    }
}

/// Task-based benchmark tracking
/// Tracks actual files/queries needed for a specific task
#[derive(Debug, Clone, Default)]
pub struct TaskBenchmark {
    pub task_name: String,

    /// Semantic queries made
    pub semantic_queries: Vec<SemanticQuery>,

    /// Raw files that would have been read (estimated)
    pub estimated_raw_reads: Vec<RawFileRead>,

    /// Files actually read for editing
    pub files_read_for_edit: Vec<RawFileRead>,
}

#[derive(Debug, Clone)]
pub struct SemanticQuery {
    pub query_type: String, // "repo_overview", "module", "symbol", "call_graph"
    pub target: String,     // module name, symbol hash, etc.
    pub tokens: usize,
}

#[derive(Debug, Clone)]
pub struct RawFileRead {
    pub file: String,
    pub bytes: usize,
    pub tokens: usize,
    pub reason: String, // "exploration", "edit", "context"
}

impl TaskBenchmark {
    pub fn new(task_name: &str) -> Self {
        Self {
            task_name: task_name.to_string(),
            ..Default::default()
        }
    }

    pub fn add_semantic_query(&mut self, query_type: &str, target: &str, output: &str) {
        self.semantic_queries.push(SemanticQuery {
            query_type: query_type.to_string(),
            target: target.to_string(),
            tokens: estimate_tokens(output),
        });
    }

    pub fn add_raw_read(&mut self, file: &str, content: &str, reason: &str) {
        self.files_read_for_edit.push(RawFileRead {
            file: file.to_string(),
            bytes: content.len(),
            tokens: estimate_tokens(content),
            reason: reason.to_string(),
        });
    }

    pub fn estimate_raw_exploration(&mut self, files: &[(String, usize)]) {
        for (file, tokens) in files {
            self.estimated_raw_reads.push(RawFileRead {
                file: file.clone(),
                bytes: tokens * 4, // rough estimate
                tokens: *tokens,
                reason: "exploration".to_string(),
            });
        }
    }

    /// Calculate totals and generate comparison
    pub fn report(&self) -> String {
        let semantic_tokens: usize = self.semantic_queries.iter().map(|q| q.tokens).sum();
        let edit_tokens: usize = self.files_read_for_edit.iter().map(|r| r.tokens).sum();
        let semantic_total = semantic_tokens + edit_tokens;

        let raw_exploration: usize = self.estimated_raw_reads.iter().map(|r| r.tokens).sum();
        // Assume 2x re-reads for raw exploration
        let raw_total = (raw_exploration * 2) + edit_tokens;

        let mut output = String::new();
        output.push_str("═══════════════════════════════════════════════════════\n");
        output.push_str(&format!("  TASK BENCHMARK: {}\n", self.task_name));
        output.push_str("═══════════════════════════════════════════════════════\n\n");

        output.push_str("SEMANTIC PATH:\n");
        for q in &self.semantic_queries {
            output.push_str(&format!(
                "  {} ({}) → {} tokens\n",
                q.query_type, q.target, q.tokens
            ));
        }
        for r in &self.files_read_for_edit {
            output.push_str(&format!(
                "  Read {} ({}) → {} tokens\n",
                r.file, r.reason, r.tokens
            ));
        }
        output.push_str(&format!("  TOTAL: {} tokens\n\n", semantic_total));

        output.push_str("ESTIMATED RAW PATH:\n");
        for r in &self.estimated_raw_reads {
            output.push_str(&format!(
                "  Read {} ({}) → {} tokens\n",
                r.file, r.reason, r.tokens
            ));
        }
        output.push_str(&format!("  + 2x re-reads during exploration\n"));
        for r in &self.files_read_for_edit {
            output.push_str(&format!(
                "  Read {} ({}) → {} tokens\n",
                r.file, r.reason, r.tokens
            ));
        }
        output.push_str(&format!("  TOTAL: {} tokens\n\n", raw_total));

        let savings = if raw_total > 0 {
            ((raw_total - semantic_total) as f64 / raw_total as f64) * 100.0
        } else {
            0.0
        };

        output.push_str(&format!(
            "SAVINGS: {:.1}% ({} tokens saved)\n",
            savings,
            raw_total - semantic_total
        ));

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_benchmark_add_call_graph() {
        // Simulate the actual task we did: "Add get_call_graph MCP tool"
        let mut task = TaskBenchmark::new("Add get_call_graph MCP tool");

        // Semantic queries we made (approximate token counts from actual output)
        task.add_semantic_query("repo_overview", ".", &"a".repeat(900)); // ~237 tokens
        task.add_semantic_query("module", "mcp_server", &"a".repeat(3800)); // ~1000 tokens
        task.add_semantic_query("module", "cache", &"a".repeat(2280)); // ~600 tokens
        task.add_semantic_query("symbol", "4135d4e7f42a3501", &"a".repeat(1520)); // ~400 tokens

        // Files we actually read for editing
        task.add_raw_read("src/cache.rs", &"a".repeat(52100), "edit"); // ~13k tokens
        task.add_raw_read("src/mcp_server/mod.rs", &"a".repeat(91200), "edit"); // ~24k tokens

        // What raw exploration would have looked like
        task.estimate_raw_exploration(&[
            ("src/lib.rs".to_string(), 2200),
            ("src/schema.rs".to_string(), 8700),
            ("src/shard.rs".to_string(), 4500),
            ("src/toon.rs".to_string(), 10800),
            ("src/main.rs".to_string(), 12500),
        ]);

        let report = task.report();
        println!("{}", report);

        // Semantic path should be significantly cheaper
        let semantic_tokens: usize = task.semantic_queries.iter().map(|q| q.tokens).sum();
        let raw_tokens: usize = task.estimated_raw_reads.iter().map(|r| r.tokens).sum();

        assert!(
            semantic_tokens < raw_tokens,
            "Semantic should use fewer tokens than raw exploration"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        // Roughly 4 chars per token
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("") == 0);

        // 100 chars should be ~25-30 tokens
        let text = "a".repeat(100);
        let tokens = estimate_tokens(&text);
        assert!(tokens >= 20 && tokens <= 35);
    }

    #[test]
    fn test_token_metrics() {
        let source = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let toon = "symbol: main\nsymbol_kind: function";

        let metrics = TokenMetrics::new("test.rs", source, toon);

        assert!(metrics.source_tokens > metrics.toon_tokens);
        assert!(metrics.compression_ratio > 0.0);
        assert!(metrics.token_savings > 0.0);
    }
}
