//! Insecure Deserialization (CWE-502) patterns
//!
//! Patterns for unsafe deserialization vulnerabilities across languages

use crate::lang::Lang;
use crate::security::{CVEPattern, PatternSource, Severity};
use crate::security::compiler::fingerprinter::fingerprint_from_source;

/// Deserialization vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // JavaScript/TypeScript
        js_json_parse_eval(),
        // Python pickle
        python_pickle_loads(),
        python_yaml_load(),
        // Java ObjectInputStream
        java_object_input_stream(),
        // C# BinaryFormatter
        csharp_binary_formatter(),
    ]
}

/// JavaScript JSON.parse with eval fallback
fn js_json_parse_eval() -> CVEPattern {
    let source = r#"
        const data = eval("(" + userInput + ")");
        const obj = new Function("return " + data)();
        eval(jsonString);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-502-JS-EVAL",
        vec!["CWE-502".into(), "CWE-94".into()],
        0,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "eval".into(),
        "Function".into(),
    ])
    .with_cvss(9.8)
    .with_description("Code execution via eval-based deserialization in JavaScript")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.95)
}

/// Python pickle.loads - arbitrary code execution
fn python_pickle_loads() -> CVEPattern {
    let source = r#"
        import pickle
        data = pickle.loads(user_input)
        obj = pickle.load(untrusted_file)
        result = cPickle.loads(raw_data)
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new(
        "CWE-502-PY-PICKLE",
        vec!["CWE-502".into()],
        1,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "loads".into(),
        "load".into(),
        "pickle".into(),
        "cPickle".into(),
    ])
    .with_cvss(9.8)
    .with_description("Arbitrary code execution via pickle deserialization in Python")
    .with_languages(vec![Lang::Python])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.95)
}

/// Python yaml.load without safe loader
fn python_yaml_load() -> CVEPattern {
    let source = r#"
        import yaml
        data = yaml.load(user_input)
        config = yaml.load(open("config.yaml"))
        # Safe alternative: yaml.safe_load(data)
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new(
        "CWE-502-PY-YAML",
        vec!["CWE-502".into()],
        2,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "load".into(),
        "yaml".into(),
    ])
    .with_cvss(9.8)
    .with_description("Code execution via unsafe YAML deserialization in Python")
    .with_languages(vec![Lang::Python])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.85)
}

/// Java ObjectInputStream - arbitrary code execution
fn java_object_input_stream() -> CVEPattern {
    let source = r#"
        ObjectInputStream ois = new ObjectInputStream(inputStream);
        Object obj = ois.readObject();
        XMLDecoder decoder = new XMLDecoder(input);
        Object data = decoder.readObject();
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CWE-502-JAVA-OIS",
        vec!["CWE-502".into()],
        3,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "ObjectInputStream".into(),
        "readObject".into(),
        "XMLDecoder".into(),
        "XStream".into(),
        "fromXML".into(),
    ])
    .with_cvss(9.8)
    .with_description("Arbitrary code execution via Java object deserialization")
    .with_languages(vec![Lang::Java])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.90)
}

/// C# BinaryFormatter - arbitrary code execution
fn csharp_binary_formatter() -> CVEPattern {
    let source = r#"
        BinaryFormatter formatter = new BinaryFormatter();
        object obj = formatter.Deserialize(stream);
        var data = new JavaScriptSerializer().Deserialize(input);
        JsonConvert.DeserializeObject(json, new JsonSerializerSettings { TypeNameHandling = TypeNameHandling.Auto });
    "#;

    let fp = fingerprint_from_source(source, Lang::CSharp);

    CVEPattern::new(
        "CWE-502-CSHARP-BINARY",
        vec!["CWE-502".into()],
        4,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "BinaryFormatter".into(),
        "Deserialize".into(),
        "DeserializeObject".into(),
        "JavaScriptSerializer".into(),
        "TypeNameHandling".into(),
    ])
    .with_cvss(9.8)
    .with_description("Arbitrary code execution via BinaryFormatter deserialization in C#")
    .with_languages(vec![Lang::CSharp])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.90)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialization_patterns() {
        let patterns = patterns();
        assert!(!patterns.is_empty());

        for pattern in &patterns {
            assert!(pattern.cwe_ids.contains(&"CWE-502".to_string()));
            assert!(pattern.severity == Severity::Critical);
        }
    }
}
