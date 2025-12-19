//! CVE Pattern Compiler
//!
//! This module provides tools to compile vulnerability patterns from:
//! - GitHub Security Advisories (GHSA)
//! - NVD API (metadata enrichment)
//! - Manual curation
//!
//! The compiler is used offline to build the pattern database that gets
//! embedded in the semfora-engine binary.

pub mod commit_parser;
pub mod fingerprinter;
pub mod ghsa;
pub mod nvd;

use crate::error::Result;
use crate::lang::Lang;
use crate::security::{CVEPattern, PatternDatabase, PatternSource, Severity};
use std::path::Path;

/// Configuration for the pattern compiler
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// GitHub API token (optional, increases rate limits)
    pub github_token: Option<String>,

    /// NVD API key (optional, increases rate limits)
    pub nvd_api_key: Option<String>,

    /// CWE categories to fetch
    pub cwe_categories: Vec<String>,

    /// Minimum CVSS score to include (0.0-10.0)
    pub min_cvss: f32,

    /// Maximum patterns per CWE category
    pub max_patterns_per_cwe: usize,

    /// Languages to include patterns for
    pub languages: Vec<Lang>,

    /// Include KEV (Known Exploited Vulnerabilities) only
    pub kev_only: bool,
}

/// Statistics from pattern compilation
#[derive(Debug, Default)]
pub struct CompileStats {
    /// Total advisories fetched
    pub total_advisories: usize,
    /// Advisories processed
    pub processed: usize,
    /// Skipped due to missing CVE ID
    pub skipped_no_cve: usize,
    /// Skipped due to missing fix commit
    pub skipped_no_commit: usize,
    /// Skipped due to low CVSS score
    pub skipped_low_cvss: usize,
    /// Patterns successfully generated
    pub patterns_generated: usize,
    /// Errors during processing
    pub errors: usize,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            github_token: std::env::var("GITHUB_TOKEN").ok(),
            nvd_api_key: std::env::var("NVD_API_KEY").ok(),
            cwe_categories: vec![
                "CWE-89".into(),  // SQL Injection
                "CWE-79".into(),  // XSS
                "CWE-78".into(),  // OS Command Injection
                "CWE-502".into(), // Deserialization
                "CWE-287".into(), // Auth Bypass
                "CWE-22".into(),  // Path Traversal
                "CWE-94".into(),  // Code Injection
                "CWE-918".into(), // SSRF
                "CWE-611".into(), // XXE
                "CWE-434".into(), // Unrestricted Upload
            ],
            min_cvss: 7.0, // HIGH and CRITICAL only
            max_patterns_per_cwe: 100,
            languages: vec![
                Lang::JavaScript,
                Lang::TypeScript,
                Lang::Python,
                Lang::Java,
                Lang::CSharp,
                Lang::Rust,
                Lang::Go,
                Lang::Cpp,
                Lang::C,
            ],
            kev_only: false,
        }
    }
}

/// Pattern compiler - builds CVE pattern database from external sources
pub struct PatternCompiler {
    config: CompilerConfig,
    ghsa_client: ghsa::GhsaClient,
    nvd_client: nvd::NvdClient,
}

impl PatternCompiler {
    /// Create a new pattern compiler with the given configuration
    pub fn new(
        config: CompilerConfig,
        github_token: Option<String>,
        nvd_api_key: Option<String>,
    ) -> Self {
        Self {
            ghsa_client: ghsa::GhsaClient::new(github_token.or(config.github_token.clone())),
            nvd_client: nvd::NvdClient::new(nvd_api_key.or(config.nvd_api_key.clone())),
            config,
        }
    }

    /// Create a compiler with default configuration
    pub fn with_defaults() -> Self {
        let config = CompilerConfig::default();
        Self::new(config, None, None)
    }

    /// Compile patterns from all configured sources
    pub async fn compile(&self) -> Result<PatternDatabase> {
        let mut db = PatternDatabase::new();

        // Fetch from GitHub Security Advisories
        for cwe in &self.config.cwe_categories {
            let advisories = self.ghsa_client.fetch_by_cwe(cwe).await?;

            for advisory in advisories
                .into_iter()
                .take(self.config.max_patterns_per_cwe)
            {
                if let Some(pattern) = self.advisory_to_pattern(&advisory).await? {
                    if pattern.cvss_v3_score.unwrap_or(0.0) >= self.config.min_cvss {
                        db.add_pattern(pattern);
                    }
                }
            }
        }

        // Load manual patterns
        let manual_patterns = self.load_manual_patterns()?;
        for pattern in manual_patterns {
            db.add_pattern(pattern);
        }

        Ok(db)
    }

    /// Convert a GHSA advisory to a CVE pattern
    async fn advisory_to_pattern(&self, advisory: &ghsa::Advisory) -> Result<Option<CVEPattern>> {
        // Extract CVE ID from advisory
        let cve_id = match &advisory.cve_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };

        // Get NVD metadata for enrichment
        let nvd_meta = self.nvd_client.fetch_cve(&cve_id).await.ok();

        // Extract vulnerable code from fix commit
        let (fingerprints, vulnerable_calls) = if let Some(ref commit_url) = advisory.fix_commit {
            let code = commit_parser::extract_vulnerable_code(commit_url).await?;
            let fp = fingerprinter::generate_fingerprints(&code)?;
            (fp.fingerprints, fp.calls)
        } else {
            (fingerprinter::Fingerprints::default(), Vec::new())
        };

        let pattern = CVEPattern::new(
            &cve_id,
            advisory.cwes.clone(),
            0, // pattern_id
        )
        .with_fingerprints(
            fingerprints.call,
            fingerprints.control_flow,
            fingerprints.state,
        )
        .with_vulnerable_calls(vulnerable_calls)
        .with_cvss(
            nvd_meta
                .as_ref()
                .and_then(|m| m.cvss_v3_score)
                .unwrap_or(0.0),
        )
        .with_description(&advisory.summary)
        .with_languages(advisory.affected_languages.clone())
        .with_source(PatternSource::GitHubAdvisory {
            ghsa_id: advisory.ghsa_id.clone(),
            commit_sha: advisory.fix_commit.clone().unwrap_or_default(),
        })
        .with_confidence(0.85); // GHSA patterns are generally high quality

        Ok(Some(pattern))
    }

    /// Load manually curated patterns from the patterns/manual directory
    fn load_manual_patterns(&self) -> Result<Vec<CVEPattern>> {
        // This will be filled in with patterns from src/security/patterns/manual/
        Ok(super::patterns::manual::all_patterns())
    }

    /// Compile patterns from GitHub Security Advisories only
    pub async fn compile_from_ghsa(&self) -> Result<Vec<CVEPattern>> {
        self.compile_from_ghsa_with_options(false).await
    }

    /// Compile patterns from GHSA with options
    /// If `search_commits` is true, will search GitHub for fix commits when not in advisory
    pub async fn compile_from_ghsa_with_options(
        &self,
        search_commits: bool,
    ) -> Result<Vec<CVEPattern>> {
        let mut patterns = Vec::new();
        let mut stats = CompileStats::default();

        for cwe in &self.config.cwe_categories {
            tracing::info!("Fetching advisories for {}...", cwe);

            let advisories = if search_commits {
                self.ghsa_client.fetch_by_cwe_with_commits(cwe).await?
            } else {
                self.ghsa_client.fetch_by_cwe(cwe).await?
            };

            stats.total_advisories += advisories.len();

            for advisory in advisories
                .into_iter()
                .take(self.config.max_patterns_per_cwe)
            {
                stats.processed += 1;

                // Skip if no CVE ID
                if advisory.cve_id.is_none() {
                    stats.skipped_no_cve += 1;
                    continue;
                }

                // Skip if no fix commit and we care about that
                if advisory.fix_commit.is_none() {
                    stats.skipped_no_commit += 1;
                    // Still generate a pattern, but with empty fingerprints
                    // Manual review can add fingerprints later
                }

                match self.advisory_to_pattern(&advisory).await {
                    Ok(Some(pattern)) => {
                        if pattern.cvss_v3_score.unwrap_or(0.0) >= self.config.min_cvss {
                            stats.patterns_generated += 1;
                            patterns.push(pattern);
                        } else {
                            stats.skipped_low_cvss += 1;
                        }
                    }
                    Ok(None) => stats.skipped_no_cve += 1,
                    Err(e) => {
                        tracing::debug!("Failed to process advisory {}: {}", advisory.ghsa_id, e);
                        stats.errors += 1;
                    }
                }
            }
        }

        tracing::info!("GHSA compilation stats: {:?}", stats);
        Ok(patterns)
    }

    /// Enrich a pattern with NVD metadata
    pub async fn enrich_with_nvd(&self, pattern: &mut CVEPattern) -> Result<()> {
        if let Ok(meta) = self.nvd_client.fetch_cve(&pattern.cve_id).await {
            if let Some(score) = meta.cvss_v3_score {
                pattern.cvss_v3_score = Some(score);
                pattern.severity = Severity::from_cvss(score);
            }

            // Merge CWE IDs from NVD
            for cwe in meta.cwes {
                if !pattern.cwe_ids.contains(&cwe) {
                    pattern.cwe_ids.push(cwe);
                }
            }
        }
        Ok(())
    }

    /// Save compiled database to a file
    pub fn save_to_file(&self, db: &PatternDatabase, path: &Path) -> Result<()> {
        let bytes = db.to_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CompilerConfig::default();
        assert!(!config.cwe_categories.is_empty());
        assert!(config.min_cvss >= 0.0);
        assert!(!config.languages.is_empty());
    }
}
