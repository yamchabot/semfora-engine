//! Embedded agent templates for semfora workflow agents.
//!
//! These templates are in Claude Code's native markdown format and can be
//! converted to other platforms as needed.

/// Full codebase audit agent
pub const SEMFORA_AUDIT: &str = include_str!("semfora-audit.md");

/// Fast semantic code search agent
pub const SEMFORA_SEARCH: &str = include_str!("semfora-search.md");

/// PR/diff review agent
pub const SEMFORA_REVIEW: &str = include_str!("semfora-review.md");

/// Refactoring impact analysis agent
pub const SEMFORA_IMPACT: &str = include_str!("semfora-impact.md");

/// Code quality and complexity analysis agent
pub const SEMFORA_QUALITY: &str = include_str!("semfora-quality.md");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates_not_empty() {
        assert!(!SEMFORA_AUDIT.is_empty());
        assert!(!SEMFORA_SEARCH.is_empty());
        assert!(!SEMFORA_REVIEW.is_empty());
        assert!(!SEMFORA_IMPACT.is_empty());
        assert!(!SEMFORA_QUALITY.is_empty());
    }

    #[test]
    fn test_templates_have_frontmatter() {
        assert!(SEMFORA_AUDIT.starts_with("---"));
        assert!(SEMFORA_SEARCH.starts_with("---"));
        assert!(SEMFORA_REVIEW.starts_with("---"));
        assert!(SEMFORA_IMPACT.starts_with("---"));
        assert!(SEMFORA_QUALITY.starts_with("---"));
    }

    #[test]
    fn test_audit_contains_key_sections() {
        assert!(SEMFORA_AUDIT.contains("## Workflow"));
        assert!(SEMFORA_AUDIT.contains("get_overview"));
        assert!(SEMFORA_AUDIT.contains("get_callers"));
    }
}
