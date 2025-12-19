//! NVD (National Vulnerability Database) API client
//!
//! Fetches CVE metadata from NVD for enrichment:
//! - CVSS v3 scores
//! - CWE mappings
//! - Descriptions
//! - References

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// CVE metadata from NVD
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCveMetadata {
    /// CVE ID
    pub cve_id: String,

    /// CVSS v3 base score (0.0-10.0)
    pub cvss_v3_score: Option<f32>,

    /// CVSS v3 vector string
    pub cvss_v3_vector: Option<String>,

    /// CWE identifiers
    pub cwes: Vec<String>,

    /// Description in English
    pub description: String,

    /// Reference URLs
    pub references: Vec<String>,

    /// Published date (ISO 8601)
    pub published: String,

    /// Last modified date (ISO 8601)
    pub last_modified: String,

    /// Whether this is in KEV (Known Exploited Vulnerabilities)
    pub is_kev: bool,
}

/// NVD API response structures
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NvdResponse {
    #[serde(rename = "resultsPerPage")]
    results_per_page: usize,
    #[serde(rename = "startIndex")]
    start_index: usize,
    #[serde(rename = "totalResults")]
    total_results: usize,
    vulnerabilities: Vec<NvdVulnerability>,
}

#[derive(Debug, Deserialize)]
struct NvdVulnerability {
    cve: NvdCve,
}

#[derive(Debug, Deserialize)]
struct NvdCve {
    id: String,
    descriptions: Vec<NvdDescription>,
    metrics: Option<NvdMetrics>,
    weaknesses: Option<Vec<NvdWeakness>>,
    references: Option<Vec<NvdReference>>,
    published: String,
    #[serde(rename = "lastModified")]
    last_modified: String,
    #[serde(rename = "cisaExploitAdd")]
    cisa_exploit_add: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NvdDescription {
    lang: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct NvdMetrics {
    #[serde(rename = "cvssMetricV31")]
    cvss_v31: Option<Vec<CvssV31>>,
    #[serde(rename = "cvssMetricV30")]
    cvss_v30: Option<Vec<CvssV30>>,
}

#[derive(Debug, Deserialize)]
struct CvssV31 {
    #[serde(rename = "cvssData")]
    cvss_data: CvssData,
}

#[derive(Debug, Deserialize)]
struct CvssV30 {
    #[serde(rename = "cvssData")]
    cvss_data: CvssData,
}

#[derive(Debug, Deserialize)]
struct CvssData {
    #[serde(rename = "baseScore")]
    base_score: f32,
    #[serde(rename = "vectorString")]
    vector_string: String,
}

#[derive(Debug, Deserialize)]
struct NvdWeakness {
    description: Vec<NvdDescription>,
}

#[derive(Debug, Deserialize)]
struct NvdReference {
    url: String,
}

/// Client for the NVD CVE API
pub struct NvdClient {
    api_key: Option<String>,
    client: reqwest::Client,
    base_url: String,
}

impl NvdClient {
    /// Create a new NVD client
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            base_url: "https://services.nvd.nist.gov/rest/json/cves/2.0".to_string(),
        }
    }

    /// Fetch metadata for a specific CVE
    pub async fn fetch_cve(&self, cve_id: &str) -> Result<NvdCveMetadata> {
        let url = format!("{}?cveId={}", self.base_url, cve_id);

        let mut request = self.client.get(&url);

        if let Some(ref key) = self.api_key {
            request = request.header("apiKey", key);
        }

        // Rate limiting: NVD allows 5 requests/30s without key, 50/30s with key
        tokio::time::sleep(Duration::from_millis(if self.api_key.is_some() {
            600
        } else {
            6000
        }))
        .await;

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(crate::error::McpDiffError::Generic(format!(
                "NVD API error: {} for {}",
                response.status(),
                cve_id
            )));
        }

        let nvd_response: NvdResponse = response.json().await?;

        if nvd_response.vulnerabilities.is_empty() {
            return Err(crate::error::McpDiffError::Generic(format!(
                "CVE not found: {}",
                cve_id
            )));
        }

        let cve = &nvd_response.vulnerabilities[0].cve;

        // Extract CVSS score (prefer v3.1, fall back to v3.0)
        let (cvss_v3_score, cvss_v3_vector) = cve.metrics.as_ref().map_or((None, None), |m| {
            if let Some(ref v31) = m.cvss_v31 {
                if let Some(first) = v31.first() {
                    return (
                        Some(first.cvss_data.base_score),
                        Some(first.cvss_data.vector_string.clone()),
                    );
                }
            }
            if let Some(ref v30) = m.cvss_v30 {
                if let Some(first) = v30.first() {
                    return (
                        Some(first.cvss_data.base_score),
                        Some(first.cvss_data.vector_string.clone()),
                    );
                }
            }
            (None, None)
        });

        // Extract English description
        let description = cve
            .descriptions
            .iter()
            .find(|d| d.lang == "en")
            .map(|d| d.value.clone())
            .unwrap_or_default();

        // Extract CWEs
        let cwes = cve
            .weaknesses
            .as_ref()
            .map(|ws| {
                ws.iter()
                    .flat_map(|w| {
                        w.description
                            .iter()
                            .filter(|d| d.lang == "en" && d.value.starts_with("CWE-"))
                            .map(|d| d.value.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Extract references
        let references = cve
            .references
            .as_ref()
            .map(|refs| refs.iter().map(|r| r.url.clone()).collect())
            .unwrap_or_default();

        // Check if in KEV (Known Exploited Vulnerabilities)
        let is_kev = cve.cisa_exploit_add.is_some();

        Ok(NvdCveMetadata {
            cve_id: cve.id.clone(),
            cvss_v3_score,
            cvss_v3_vector,
            cwes,
            description,
            references,
            published: cve.published.clone(),
            last_modified: cve.last_modified.clone(),
            is_kev,
        })
    }

    /// Fetch all CVEs for a specific CWE category
    pub async fn fetch_by_cwe(&self, cwe: &str) -> Result<Vec<NvdCveMetadata>> {
        let mut results = Vec::new();
        let mut start_index = 0;
        const RESULTS_PER_PAGE: usize = 2000;

        loop {
            let url = format!(
                "{}?cweId={}&startIndex={}&resultsPerPage={}",
                self.base_url, cwe, start_index, RESULTS_PER_PAGE
            );

            let mut request = self.client.get(&url);

            if let Some(ref key) = self.api_key {
                request = request.header("apiKey", key);
            }

            // Rate limiting
            tokio::time::sleep(Duration::from_millis(if self.api_key.is_some() {
                600
            } else {
                6000
            }))
            .await;

            let response = request.send().await?;

            if !response.status().is_success() {
                tracing::warn!("NVD API error for CWE {}: {}", cwe, response.status());
                break;
            }

            let nvd_response: NvdResponse = response.json().await?;

            for vuln in nvd_response.vulnerabilities {
                let cve = &vuln.cve;

                let (cvss_v3_score, cvss_v3_vector) =
                    cve.metrics.as_ref().map_or((None, None), |m| {
                        if let Some(ref v31) = m.cvss_v31 {
                            if let Some(first) = v31.first() {
                                return (
                                    Some(first.cvss_data.base_score),
                                    Some(first.cvss_data.vector_string.clone()),
                                );
                            }
                        }
                        if let Some(ref v30) = m.cvss_v30 {
                            if let Some(first) = v30.first() {
                                return (
                                    Some(first.cvss_data.base_score),
                                    Some(first.cvss_data.vector_string.clone()),
                                );
                            }
                        }
                        (None, None)
                    });

                let description = cve
                    .descriptions
                    .iter()
                    .find(|d| d.lang == "en")
                    .map(|d| d.value.clone())
                    .unwrap_or_default();

                let cwes = cve
                    .weaknesses
                    .as_ref()
                    .map(|ws| {
                        ws.iter()
                            .flat_map(|w| {
                                w.description
                                    .iter()
                                    .filter(|d| d.lang == "en" && d.value.starts_with("CWE-"))
                                    .map(|d| d.value.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let references = cve
                    .references
                    .as_ref()
                    .map(|refs| refs.iter().map(|r| r.url.clone()).collect())
                    .unwrap_or_default();

                let is_kev = cve.cisa_exploit_add.is_some();

                results.push(NvdCveMetadata {
                    cve_id: cve.id.clone(),
                    cvss_v3_score,
                    cvss_v3_vector,
                    cwes,
                    description,
                    references,
                    published: cve.published.clone(),
                    last_modified: cve.last_modified.clone(),
                    is_kev,
                });
            }

            if start_index + RESULTS_PER_PAGE >= nvd_response.total_results {
                break;
            }
            start_index += RESULTS_PER_PAGE;
        }

        Ok(results)
    }

    /// Fetch only KEV (Known Exploited Vulnerabilities)
    pub async fn fetch_kev(&self) -> Result<Vec<NvdCveMetadata>> {
        let mut results = Vec::new();
        let mut start_index = 0;
        const RESULTS_PER_PAGE: usize = 2000;

        loop {
            let url = format!(
                "{}?hasKev&startIndex={}&resultsPerPage={}",
                self.base_url, start_index, RESULTS_PER_PAGE
            );

            let mut request = self.client.get(&url);

            if let Some(ref key) = self.api_key {
                request = request.header("apiKey", key);
            }

            tokio::time::sleep(Duration::from_millis(if self.api_key.is_some() {
                600
            } else {
                6000
            }))
            .await;

            let response = request.send().await?;

            if !response.status().is_success() {
                tracing::warn!("NVD API error for KEV: {}", response.status());
                break;
            }

            let nvd_response: NvdResponse = response.json().await?;

            for vuln in nvd_response.vulnerabilities {
                // Process same as fetch_by_cwe...
                let cve = &vuln.cve;

                let (cvss_v3_score, cvss_v3_vector) =
                    cve.metrics.as_ref().map_or((None, None), |m| {
                        if let Some(ref v31) = m.cvss_v31 {
                            if let Some(first) = v31.first() {
                                return (
                                    Some(first.cvss_data.base_score),
                                    Some(first.cvss_data.vector_string.clone()),
                                );
                            }
                        }
                        (None, None)
                    });

                let description = cve
                    .descriptions
                    .iter()
                    .find(|d| d.lang == "en")
                    .map(|d| d.value.clone())
                    .unwrap_or_default();

                let cwes = cve
                    .weaknesses
                    .as_ref()
                    .map(|ws| {
                        ws.iter()
                            .flat_map(|w| {
                                w.description
                                    .iter()
                                    .filter(|d| d.lang == "en" && d.value.starts_with("CWE-"))
                                    .map(|d| d.value.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let references = cve
                    .references
                    .as_ref()
                    .map(|refs| refs.iter().map(|r| r.url.clone()).collect())
                    .unwrap_or_default();

                results.push(NvdCveMetadata {
                    cve_id: cve.id.clone(),
                    cvss_v3_score,
                    cvss_v3_vector,
                    cwes,
                    description,
                    references,
                    published: cve.published.clone(),
                    last_modified: cve.last_modified.clone(),
                    is_kev: true,
                });
            }

            if start_index + RESULTS_PER_PAGE >= nvd_response.total_results {
                break;
            }
            start_index += RESULTS_PER_PAGE;
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvd_client_creation() {
        let client = NvdClient::new(None);
        assert!(client.api_key.is_none());

        let client = NvdClient::new(Some("test_key".into()));
        assert!(client.api_key.is_some());
    }
}
