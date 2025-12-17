//! GitHub Security Advisory (GHSA) client
//!
//! Fetches security advisories from GitHub's GraphQL API to extract
//! vulnerable code patterns from fix commits.

use crate::error::Result;
use crate::lang::Lang;
use serde::{Deserialize, Serialize};

/// A GitHub Security Advisory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advisory {
    /// GHSA ID (e.g., "GHSA-xxxx-xxxx-xxxx")
    pub ghsa_id: String,

    /// CVE ID if assigned (e.g., "CVE-2021-44228")
    pub cve_id: Option<String>,

    /// Short summary of the vulnerability
    pub summary: String,

    /// Detailed description
    pub description: String,

    /// Severity from GitHub (CRITICAL, HIGH, MODERATE, LOW)
    pub severity: String,

    /// CWE identifiers
    pub cwes: Vec<String>,

    /// URL to the fix commit (if available)
    pub fix_commit: Option<String>,

    /// Affected package ecosystems/languages
    pub affected_languages: Vec<Lang>,

    /// Published date (ISO 8601)
    pub published_at: String,

    /// Last updated date (ISO 8601)
    pub updated_at: String,

    /// References (URLs to patches, discussions, etc.)
    pub references: Vec<String>,
}

/// Client for fetching GitHub Security Advisories
pub struct GhsaClient {
    token: Option<String>,
    client: reqwest::Client,
}

impl GhsaClient {
    /// Create a new GHSA client
    pub fn new(token: Option<String>) -> Self {
        Self {
            token,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch advisories for a specific CWE category with pagination
    pub async fn fetch_by_cwe(&self, cwe: &str) -> Result<Vec<Advisory>> {
        let mut advisories = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page = 0;
        const MAX_PAGES: usize = 10; // Safety limit: 10 pages * 100 = 1000 advisories max per CWE

        // If no token, return empty (can't access API without auth)
        if self.token.is_none() {
            tracing::warn!("No GitHub token provided, skipping GHSA fetch for {}", cwe);
            return Ok(advisories);
        }

        loop {
            page += 1;
            if page > MAX_PAGES {
                tracing::info!("Hit max pages ({}) for CWE {}, stopping", MAX_PAGES, cwe);
                break;
            }

            let after_clause = cursor
                .as_ref()
                .map(|c| format!(", after: \"{}\"", c))
                .unwrap_or_default();

            // GraphQL query with pagination - fetches advisories and filters by CWE
            let query = format!(
                r#"
                query {{
                    securityAdvisories(
                        first: 100{after}
                        orderBy: {{ field: PUBLISHED_AT, direction: DESC }}
                    ) {{
                        pageInfo {{
                            hasNextPage
                            endCursor
                        }}
                        nodes {{
                            ghsaId
                            summary
                            description
                            severity
                            publishedAt
                            updatedAt
                            cwes(first: 10) {{
                                nodes {{
                                    cweId
                                }}
                            }}
                            identifiers {{
                                type
                                value
                            }}
                            references {{
                                url
                            }}
                            vulnerabilities(first: 10) {{
                                nodes {{
                                    package {{
                                        ecosystem
                                        name
                                    }}
                                    firstPatchedVersion {{
                                        identifier
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
                "#,
                after = after_clause
            );

            let response = self
                .client
                .post("https://api.github.com/graphql")
                .header("Authorization", format!("Bearer {}", self.token.as_ref().unwrap()))
                .header("User-Agent", "semfora-security-compiler")
                .json(&serde_json::json!({ "query": query }))
                .send()
                .await?;

            if !response.status().is_success() {
                tracing::warn!("GHSA API returned {}: {}", response.status(), response.text().await?);
                break;
            }

            let data: serde_json::Value = response.json().await?;

            // Check for GraphQL errors
            if let Some(errors) = data["errors"].as_array() {
                if !errors.is_empty() {
                    tracing::warn!("GHSA GraphQL errors: {:?}", errors);
                    break;
                }
            }

            // Parse the GraphQL response
            let nodes = match data["data"]["securityAdvisories"]["nodes"].as_array() {
                Some(n) => n,
                None => break,
            };

            let mut found_matching = 0;
            for node in nodes {
                // Check if this advisory has the CWE we're looking for
                let cwes: Vec<String> = node["cwes"]["nodes"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| c["cweId"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                // Skip if doesn't match our target CWE
                if !cwes.iter().any(|c| c == cwe) {
                    continue;
                }
                found_matching += 1;

                // Extract CVE ID from identifiers
                let cve_id = node["identifiers"]
                    .as_array()
                    .and_then(|ids| {
                        ids.iter().find_map(|id| {
                            if id["type"].as_str() == Some("CVE") {
                                id["value"].as_str().map(String::from)
                            } else {
                                None
                            }
                        })
                    });

                // Extract references
                let references: Vec<String> = node["references"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|r| r["url"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                // Find fix commit URL from references (prioritize commit over PR)
                let fix_commit = references.iter()
                    .find(|r| r.contains("/commit/"))
                    .or_else(|| references.iter().find(|r| r.contains("/pull/")))
                    .cloned();

                // Determine affected languages from ecosystem
                let affected_languages = self.extract_languages(&node["vulnerabilities"]["nodes"]);

                let advisory = Advisory {
                    ghsa_id: node["ghsaId"].as_str().unwrap_or_default().to_string(),
                    cve_id,
                    summary: node["summary"].as_str().unwrap_or_default().to_string(),
                    description: node["description"].as_str().unwrap_or_default().to_string(),
                    severity: node["severity"].as_str().unwrap_or("MODERATE").to_string(),
                    cwes,
                    fix_commit,
                    affected_languages,
                    published_at: node["publishedAt"].as_str().unwrap_or_default().to_string(),
                    updated_at: node["updatedAt"].as_str().unwrap_or_default().to_string(),
                    references,
                };

                advisories.push(advisory);
            }

            tracing::debug!(
                "Page {}: {} advisories, {} matching {}",
                page, nodes.len(), found_matching, cwe
            );

            // Check if there are more pages
            let page_info = &data["data"]["securityAdvisories"]["pageInfo"];
            let has_next = page_info["hasNextPage"].as_bool().unwrap_or(false);

            if !has_next {
                break;
            }

            cursor = page_info["endCursor"].as_str().map(String::from);

            // Rate limit: 100ms between pages to be nice to GitHub
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        tracing::info!("Fetched {} advisories for {}", advisories.len(), cwe);
        Ok(advisories)
    }

    /// Extract languages from vulnerability ecosystem data
    fn extract_languages(&self, vulnerabilities: &serde_json::Value) -> Vec<Lang> {
        let mut languages = Vec::new();

        if let Some(nodes) = vulnerabilities.as_array() {
            for node in nodes {
                if let Some(ecosystem) = node["package"]["ecosystem"].as_str() {
                    match ecosystem.to_uppercase().as_str() {
                        "NPM" => {
                            if !languages.contains(&Lang::JavaScript) {
                                languages.push(Lang::JavaScript);
                            }
                            if !languages.contains(&Lang::TypeScript) {
                                languages.push(Lang::TypeScript);
                            }
                        }
                        "PIP" | "PYPI" => {
                            if !languages.contains(&Lang::Python) {
                                languages.push(Lang::Python);
                            }
                        }
                        "MAVEN" => {
                            if !languages.contains(&Lang::Java) {
                                languages.push(Lang::Java);
                            }
                        }
                        "NUGET" => {
                            if !languages.contains(&Lang::CSharp) {
                                languages.push(Lang::CSharp);
                            }
                        }
                        "CARGO" | "CRATES.IO" => {
                            if !languages.contains(&Lang::Rust) {
                                languages.push(Lang::Rust);
                            }
                        }
                        "GO" => {
                            if !languages.contains(&Lang::Go) {
                                languages.push(Lang::Go);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        languages
    }

    /// Fetch a single advisory by GHSA ID
    pub async fn fetch_advisory(&self, ghsa_id: &str) -> Result<Option<Advisory>> {
        let advisories = self.fetch_by_cwe("").await?; // This is a placeholder
        Ok(advisories.into_iter().find(|a| a.ghsa_id == ghsa_id))
    }

    /// Search GitHub for fix commits mentioning a CVE ID
    /// This helps find commits when the advisory doesn't directly link to them
    pub async fn search_fix_commits(&self, cve_id: &str) -> Result<Vec<String>> {
        if self.token.is_none() {
            return Ok(Vec::new());
        }

        // Search for commits mentioning the CVE
        let query = format!("{} fix OR patch OR security", cve_id);

        let response = self
            .client
            .get("https://api.github.com/search/commits")
            .query(&[("q", &query), ("sort", &"committer-date".to_string()), ("order", &"desc".to_string()), ("per_page", &"10".to_string())])
            .header("Authorization", format!("Bearer {}", self.token.as_ref().unwrap()))
            .header("User-Agent", "semfora-security-compiler")
            .header("Accept", "application/vnd.github.cloak-preview+json") // Required for commit search
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::debug!("Commit search failed for {}: {}", cve_id, response.status());
            return Ok(Vec::new());
        }

        let data: serde_json::Value = response.json().await?;
        let mut commit_urls = Vec::new();

        if let Some(items) = data["items"].as_array() {
            for item in items {
                if let Some(url) = item["html_url"].as_str() {
                    commit_urls.push(url.to_string());
                }
            }
        }

        Ok(commit_urls)
    }

    /// Fetch advisories with enhanced commit discovery
    /// First tries advisory references, then falls back to commit search
    pub async fn fetch_by_cwe_with_commits(&self, cwe: &str) -> Result<Vec<Advisory>> {
        let mut advisories = self.fetch_by_cwe(cwe).await?;

        // Try to find commits for advisories that don't have them
        let mut enhanced = 0;
        for advisory in &mut advisories {
            if advisory.fix_commit.is_some() {
                continue;
            }

            if let Some(cve_id) = &advisory.cve_id {
                // Rate limit: Don't hammer the API
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                match self.search_fix_commits(cve_id).await {
                    Ok(commits) => {
                        if let Some(commit_url) = commits.first() {
                            tracing::debug!(
                                "Found commit {} for {} via search",
                                commit_url, cve_id
                            );
                            advisory.fix_commit = Some(commit_url.clone());
                            enhanced += 1;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Commit search failed for {}: {}", cve_id, e);
                    }
                }
            }
        }

        if enhanced > 0 {
            tracing::info!(
                "Enhanced {} advisories with commits via search for {}",
                enhanced, cwe
            );
        }

        Ok(advisories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghsa_client_creation() {
        let client = GhsaClient::new(None);
        assert!(client.token.is_none());

        let client = GhsaClient::new(Some("test_token".into()));
        assert!(client.token.is_some());
    }
}
