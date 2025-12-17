//! Cross-Site Scripting (CWE-79) patterns
//!
//! Generic patterns for XSS vulnerabilities across languages

use crate::lang::Lang;
use crate::security::{CVEPattern, PatternSource, Severity};
use crate::security::compiler::fingerprinter::fingerprint_from_source;

/// XSS vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // JavaScript/TypeScript DOM XSS
        js_innerhtml_xss(),
        js_document_write_xss(),
        // React dangerouslySetInnerHTML
        react_dangerous_html(),
        // Server-side template injection
        template_injection(),
    ]
}

/// JavaScript innerHTML XSS
fn js_innerhtml_xss() -> CVEPattern {
    let source = r#"
        element.innerHTML = userInput;
        document.getElementById("output").innerHTML = data;
        container.innerHTML = "<div>" + untrustedData + "</div>";
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-79-JS-INNERHTML",
        vec!["CWE-79".into()],
        0,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "innerHTML".into(),
        "outerHTML".into(),
        "insertAdjacentHTML".into(),
    ])
    .with_cvss(6.1)
    .with_description("XSS via innerHTML assignment with user-controlled input")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.85)
}

/// JavaScript document.write XSS
fn js_document_write_xss() -> CVEPattern {
    let source = r#"
        document.write(userInput);
        document.write("<script>" + data + "</script>");
        document.writeln(untrustedContent);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-79-JS-DOCWRITE",
        vec!["CWE-79".into()],
        1,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "write".into(),
        "writeln".into(),
    ])
    .with_cvss(6.1)
    .with_description("XSS via document.write with user-controlled input")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.80)
}

/// React dangerouslySetInnerHTML XSS
fn react_dangerous_html() -> CVEPattern {
    let source = r#"
        <div dangerouslySetInnerHTML={{ __html: userInput }} />
        <span dangerouslySetInnerHTML={{ __html: data }} />
        return <div dangerouslySetInnerHTML={{ __html: props.content }} />;
    "#;

    let fp = fingerprint_from_source(source, Lang::Jsx);

    CVEPattern::new(
        "CWE-79-REACT-DANGEROUS",
        vec!["CWE-79".into()],
        2,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "dangerouslySetInnerHTML".into(),
    ])
    .with_cvss(6.1)
    .with_description("XSS via React dangerouslySetInnerHTML with user-controlled input")
    .with_languages(vec![Lang::Jsx, Lang::Tsx, Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.90)
}

/// Server-side template injection leading to XSS
fn template_injection() -> CVEPattern {
    let source = r#"
        res.send("<html><body>" + userInput + "</body></html>");
        response.write("Hello, " + name);
        ctx.body = "<div>" + data + "</div>";
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-79-TEMPLATE-INJECTION",
        vec!["CWE-79".into(), "CWE-94".into()],
        3,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "send".into(),
        "write".into(),
        "render".into(),
    ])
    .with_cvss(6.1)
    .with_description("XSS via server-side HTML string concatenation")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript, Lang::Python])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.75)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xss_patterns() {
        let patterns = patterns();
        assert!(!patterns.is_empty());

        for pattern in &patterns {
            assert!(pattern.cwe_ids.contains(&"CWE-79".to_string()));
        }
    }
}
