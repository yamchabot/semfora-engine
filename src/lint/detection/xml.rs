//! XML linter detection (xmllint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{get_xmllint_version, has_xml_files, is_command_available};

/// Detect XML linters (xmllint)
pub fn detect_xml_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // xmllint is part of libxml2, typically available on Unix systems
    let xmllint_available = is_command_available("xmllint");

    // Detect if xmllint is installed and there are XML files
    if xmllint_available && has_xml_files(dir) {
        linters.push(DetectedLinter {
            linter: Linter::XmlLint,
            config_path: None, // xmllint doesn't use config files
            version: get_xmllint_version(),
            available: true,
            run_command: LintCommand {
                program: "xmllint".to_string(),
                args: vec!["--noout".to_string(), "*.xml".to_string()],
                fix_args: Some(vec![
                    "xmllint".to_string(),
                    "--format".to_string(),
                    "--output".to_string(),
                ]), // Note: xmllint --format reformats, but needs file-by-file handling
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false, // xmllint can format but doesn't have auto-fix for errors
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    linters
}
