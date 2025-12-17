//! Dockerfile semantic extractor
//!
//! Extracts semantic information from Dockerfiles for IaC security analysis.
//! Uses text-based parsing (tree-sitter-dockerfile requires update to tree-sitter 0.25).
//!
//! Supports detection of:
//! - Base images (FROM)
//! - Shell commands (RUN)
//! - Environment variables (ENV, ARG)
//! - User context (USER)
//! - Exposed ports (EXPOSE)
//! - File operations (COPY, ADD)
//! - Entry points (ENTRYPOINT, CMD)
//!
//! Security-relevant extractions:
//! - Running as root (missing USER directive)
//! - Hardcoded secrets in ENV/ARG
//! - Use of ADD instead of COPY (potential URL fetch)
//! - Unpinned base image tags (:latest)

use tree_sitter::Tree;

use crate::schema::{
    Call, SemanticSummary, StateChange, SymbolInfo, SymbolKind,
};
use crate::error::Result;

/// Extract semantic information from a Dockerfile
///
/// Note: Currently uses text-based parsing because tree-sitter-dockerfile
/// hasn't been updated to tree-sitter 0.25 yet. The Tree parameter is kept
/// for API consistency but is not used.
pub fn extract(summary: &mut SemanticSummary, source: &str, _tree: &Tree) -> Result<()> {
    // Parse Dockerfile line by line
    let mut has_user_directive = false;
    let mut current_line_number = 0usize;
    let mut security_issues: Vec<String> = Vec::new();

    // Join continuation lines (ending with \)
    let processed = preprocess_continuations(source);

    for line in processed.lines() {
        current_line_number += 1;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse instruction
        if let Some((instruction, args)) = parse_instruction(trimmed) {
            match instruction.to_uppercase().as_str() {
                "FROM" => {
                    let image = args.split_whitespace().next().unwrap_or(args);
                    // Handle AS alias (multi-stage)
                    let image = image.split_whitespace().next().unwrap_or(image);

                    summary.symbols.push(SymbolInfo {
                        name: format!("FROM {}", image),
                        kind: SymbolKind::Module,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });

                    // First FROM sets the primary symbol
                    if summary.symbol.is_none() {
                        summary.symbol = Some(format!("Dockerfile:{}", image));
                    }

                    summary.added_dependencies.push(format!("image:{}", image));

                    // Security check: unpinned images
                    if image.ends_with(":latest") || !image.contains(':') {
                        security_issues.push(format!("Unpinned image tag: {}", image));
                    }
                }

                "RUN" => {
                    let truncated = truncate(args, 50);

                    summary.symbols.push(SymbolInfo {
                        name: format!("RUN {}", truncated),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });

                    // Extract shell commands as calls
                    extract_shell_commands(summary, args);

                    // Security checks
                    let lower = args.to_lowercase();
                    if lower.contains("curl") && (lower.contains("| sh") || lower.contains("| bash")) {
                        security_issues.push("curl piped to shell - potential code injection".to_string());
                    }
                    if lower.contains("chmod 777") {
                        security_issues.push("chmod 777 - overly permissive".to_string());
                    }
                    if lower.contains("sudo") {
                        security_issues.push("sudo usage in container".to_string());
                    }
                }

                "ENV" => {
                    let pairs = parse_env_args(args);

                    for pair in &pairs {
                        summary.state_changes.push(StateChange {
                            name: pair.clone(),
                            state_type: "env".to_string(),
                            initializer: String::new(),
                        });

                        // Security check for secrets
                        let lower = pair.to_lowercase();
                        if lower.contains("password") || lower.contains("secret")
                            || lower.contains("api_key") || lower.contains("token")
                        {
                            security_issues.push(format!("Potential secret in ENV: {}", pair.split('=').next().unwrap_or(pair)));
                        }
                    }

                    summary.symbols.push(SymbolInfo {
                        name: format!("ENV {}", pairs.join(", ")),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                "ARG" => {
                    summary.state_changes.push(StateChange {
                        name: args.to_string(),
                        state_type: "arg".to_string(),
                        initializer: String::new(),
                    });

                    summary.symbols.push(SymbolInfo {
                        name: format!("ARG {}", args),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });

                    // Security check: ARG with default secret
                    if args.contains('=') {
                        let lower = args.to_lowercase();
                        if lower.contains("password") || lower.contains("secret") || lower.contains("token") {
                            security_issues.push(format!("Potential secret in ARG default: {}", args.split('=').next().unwrap_or(args)));
                        }
                    }
                }

                "EXPOSE" => {
                    let ports: Vec<&str> = args.split_whitespace().collect();

                    for port in &ports {
                        summary.state_changes.push(StateChange {
                            name: format!("port:{}", port),
                            state_type: "expose".to_string(),
                            initializer: String::new(),
                        });
                    }

                    summary.symbols.push(SymbolInfo {
                        name: format!("EXPOSE {}", ports.join(" ")),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                "USER" => {
                    has_user_directive = true;
                    let user = args.split(':').next().unwrap_or(args);

                    summary.state_changes.push(StateChange {
                        name: format!("user:{}", user),
                        state_type: "user".to_string(),
                        initializer: String::new(),
                    });

                    summary.symbols.push(SymbolInfo {
                        name: format!("USER {}", user),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });

                    if user == "root" || user == "0" {
                        security_issues.push("Explicit USER root".to_string());
                    }
                }

                "COPY" => {
                    summary.symbols.push(SymbolInfo {
                        name: "COPY".to_string(),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                "ADD" => {
                    summary.symbols.push(SymbolInfo {
                        name: "ADD".to_string(),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });

                    summary.insertions.push("ADD: consider using COPY instead".to_string());

                    if args.contains("http://") || args.contains("https://") {
                        security_issues.push("ADD with URL - consider using curl + verification".to_string());
                    }
                }

                "WORKDIR" => {
                    summary.state_changes.push(StateChange {
                        name: format!("workdir:{}", args),
                        state_type: "workdir".to_string(),
                        initializer: String::new(),
                    });

                    summary.symbols.push(SymbolInfo {
                        name: format!("WORKDIR {}", args),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                "ENTRYPOINT" | "CMD" => {
                    summary.symbols.push(SymbolInfo {
                        name: format!("{} {}", instruction.to_uppercase(), truncate(args, 30)),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                "LABEL" | "MAINTAINER" | "VOLUME" | "HEALTHCHECK" | "SHELL" | "STOPSIGNAL" | "ONBUILD" => {
                    summary.symbols.push(SymbolInfo {
                        name: instruction.to_uppercase(),
                        kind: SymbolKind::Function,
                        start_line: current_line_number,
                        end_line: current_line_number,
                        ..Default::default()
                    });
                }

                _ => {}
            }
        }
    }

    // Security check: no USER directive means running as root
    if !has_user_directive && !summary.symbols.is_empty() {
        security_issues.push("No USER directive - container runs as root".to_string());
    }

    // Add security issues as insertions
    for issue in security_issues {
        summary.insertions.push(format!("Security: {}", issue));
    }

    // Set extraction complete if we found any instructions
    summary.extraction_complete = !summary.symbols.is_empty();

    // Set line boundaries
    if let Some(first) = summary.symbols.first() {
        summary.start_line = Some(first.start_line);
    }
    if let Some(last) = summary.symbols.last() {
        summary.end_line = Some(last.end_line);
    }

    // Dockerfiles modify infrastructure
    if !summary.symbols.is_empty() {
        summary.public_surface_changed = true;
    }

    Ok(())
}

/// Preprocess source to handle line continuations
fn preprocess_continuations(source: &str) -> String {
    let mut result = String::new();
    let mut continuation = String::new();

    for line in source.lines() {
        if line.trim_end().ends_with('\\') {
            // Remove the backslash and accumulate
            let without_backslash = line.trim_end().strip_suffix('\\').unwrap_or(line);
            continuation.push_str(without_backslash);
            continuation.push(' ');
        } else {
            continuation.push_str(line);
            result.push_str(&continuation);
            result.push('\n');
            continuation.clear();
        }
    }

    // Don't forget any trailing continuation
    if !continuation.is_empty() {
        result.push_str(&continuation);
    }

    result
}

/// Parse a Dockerfile instruction line into (instruction, arguments)
fn parse_instruction(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();

    // Find the instruction (first word)
    let space_idx = trimmed.find(|c: char| c.is_whitespace());

    match space_idx {
        Some(idx) => {
            let instruction = &trimmed[..idx];
            let args = trimmed[idx..].trim();
            Some((instruction, args))
        }
        None => {
            // Instruction with no arguments
            Some((trimmed, ""))
        }
    }
}

/// Parse ENV arguments into key=value or key value pairs
fn parse_env_args(args: &str) -> Vec<String> {
    let mut pairs = Vec::new();

    // ENV can be:
    // ENV KEY=value KEY2=value2
    // ENV KEY value (single pair, space-separated)

    if args.contains('=') {
        // Key=value format, possibly multiple
        for part in args.split_whitespace() {
            if part.contains('=') {
                pairs.push(part.to_string());
            }
        }
    } else {
        // Space-separated single pair: KEY value
        let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
        if !parts.is_empty() {
            pairs.push(parts.join("="));
        }
    }

    pairs
}

/// Extract shell commands as calls
fn extract_shell_commands(summary: &mut SemanticSummary, cmd: &str) {
    // Parse shell commands separated by &&, ;, or |
    for part in cmd.split("&&").flat_map(|s| s.split(';')).flat_map(|s| s.split('|')) {
        let trimmed = part.trim();
        if let Some(cmd_name) = trimmed.split_whitespace().next() {
            // Skip shell builtins that aren't meaningful
            if !["[", "test", "true", "false", "echo"].contains(&cmd_name) {
                summary.calls.push(Call {
                    name: cmd_name.to_string(),
                    object: Some("shell".to_string()),
                    ..Default::default()
                });
            }
        }
    }
}

/// Truncate string with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn parse_dockerfile(source: &str) -> SemanticSummary {
        // Create a dummy tree (not used by text-based parser)
        let mut parser = tree_sitter::Parser::new();
        // Use bash grammar as placeholder since dockerfile grammar isn't available
        parser.set_language(&Lang::Bash.tree_sitter_language()).unwrap();
        let tree = parser.parse("", None).unwrap();

        let mut summary = SemanticSummary::default();
        extract(&mut summary, source, &tree).unwrap();
        summary
    }

    #[test]
    fn test_extract_from_instruction() {
        let source = r#"
FROM node:18-alpine
WORKDIR /app
"#;
        let summary = parse_dockerfile(source);

        assert!(summary.symbol.is_some());
        assert!(summary.symbol.as_ref().unwrap().contains("node:18-alpine"));
        assert!(!summary.symbols.is_empty());
    }

    #[test]
    fn test_unpinned_image_warning() {
        let source = "FROM ubuntu:latest";
        let summary = parse_dockerfile(source);

        // Should have security warning about unpinned image
        assert!(summary.insertions.iter().any(|i| i.contains("Unpinned")));
    }

    #[test]
    fn test_curl_pipe_bash_warning() {
        let source = r#"
FROM alpine
RUN curl -sSL https://example.com/install.sh | bash
"#;
        let summary = parse_dockerfile(source);

        assert!(summary.insertions.iter().any(|i| i.contains("curl") && i.contains("shell")));
    }

    #[test]
    fn test_env_secret_detection() {
        let source = r#"
FROM alpine
ENV API_KEY=secret123
"#;
        let summary = parse_dockerfile(source);

        assert!(summary.insertions.iter().any(|i| i.contains("secret") || i.contains("API_KEY")));
    }

    #[test]
    fn test_missing_user_directive() {
        let source = r#"
FROM node:18
RUN npm install
CMD ["node", "app.js"]
"#;
        let summary = parse_dockerfile(source);

        // Should have warning about running as root
        assert!(summary.insertions.iter().any(|i| i.contains("root")));
    }

    #[test]
    fn test_user_directive_present() {
        let source = r#"
FROM node:18
RUN npm install
USER node
CMD ["node", "app.js"]
"#;
        let summary = parse_dockerfile(source);

        // Should NOT have warning about running as root (user is set to non-root)
        assert!(!summary.insertions.iter().any(|i| i.contains("No USER directive")));
    }

    #[test]
    fn test_add_warning() {
        let source = r#"
FROM alpine
ADD https://example.com/file.tar.gz /app/
"#;
        let summary = parse_dockerfile(source);

        // Should have warning about ADD
        assert!(summary.insertions.iter().any(|i| i.contains("ADD")));
    }

    #[test]
    fn test_shell_command_extraction() {
        let source = r#"
FROM alpine
RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*
"#;
        let summary = parse_dockerfile(source);

        // Should extract shell commands
        let cmd_names: Vec<&str> = summary.calls.iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(cmd_names.contains(&"apt-get"));
        assert!(cmd_names.contains(&"rm"));
    }

    #[test]
    fn test_multiline_continuation() {
        let source = r#"
FROM alpine
RUN apt-get update \
    && apt-get install -y \
    curl \
    wget
"#;
        let summary = parse_dockerfile(source);

        // Should parse as single RUN instruction
        let run_count = summary.symbols.iter()
            .filter(|s| s.name.starts_with("RUN"))
            .count();
        assert_eq!(run_count, 1);
    }

    #[test]
    fn test_expose_ports() {
        let source = r#"
FROM node:18
EXPOSE 3000 8080
"#;
        let summary = parse_dockerfile(source);

        // Should have port state changes
        let port_changes: Vec<_> = summary.state_changes.iter()
            .filter(|s| s.state_type == "expose")
            .collect();
        assert_eq!(port_changes.len(), 2);
    }
}
