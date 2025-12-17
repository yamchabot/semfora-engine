//! Log4Shell (CVE-2021-44228) patterns
//!
//! Critical remote code execution vulnerability in Apache Log4j 2.x
//! CVSS: 10.0 (Critical)

use crate::lang::Lang;
use crate::security::{CVEPattern, PatternSource, Severity};
use crate::security::compiler::fingerprinter::fingerprint_from_source;

/// Log4Shell vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // Pattern 1: JNDI lookup in log message
        jndi_lookup_pattern(),
        // Pattern 2: LDAP/RMI context lookup
        naming_lookup_pattern(),
        // Pattern 3: Log4j2 MessagePatternConverter
        message_pattern_converter(),
    ]
}

/// JNDI lookup in log message - the primary Log4Shell attack vector
fn jndi_lookup_pattern() -> CVEPattern {
    let source = r#"
        logger.info("User: " + userInput);
        logger.error(request.getParameter("name"));
        log.debug("Data: " + untrustedData);
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CVE-2021-44228",
        vec!["CWE-502".into(), "CWE-917".into(), "CWE-20".into()],
        0,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "logger.info".into(),
        "logger.error".into(),
        "logger.warn".into(),
        "logger.debug".into(),
        "logger.trace".into(),
        "logger.fatal".into(),
        "log.info".into(),
        "log.error".into(),
        "log.warn".into(),
        "log.debug".into(),
    ])
    .with_cvss(10.0)
    .with_description("Log4Shell: User-controlled input passed to Log4j logging methods can trigger JNDI injection")
    .with_languages(vec![Lang::Java])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.9)
}

/// JNDI/LDAP/RMI context lookup - direct attack vector
fn naming_lookup_pattern() -> CVEPattern {
    let source = r#"
        Context ctx = new InitialContext();
        Object obj = ctx.lookup(userInput);
        InitialContext ic = new InitialContext();
        ic.lookup("ldap://" + data);
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CVE-2021-44228",
        vec!["CWE-502".into(), "CWE-917".into()],
        1,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "lookup".into(),
        "InitialContext".into(),
        "Context".into(),
        "bind".into(),
        "rebind".into(),
    ])
    .with_cvss(10.0)
    .with_description("JNDI lookup with user-controlled input enables remote code execution")
    .with_languages(vec![Lang::Java])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.95)
}

/// Log4j2 internal vulnerable component
fn message_pattern_converter() -> CVEPattern {
    let source = r#"
        MessagePatternConverter converter = new MessagePatternConverter();
        converter.format(event, toAppendTo);
        StrSubstitutor.replace(event.getMessage().getFormattedMessage());
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CVE-2021-44228",
        vec!["CWE-502".into(), "CWE-917".into()],
        2,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "MessagePatternConverter".into(),
        "StrSubstitutor".into(),
        "replace".into(),
        "getFormattedMessage".into(),
    ])
    .with_cvss(10.0)
    .with_description("Log4j2 internal MessagePatternConverter with string substitution")
    .with_languages(vec![Lang::Java])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.85)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log4shell_patterns() {
        let patterns = patterns();
        assert_eq!(patterns.len(), 3);

        for pattern in &patterns {
            assert_eq!(pattern.cve_id, "CVE-2021-44228");
            assert_eq!(pattern.severity, Severity::Critical);
            assert!(pattern.cwe_ids.contains(&"CWE-502".to_string()));
        }
    }
}
