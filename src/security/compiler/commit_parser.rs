//! Commit parser for extracting vulnerable code from fix commits
//!
//! This module parses git diffs to extract the "before" state of code
//! that was fixed in a security patch.

use crate::error::Result;
use crate::lang::Lang;
use serde::{Deserialize, Serialize};

/// Extracted vulnerable code from a commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerableCode {
    /// Language of the code
    pub language: Lang,

    /// Source code before the fix
    pub source: String,

    /// File path
    pub file_path: String,

    /// Function/method names that were modified
    pub modified_functions: Vec<String>,

    /// Line range in original file
    pub start_line: u32,
    pub end_line: u32,
}

/// Parse a GitHub commit URL and extract vulnerable code
pub async fn extract_vulnerable_code(commit_url: &str) -> Result<Vec<VulnerableCode>> {
    // Parse the commit URL to get repo and SHA
    let (owner, repo, sha) = parse_commit_url(commit_url)?;

    // Fetch the diff from GitHub API
    let diff = fetch_commit_diff(&owner, &repo, &sha).await?;

    // Parse the diff to extract "removed" lines (the vulnerable code)
    let vulnerable_blocks = parse_diff_for_vulnerable_code(&diff)?;

    Ok(vulnerable_blocks)
}

/// Parse a GitHub commit URL
fn parse_commit_url(url: &str) -> Result<(String, String, String)> {
    // Handle formats:
    // https://github.com/owner/repo/commit/sha
    // https://github.com/owner/repo/pull/123/commits/sha

    let url = url.trim_end_matches('/');

    if url.contains("/commit/") {
        let parts: Vec<&str> = url.split('/').collect();
        // ..., "github.com", owner, repo, "commit", sha
        if parts.len() >= 5 {
            let owner_idx = parts.iter().position(|&p| p == "github.com").unwrap_or(0) + 1;
            if owner_idx + 3 < parts.len() {
                return Ok((
                    parts[owner_idx].to_string(),
                    parts[owner_idx + 1].to_string(),
                    parts[owner_idx + 3].to_string(),
                ));
            }
        }
    }

    if url.contains("/commits/") {
        let parts: Vec<&str> = url.split('/').collect();
        if let Some(sha) = parts.last() {
            let owner_idx = parts.iter().position(|&p| p == "github.com").unwrap_or(0) + 1;
            if owner_idx + 1 < parts.len() {
                return Ok((
                    parts[owner_idx].to_string(),
                    parts[owner_idx + 1].to_string(),
                    sha.to_string(),
                ));
            }
        }
    }

    Err(crate::error::McpDiffError::Generic(format!(
        "Could not parse commit URL: {}",
        url
    )))
}

/// Fetch commit diff from GitHub API
async fn fetch_commit_diff(owner: &str, repo: &str, sha: &str) -> Result<String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        owner, repo, sha
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "semfora-security-compiler")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(crate::error::McpDiffError::Generic(format!(
            "GitHub API error: {} for {}/{}/{}",
            response.status(),
            owner,
            repo,
            sha
        )));
    }

    Ok(response.text().await?)
}

/// Parse a unified diff to extract vulnerable code blocks
fn parse_diff_for_vulnerable_code(diff: &str) -> Result<Vec<VulnerableCode>> {
    let mut results = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_removed_lines: Vec<String> = Vec::new();
    let mut current_start_line: u32 = 0;
    let mut in_hunk = false;
    let mut line_number = 0u32;

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            // Save previous file's blocks if any
            if let Some(ref file) = current_file {
                if !current_removed_lines.is_empty() {
                    if let Some(lang) = detect_language(file) {
                        results.push(VulnerableCode {
                            language: lang,
                            source: current_removed_lines.join("\n"),
                            file_path: file.clone(),
                            modified_functions: extract_function_names(&current_removed_lines),
                            start_line: current_start_line,
                            end_line: current_start_line + current_removed_lines.len() as u32,
                        });
                    }
                }
            }

            // Parse new file path
            // diff --git a/path/to/file b/path/to/file
            if let Some(path) = line.split(" b/").last() {
                current_file = Some(path.to_string());
            }
            current_removed_lines.clear();
            in_hunk = false;
        } else if line.starts_with("@@") {
            // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
            in_hunk = true;
            if let Some(old_range) = line.split("@@").nth(1) {
                if let Some(old_start) = old_range.trim().split(' ').next() {
                    if let Some(start) = old_start.strip_prefix('-') {
                        if let Some(num) = start.split(',').next() {
                            line_number = num.parse().unwrap_or(0);
                            if current_start_line == 0 {
                                current_start_line = line_number;
                            }
                        }
                    }
                }
            }
        } else if in_hunk {
            if line.starts_with('-') && !line.starts_with("---") {
                // This is a removed line (vulnerable code)
                current_removed_lines.push(line[1..].to_string());
            }
            if !line.starts_with('+') {
                line_number += 1;
            }
        }
    }

    // Don't forget the last file
    if let Some(ref file) = current_file {
        if !current_removed_lines.is_empty() {
            if let Some(lang) = detect_language(file) {
                results.push(VulnerableCode {
                    language: lang,
                    source: current_removed_lines.join("\n"),
                    file_path: file.clone(),
                    modified_functions: extract_function_names(&current_removed_lines),
                    start_line: current_start_line,
                    end_line: current_start_line + current_removed_lines.len() as u32,
                });
            }
        }
    }

    Ok(results)
}

/// Detect language from file extension
fn detect_language(file_path: &str) -> Option<Lang> {
    let ext = file_path.rsplit('.').next()?;
    match ext.to_lowercase().as_str() {
        "js" | "mjs" | "cjs" => Some(Lang::JavaScript),
        "ts" | "mts" | "cts" => Some(Lang::TypeScript),
        "tsx" => Some(Lang::Tsx),
        "jsx" => Some(Lang::Jsx),
        "py" => Some(Lang::Python),
        "rs" => Some(Lang::Rust),
        "go" => Some(Lang::Go),
        "java" => Some(Lang::Java),
        "cs" => Some(Lang::CSharp),
        "c" => Some(Lang::C),
        "cpp" | "cc" | "cxx" => Some(Lang::Cpp),
        "h" | "hpp" => Some(Lang::Cpp),
        _ => None,
    }
}

/// Extract function/method names from code lines
fn extract_function_names(lines: &[String]) -> Vec<String> {
    let mut names = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        // JavaScript/TypeScript function patterns
        if trimmed.contains("function ") {
            if let Some(name) = extract_js_function_name(trimmed) {
                names.push(name);
            }
        }

        // Arrow function or method
        if trimmed.contains("=>") || trimmed.contains("= async") {
            if let Some(name) = extract_arrow_function_name(trimmed) {
                names.push(name);
            }
        }

        // Python def
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            if let Some(name) = extract_python_function_name(trimmed) {
                names.push(name);
            }
        }

        // Rust fn
        if trimmed.contains("fn ") {
            if let Some(name) = extract_rust_function_name(trimmed) {
                names.push(name);
            }
        }

        // Java/C# method
        if (trimmed.contains("public ") || trimmed.contains("private ") || trimmed.contains("protected "))
            && trimmed.contains("(")
        {
            if let Some(name) = extract_java_method_name(trimmed) {
                names.push(name);
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

fn extract_js_function_name(line: &str) -> Option<String> {
    // function name(...) or function name<T>(...)
    let after_fn = line.split("function ").nth(1)?;
    let name = after_fn.split(|c| c == '(' || c == '<' || c == ' ').next()?;
    if name.is_empty() || name.starts_with('(') {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_arrow_function_name(line: &str) -> Option<String> {
    // const name = async (...) => or const name = (...) =>
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() >= 2 {
        let name_part = parts[0].trim();
        // Remove const/let/var
        let name = name_part
            .replace("const ", "")
            .replace("let ", "")
            .replace("var ", "")
            .replace("export ", "")
            .trim()
            .to_string();
        if !name.is_empty() && !name.contains(' ') {
            return Some(name);
        }
    }
    None
}

fn extract_python_function_name(line: &str) -> Option<String> {
    // def name(...) or async def name(...)
    let after_def = if line.starts_with("async def ") {
        line.strip_prefix("async def ")?
    } else {
        line.strip_prefix("def ")?
    };
    let name = after_def.split('(').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_rust_function_name(line: &str) -> Option<String> {
    // fn name(...) or pub fn name(...) or async fn name(...)
    let after_fn = line.split("fn ").nth(1)?;
    let name = after_fn.split(|c| c == '(' || c == '<').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_java_method_name(line: &str) -> Option<String> {
    // public Type methodName(...) or private void methodName(...)
    let before_paren = line.split('(').next()?.trim();
    let name = before_paren.split_whitespace().last()?;
    if name.is_empty() || name.contains('<') {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_commit_url() {
        let url = "https://github.com/apache/logging-log4j2/commit/abc123";
        let (owner, repo, sha) = parse_commit_url(url).unwrap();
        assert_eq!(owner, "apache");
        assert_eq!(repo, "logging-log4j2");
        assert_eq!(sha, "abc123");
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("file.js"), Some(Lang::JavaScript));
        assert_eq!(detect_language("file.ts"), Some(Lang::TypeScript));
        assert_eq!(detect_language("file.py"), Some(Lang::Python));
        assert_eq!(detect_language("file.rs"), Some(Lang::Rust));
        assert_eq!(detect_language("file.java"), Some(Lang::Java));
        assert_eq!(detect_language("file.txt"), None);
    }

    #[test]
    fn test_extract_function_names() {
        let lines = vec![
            "function processInput(data) {".to_string(),
            "def handle_request(req):".to_string(),
            "pub fn parse_query(input: &str) -> Result {".to_string(),
        ];

        let names = extract_function_names(&lines);
        assert!(names.contains(&"processInput".to_string()));
        assert!(names.contains(&"handle_request".to_string()));
        assert!(names.contains(&"parse_query".to_string()));
    }
}
