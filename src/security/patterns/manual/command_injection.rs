//! OS Command Injection (CWE-78) patterns
//!
//! Patterns for command injection vulnerabilities across languages

use crate::lang::Lang;
use crate::security::{CVEPattern, PatternSource, Severity};
use crate::security::compiler::fingerprinter::fingerprint_from_source;

/// Command injection vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // JavaScript/Node.js
        js_child_process_exec(),
        js_shell_spawn(),
        // Python
        python_os_system(),
        python_subprocess_shell(),
        // Rust
        rust_command_shell(),
        // Java
        java_runtime_exec(),
    ]
}

/// JavaScript child_process.exec command injection
fn js_child_process_exec() -> CVEPattern {
    let source = r#"
        const { exec } = require('child_process');
        exec("ls " + userInput);
        exec(`ping ${host}`);
        child_process.exec("echo " + data);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-78-JS-EXEC",
        vec!["CWE-78".into()],
        0,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "exec".into(),
        "execSync".into(),
        "child_process".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via child_process.exec in Node.js")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.90)
}

/// JavaScript shell spawn with string command
fn js_shell_spawn() -> CVEPattern {
    let source = r#"
        spawn("sh", ["-c", userCommand]);
        spawn("bash", ["-c", "echo " + data]);
        spawnSync("cmd", ["/c", command]);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new(
        "CWE-78-JS-SPAWN",
        vec!["CWE-78".into()],
        1,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "spawn".into(),
        "spawnSync".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via spawn with shell in Node.js")
    .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.85)
}

/// Python os.system command injection
fn python_os_system() -> CVEPattern {
    let source = r#"
        import os
        os.system("ls " + user_input)
        os.popen("cat " + filename)
        os.system(f"ping {host}")
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new(
        "CWE-78-PY-SYSTEM",
        vec!["CWE-78".into()],
        2,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "system".into(),
        "popen".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via os.system in Python")
    .with_languages(vec![Lang::Python])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.90)
}

/// Python subprocess with shell=True
fn python_subprocess_shell() -> CVEPattern {
    let source = r#"
        import subprocess
        subprocess.run("ls " + path, shell=True)
        subprocess.call(f"echo {data}", shell=True)
        subprocess.Popen(command, shell=True)
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new(
        "CWE-78-PY-SUBPROCESS",
        vec!["CWE-78".into()],
        3,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "run".into(),
        "call".into(),
        "Popen".into(),
        "check_output".into(),
        "check_call".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via subprocess with shell=True in Python")
    .with_languages(vec![Lang::Python])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.85)
}

/// Rust Command with shell
fn rust_command_shell() -> CVEPattern {
    let source = r#"
        Command::new("sh")
            .arg("-c")
            .arg(&format!("ls {}", user_input))
            .output()?;
        std::process::Command::new("bash")
            .args(["-c", &command])
            .spawn()?;
    "#;

    let fp = fingerprint_from_source(source, Lang::Rust);

    CVEPattern::new(
        "CWE-78-RUST-SHELL",
        vec!["CWE-78".into()],
        4,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "Command".into(),
        "arg".into(),
        "args".into(),
        "spawn".into(),
        "output".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via shell invocation in Rust")
    .with_languages(vec![Lang::Rust])
    .with_source(PatternSource::ManualCuration {
        author: "Semfora Security Team".into(),
        date: "2024-01-01".into(),
    })
    .with_confidence(0.80)
}

/// Java Runtime.exec command injection
fn java_runtime_exec() -> CVEPattern {
    let source = r#"
        Runtime.getRuntime().exec("ls " + userInput);
        Runtime.getRuntime().exec(new String[]{"sh", "-c", command});
        ProcessBuilder pb = new ProcessBuilder("bash", "-c", cmd);
        pb.start();
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new(
        "CWE-78-JAVA-EXEC",
        vec!["CWE-78".into()],
        5,
    )
    .with_fingerprints(fp.fingerprints.call, fp.fingerprints.control_flow, fp.fingerprints.state)
    .with_vulnerable_calls(vec![
        "exec".into(),
        "getRuntime".into(),
        "ProcessBuilder".into(),
        "start".into(),
    ])
    .with_cvss(9.8)
    .with_description("Command injection via Runtime.exec in Java")
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
    fn test_command_injection_patterns() {
        let patterns = patterns();
        assert!(!patterns.is_empty());

        for pattern in &patterns {
            assert!(pattern.cwe_ids.contains(&"CWE-78".to_string()));
            assert!(pattern.severity == Severity::Critical);
        }
    }
}
