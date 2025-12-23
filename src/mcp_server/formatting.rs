//! Formatting helpers for MCP tool output
//!
//! This module contains formatting functions used to convert internal
//! data structures into the TOON (Token-Optimized Object Notation) format.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// ============================================================================
// Version Header
// ============================================================================

/// Package version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Generate TOON header with type and version
///
/// All MCP tool responses should start with this header for consistency.
/// Format: `_type: <type_name>\nversion: <version>\n`
#[inline]
pub fn toon_header(type_name: &str) -> String {
    format!("_type: {}\nversion: {}\n", type_name, VERSION)
}

use crate::parsing::parse_and_extract;
use crate::{encode_toon, CacheDir, Lang, SymbolIndexEntry};

// ============================================================================
// Diff Formatting
// ============================================================================

/// Format diff output with pagination support - TOON format
/// Returns paginated file analysis with semantic summaries
pub fn format_diff_output_paginated(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
    offset: usize,
    limit: usize,
) -> String {
    let total_files = changed_files.len();

    // Apply pagination
    let page_files: Vec<_> = changed_files.iter().skip(offset).take(limit).collect();

    // TOON header with pagination metadata
    let mut output = toon_header("analyze_diff");
    output.push_str(&format!("base: \"{}\"\n", base_ref));
    output.push_str(&format!("target: \"{}\"\n", target_ref));
    output.push_str(&format!("total_files: {}\n", total_files));
    output.push_str(&format!("showing: {}\n", page_files.len()));
    output.push_str(&format!("offset: {}\n", offset));
    output.push_str(&format!("limit: {}\n", limit));

    // Pagination hint
    if offset + page_files.len() < total_files {
        output.push_str(&format!(
            "next_offset: {} (use offset={} for next page)\n",
            offset + page_files.len(),
            offset + limit
        ));
    }

    // Count change types for summary (BTreeMap for deterministic order)
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for f in changed_files {
        *by_type.entry(f.change_type.as_str()).or_insert(0) += 1;
    }
    let type_summary: Vec<_> = by_type
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("changes: {}\n", type_summary.join(", ")));

    if page_files.is_empty() {
        if total_files == 0 {
            output.push_str("\n_note: No files changed.\n");
        } else {
            output.push_str(&format!(
                "\n_note: No files at offset {}. Total: {}.\n",
                offset, total_files
            ));
        }
        return output;
    }

    output.push_str(&format!("\nfiles[{}]:\n", page_files.len()));

    // Format each file in the page
    for changed_file in page_files {
        let full_path = working_dir.join(&changed_file.path);

        output.push_str(&format!(
            "  {} [{}]\n",
            changed_file.path,
            changed_file.change_type.as_str()
        ));

        if changed_file.change_type == crate::git::ChangeType::Deleted {
            output.push_str("    (deleted)\n");
            continue;
        }

        let lang = match Lang::from_path(&full_path) {
            Ok(l) => l,
            Err(_) => {
                output.push_str("    (unsupported)\n");
                continue;
            }
        };

        let source = match fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => {
                output.push_str("    (unreadable)\n");
                continue;
            }
        };

        match parse_and_extract(&full_path, &source, lang) {
            Ok(summary) => {
                // Indent the TOON output
                let toon = encode_toon(&summary);
                for line in toon.lines() {
                    output.push_str(&format!("    {}\n", line));
                }
            }
            Err(e) => {
                output.push_str(&format!("    (error: {})\n", e));
            }
        }
    }

    output
}

/// Format diff summary only - compact overview without per-file details
/// Returns aggregate statistics for large diffs
pub fn format_diff_summary(
    working_dir: &Path,
    base_ref: &str,
    target_ref: &str,
    changed_files: &[crate::git::ChangedFile],
) -> String {
    use std::collections::HashMap;

    // TOON header
    let mut output = toon_header("analyze_diff_summary");
    output.push_str(&format!("base: \"{}\"\n", base_ref));
    output.push_str(&format!("target: \"{}\"\n", target_ref));
    output.push_str(&format!("total_files: {}\n", changed_files.len()));

    if changed_files.is_empty() {
        output.push_str("_note: No files changed.\n");
        return output;
    }

    // Count by change type (BTreeMap for deterministic order)
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for f in changed_files {
        *by_type.entry(f.change_type.as_str()).or_insert(0) += 1;
    }
    let type_summary: Vec<_> = by_type
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("by_change_type: {}\n", type_summary.join(", ")));

    // Count by language/extension
    let mut by_lang: HashMap<String, usize> = HashMap::new();
    for f in changed_files {
        let full_path = working_dir.join(&f.path);
        let lang_name = match Lang::from_path(&full_path) {
            Ok(l) => format!("{:?}", l),
            Err(_) => {
                // Use extension as fallback
                full_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "other".to_string())
            }
        };
        *by_lang.entry(lang_name).or_insert(0) += 1;
    }
    // Sort by count descending, take top 10
    let mut lang_vec: Vec<_> = by_lang.iter().collect();
    lang_vec.sort_by(|a, b| b.1.cmp(a.1));
    let lang_summary: Vec<_> = lang_vec
        .iter()
        .take(10)
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    output.push_str(&format!("by_language: {}\n", lang_summary.join(", ")));

    // Group by module (directory)
    let mut by_module: HashMap<String, usize> = HashMap::new();
    for f in changed_files {
        let module = std::path::Path::new(&f.path)
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| if s.is_empty() { "(root)" } else { s })
            .unwrap_or("(root)")
            .to_string();
        *by_module.entry(module).or_insert(0) += 1;
    }
    // Sort by count descending, take top 10
    let mut module_vec: Vec<_> = by_module.iter().collect();
    module_vec.sort_by(|a, b| b.1.cmp(a.1));
    let module_summary: Vec<_> = module_vec
        .iter()
        .take(10)
        .map(|(k, v)| format!("{} ({})", k, v))
        .collect();
    output.push_str(&format!("top_modules: {}\n", module_summary.join(", ")));

    // Quick risk assessment based on file types and locations
    let mut high_risk = 0;
    let mut medium_risk = 0;
    let mut low_risk = 0;

    for f in changed_files {
        let path_lower = f.path.to_lowercase();
        // Check for security-sensitive patterns
        let has_key_pattern = path_lower.contains("api_key")
            || path_lower.contains("apikey")
            || path_lower.contains("private_key")
            || path_lower.contains("secret_key")
            || path_lower.contains("encryption_key")
            || path_lower.contains("/keys/")
            || path_lower.ends_with("_key.rs")
            || path_lower.ends_with("_key.ts")
            || path_lower.ends_with("_key.py");
        if path_lower.contains("auth")
            || path_lower.contains("security")
            || path_lower.contains("crypt")
            || path_lower.contains("password")
            || path_lower.contains("secret")
            || has_key_pattern
            || path_lower.contains(".env")
        {
            high_risk += 1;
        } else if path_lower.contains("config")
            || path_lower.contains("api")
            || path_lower.contains("database")
            || path_lower.contains("migration")
            || path_lower.contains("schema")
        {
            medium_risk += 1;
        } else {
            low_risk += 1;
        }
    }
    output.push_str(&format!(
        "risk_estimate: high={}, medium={}, low={}\n",
        high_risk, medium_risk, low_risk
    ));

    // Hint for getting details
    output.push_str("\n_hint: Use limit/offset params to paginate file details, or omit summary_only for full analysis.\n");

    output
}

// ============================================================================
// Language Support
// ============================================================================

/// Get the list of supported languages as a formatted string
pub(super) fn get_supported_languages() -> String {
    let languages = vec![
        ("TypeScript", ".ts"),
        ("TSX", ".tsx"),
        ("JavaScript", ".js, .mjs, .cjs"),
        ("JSX", ".jsx"),
        ("Rust", ".rs"),
        ("Python", ".py, .pyi"),
        ("Go", ".go"),
        ("Java", ".java"),
        ("C#", ".cs"),
        ("C", ".c, .h"),
        ("C++", ".cpp, .cc, .cxx, .hpp, .hxx, .hh"),
        ("Kotlin", ".kt, .kts"),
        ("HTML", ".html, .htm"),
        ("CSS", ".css"),
        ("SCSS", ".scss, .sass"),
        ("JSON", ".json"),
        ("YAML", ".yaml, .yml"),
        ("TOML", ".toml"),
        ("XML", ".xml, .xsd, .xsl, .xslt, .svg, .plist, .pom"),
        ("HCL/Terraform", ".tf, .hcl, .tfvars"),
        ("Markdown", ".md, .markdown"),
        ("Vue", ".vue"),
        ("Bash/Shell", ".sh, .bash, .zsh, .fish"),
        ("Gradle", ".gradle"),
    ];

    let mut output = String::from("Supported Languages:\n\n");
    for (name, extensions) in languages {
        output.push_str(&format!("  {} ({})\n", name, extensions));
    }
    output
}

// ============================================================================
// Module Symbols Formatting
// ============================================================================

/// Format module symbols listing as compact TOON
pub(super) fn format_module_symbols(
    module: &str,
    results: &[SymbolIndexEntry],
    cache: &CacheDir,
) -> String {
    let mut output = toon_header("module_symbols");
    output.push_str(&format!("module: \"{}\"\n", module));
    output.push_str(&format!("total: {}\n", results.len()));

    if results.is_empty() {
        let available = cache.list_modules();
        output.push_str("symbols: (none)\n");
        output.push_str(&format!(
            "hint: available modules are: {}\n",
            available.join(", ")
        ));
    } else {
        output.push_str(&format!("symbols[{}]{{s,h,k,f,l,r}}:\n", results.len()));
        for entry in results {
            output.push_str(&format!(
                "  {},{},{},{},{},{}\n",
                entry.symbol, entry.hash, entry.kind, entry.file, entry.lines, entry.risk
            ));
        }
    }

    output
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // get_supported_languages Tests
    // ========================================================================

    #[test]
    fn test_get_supported_languages_includes_common_languages() {
        let output = get_supported_languages();
        assert!(output.contains("TypeScript"));
        assert!(output.contains("Rust"));
        assert!(output.contains("Python"));
        assert!(output.contains("JavaScript"));
        assert!(output.contains("Go"));
    }

    #[test]
    fn test_get_supported_languages_includes_extensions() {
        let output = get_supported_languages();
        assert!(output.contains(".ts"));
        assert!(output.contains(".rs"));
        assert!(output.contains(".py"));
        assert!(output.contains(".go"));
    }

    // ========================================================================
    // format_diff_output_paginated Tests
    // ========================================================================

    fn make_changed_file(
        path: &str,
        change_type: crate::git::ChangeType,
    ) -> crate::git::ChangedFile {
        crate::git::ChangedFile {
            path: path.to_string(),
            old_path: None,
            change_type,
        }
    }

    #[test]
    fn test_format_diff_output_paginated_empty() {
        let temp = tempfile::tempdir().unwrap();
        let output = format_diff_output_paginated(temp.path(), "main", "HEAD", &[], 0, 20);

        assert!(output.contains("_type: analyze_diff"));
        assert!(output.contains("base: \"main\""));
        assert!(output.contains("target: \"HEAD\""));
        assert!(output.contains("total_files: 0"));
        assert!(output.contains("showing: 0"));
        assert!(output.contains("No files changed"));
    }

    #[test]
    fn test_format_diff_output_paginated_single_file() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/main.ts"),
            "export function hello(): string { return 'hi'; }",
        )
        .unwrap();

        let files = vec![make_changed_file(
            "src/main.ts",
            crate::git::ChangeType::Modified,
        )];

        let output = format_diff_output_paginated(temp.path(), "main", "HEAD", &files, 0, 20);

        assert!(output.contains("_type: analyze_diff"));
        assert!(output.contains("total_files: 1"));
        assert!(output.contains("showing: 1"));
        assert!(output.contains("src/main.ts [modified]"));
        assert!(!output.contains("next_offset:"));
    }

    #[test]
    fn test_format_diff_output_paginated_with_pagination() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        for i in 0..5 {
            std::fs::write(
                temp.path().join(format!("src/file{}.ts", i)),
                format!("export function f{}(): number {{ return {}; }}", i, i),
            )
            .unwrap();
        }

        let files: Vec<_> = (0..5)
            .map(|i| make_changed_file(&format!("src/file{}.ts", i), crate::git::ChangeType::Added))
            .collect();

        let output = format_diff_output_paginated(temp.path(), "main", "HEAD", &files, 0, 2);

        assert!(output.contains("total_files: 5"));
        assert!(output.contains("showing: 2"));
        assert!(output.contains("offset: 0"));
        assert!(output.contains("limit: 2"));
        assert!(output.contains("next_offset: 2"));
        assert!(output.contains("file0.ts"));
        assert!(output.contains("file1.ts"));
        assert!(!output.contains("file4.ts"));
    }

    #[test]
    fn test_format_diff_output_paginated_deleted_file() {
        let temp = tempfile::tempdir().unwrap();

        let files = vec![make_changed_file(
            "src/deleted.ts",
            crate::git::ChangeType::Deleted,
        )];

        let output = format_diff_output_paginated(temp.path(), "main", "HEAD", &files, 0, 20);

        assert!(output.contains("src/deleted.ts [deleted]"));
        assert!(output.contains("(deleted)"));
    }

    // ========================================================================
    // format_diff_summary Tests
    // ========================================================================

    #[test]
    fn test_format_diff_summary_empty() {
        let temp = tempfile::tempdir().unwrap();
        let output = format_diff_summary(temp.path(), "main", "HEAD", &[]);

        assert!(output.contains("_type: analyze_diff_summary"));
        assert!(output.contains("total_files: 0"));
        assert!(output.contains("No files changed"));
    }

    #[test]
    fn test_format_diff_summary_multiple_types() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/app.ts"), "// ts").unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "// rs").unwrap();

        let files = vec![
            make_changed_file("src/app.ts", crate::git::ChangeType::Added),
            make_changed_file("src/app.ts", crate::git::ChangeType::Added),
            make_changed_file("src/lib.rs", crate::git::ChangeType::Modified),
        ];

        let output = format_diff_summary(temp.path(), "main", "HEAD", &files);

        assert!(output.contains("_type: analyze_diff_summary"));
        assert!(output.contains("total_files: 3"));
        assert!(output.contains("by_change_type:"));
        assert!(output.contains("by_language:"));
        assert!(output.contains("top_modules:"));
    }

    #[test]
    fn test_format_diff_summary_risk_assessment() {
        let temp = tempfile::tempdir().unwrap();

        let files = vec![
            make_changed_file("src/auth/login.ts", crate::git::ChangeType::Modified),
            make_changed_file("src/api/handler.ts", crate::git::ChangeType::Modified),
            make_changed_file("src/utils/format.ts", crate::git::ChangeType::Modified),
        ];

        let output = format_diff_summary(temp.path(), "main", "HEAD", &files);

        assert!(output.contains("risk_estimate:"));
        assert!(output.contains("high=1"));
        assert!(output.contains("medium=1"));
        assert!(output.contains("low=1"));
    }
}
