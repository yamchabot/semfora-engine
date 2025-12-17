//! Spring4Shell (CVE-2022-22965) patterns
//!
//! Remote code execution via Spring Framework data binding
//! CVSS: 9.8 (Critical)

use crate::lang::Lang;
use crate::security::{CVEPattern, PatternSource, Severity};
use crate::security::compiler::fingerprinter::fingerprint_from_source;

/// Spring4Shell vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // Pattern 1: ClassLoader manipulation via data binding
        classloader_manipulation_pattern(),
        // Pattern 2: Unsafe PropertyDescriptor usage
        property_descriptor_pattern(),
    ]
}

/// ClassLoader manipulation via Spring data binding
fn classloader_manipulation_pattern() -> CVEPattern {
    let source = r#"
        @RequestMapping("/user")
        public String handleRequest(User user) {
            // Spring data binding allows setting nested properties
            // class.module.classLoader.resources.context.parent.pipeline
            return "success";
        }

        BeanWrapperImpl wrapper = new BeanWrapperImpl(target);
        wrapper.setPropertyValue("class.module.classLoader", value);
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CVE-2022-22965",
        vec!["CWE-94".into(), "CWE-1321".into()],
        0,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "BeanWrapperImpl".into(),
        "setPropertyValue".into(),
        "getPropertyValue".into(),
        "DataBinder".into(),
        "bind".into(),
        "WebDataBinder".into(),
    ])
    .with_cvss(9.8)
    .with_description("Spring4Shell: Data binding allows classLoader manipulation leading to RCE")
    .with_languages(vec![Lang::Java])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.9)
}

/// Unsafe PropertyDescriptor usage in Spring
fn property_descriptor_pattern() -> CVEPattern {
    let source = r#"
        CachedIntrospectionResults.forClass(beanClass);
        PropertyDescriptor[] pds = introspectionResults.getPropertyDescriptors();
        for (PropertyDescriptor pd : pds) {
            if ("class".equals(pd.getName())) {
                // This is the vulnerable path
                pd.getWriteMethod().invoke(target, value);
            }
        }
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CVE-2022-22965",
        vec!["CWE-94".into(), "CWE-1321".into()],
        1,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "CachedIntrospectionResults".into(),
        "PropertyDescriptor".into(),
        "getPropertyDescriptors".into(),
        "getWriteMethod".into(),
        "invoke".into(),
    ])
    .with_cvss(9.8)
    .with_description("Spring Framework PropertyDescriptor allows access to class property")
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
    fn test_spring4shell_patterns() {
        let patterns = patterns();
        assert_eq!(patterns.len(), 2);

        for pattern in &patterns {
            assert_eq!(pattern.cve_id, "CVE-2022-22965");
            assert_eq!(pattern.severity, Severity::Critical);
        }
    }
}
