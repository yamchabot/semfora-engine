//! Behavioral risk calculation

use crate::schema::{RiskLevel, SemanticSummary};

/// Calculate behavioral risk level from a semantic summary
///
/// Risk scoring (tuned for practical use):
/// - +1 per new import (capped at 3)
/// - +1 per state variable
/// - +1 for presence of complex control flow (if/match/for), +1 if > 5, +1 if > 15
/// - +2 for I/O or network calls
/// - +3 for public API changes
/// - +3 for persistence operations
pub fn calculate_risk(summary: &SemanticSummary) -> RiskLevel {
    let mut score = 0;

    // +1 per new import, capped at 3 (imports are normal, not risky)
    score += summary.added_dependencies.len().min(3);

    // +1 per state variable
    score += summary.state_changes.len();

    // Control flow: graduated scoring instead of +2 per item
    // This prevents normal Rust files with many if/match from being "high risk"
    let cf_count = summary.control_flow_changes.len();
    if cf_count > 0 {
        score += 1; // Base: has control flow
    }
    if cf_count > 5 {
        score += 1; // Moderate complexity
    }
    if cf_count > 15 {
        score += 1; // High complexity
    }

    // +2 for I/O or network calls (detected via insertions)
    for insertion in &summary.insertions {
        let lower = insertion.to_lowercase();
        if lower.contains("network")
            || lower.contains("fetch")
            || lower.contains("invoke")
            || lower.contains("i/o")
            || lower.contains("file")
        {
            score += 2;
        }
    }

    // +3 for public API changes
    if summary.public_surface_changed {
        score += 3;
    }

    // +3 for persistence operations
    for insertion in &summary.insertions {
        let lower = insertion.to_lowercase();
        if lower.contains("storage")
            || lower.contains("database")
            || lower.contains("persist")
            || lower.contains("localstorage")
            || lower.contains("sessionstorage")
        {
            score += 3;
        }
    }

    RiskLevel::from_score(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ControlFlowChange, ControlFlowKind, Location, StateChange};

    #[test]
    fn test_low_risk() {
        let summary = SemanticSummary {
            added_dependencies: vec!["useState".to_string()],
            ..Default::default()
        };
        assert_eq!(calculate_risk(&summary), RiskLevel::Low);
    }

    #[test]
    fn test_medium_risk() {
        let summary = SemanticSummary {
            added_dependencies: vec!["useState".to_string(), "useEffect".to_string()],
            state_changes: vec![StateChange {
                name: "open".to_string(),
                state_type: "boolean".to_string(),
                initializer: "false".to_string(),
            }],
            ..Default::default()
        };
        assert_eq!(calculate_risk(&summary), RiskLevel::Medium);
    }

    #[test]
    fn test_high_risk_control_flow() {
        // High risk now requires more substantial changes
        // With new scoring: control flow is graduated (1 base + 1 if >5 + 1 if >15)
        let summary = SemanticSummary {
            added_dependencies: vec!["fetch".to_string()],
            control_flow_changes: vec![
                ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                },
                ControlFlowChange {
                    kind: ControlFlowKind::For,
                    location: Location::default(),
                },
            ],
            insertions: vec!["network call introduced".to_string()],
            public_surface_changed: true,
            ..Default::default()
        };
        // 1 import + 1 control flow + 2 network + 3 public = 7 = high
        assert_eq!(calculate_risk(&summary), RiskLevel::High);
    }

    #[test]
    fn test_high_risk_network() {
        let summary = SemanticSummary {
            insertions: vec!["network call introduced".to_string()],
            public_surface_changed: true,
            ..Default::default()
        };
        // 2 network + 3 public = 5 = high
        assert_eq!(calculate_risk(&summary), RiskLevel::High);
    }
}
