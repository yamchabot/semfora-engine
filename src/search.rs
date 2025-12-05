//! Search hints for filtering queries
//!
//! This module provides filtering capabilities for symbol and file searches
//! based on extension, directory, file pattern, and programming language.

use serde::{Deserialize, Serialize};

/// Search hints for filtering queries
///
/// Supports filtering by extension, directory, file pattern, and language.
/// All filters are optional and combined with AND logic.
///
/// # Example
///
/// ```
/// use semfora_mcp::SearchHints;
///
/// let hints = SearchHints::new()
///     .with_ext("rs")
///     .with_dir("src")
///     .with_lang("rust");
///
/// assert!(hints.matches("src/lib.rs"));
/// assert!(!hints.matches("tests/test.py"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHints {
    /// File extension filter (e.g., "rs", "ts")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    /// Directory filter (e.g., "src", "tests")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// File glob pattern (e.g., "*.test.ts")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Language filter (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl SearchHints {
    /// Create empty search hints (matches all files)
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by file extension (e.g., "rs", "ts")
    pub fn with_ext(mut self, ext: impl Into<String>) -> Self {
        self.ext = Some(ext.into());
        self
    }

    /// Filter by directory name (e.g., "src", "tests")
    pub fn with_dir(mut self, dir: impl Into<String>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Filter by file glob pattern (e.g., "*.test.ts", "test_*")
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Filter by programming language (e.g., "rust", "python")
    pub fn with_lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    /// Check if a file path matches all configured hints
    ///
    /// Returns `true` if the path matches all non-None filters.
    /// Empty hints (all None) match any path.
    pub fn matches(&self, path: &str) -> bool {
        // Guard: extension check
        if let Some(ref ext) = self.ext {
            if !path.ends_with(&format!(".{}", ext)) {
                return false;
            }
        }

        // Guard: directory check
        if let Some(ref dir) = self.dir {
            let in_dir = path.contains(&format!("/{}/", dir))
                || path.starts_with(&format!("{}/", dir));
            if !in_dir {
                return false;
            }
        }

        // Guard: file pattern check (simple glob)
        if let Some(ref pattern) = self.file {
            if !self.matches_file_pattern(path, pattern) {
                return false;
            }
        }

        // Guard: language check
        if let Some(ref lang) = self.lang {
            match lang_from_extension(path) {
                Some(file_lang) if file_lang == lang.to_lowercase() => {}
                _ => return false,
            }
        }

        true
    }

    /// Check if filename matches a simple glob pattern
    fn matches_file_pattern(&self, path: &str, pattern: &str) -> bool {
        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if pattern.starts_with('*') {
            // Suffix match: *.test.ts
            let suffix = &pattern[1..];
            file_name.ends_with(suffix)
        } else if pattern.ends_with('*') {
            // Prefix match: test_*
            let prefix = &pattern[..pattern.len() - 1];
            file_name.starts_with(prefix)
        } else {
            // Exact match
            file_name == pattern
        }
    }
}

/// Map a file path to its programming language based on extension
///
/// Returns `None` for unknown extensions.
pub fn lang_from_extension(path: &str) -> Option<String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;

    let lang = match ext.to_lowercase().as_str() {
        // Rust
        "rs" => "rust",
        // Python
        "py" | "pyi" | "pyw" => "python",
        // JavaScript
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        // TypeScript
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        // Go
        "go" => "go",
        // Java
        "java" => "java",
        // Kotlin
        "kt" | "kts" => "kotlin",
        // C
        "c" | "h" => "c",
        // C++
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        // C#
        "cs" => "csharp",
        // Ruby
        "rb" => "ruby",
        // PHP
        "php" => "php",
        // Swift
        "swift" => "swift",
        // Scala
        "scala" | "sc" => "scala",
        // Shell
        "sh" | "bash" | "zsh" => "shell",
        // SQL
        "sql" => "sql",
        // HTML
        "html" | "htm" => "html",
        // CSS
        "css" => "css",
        // SCSS/Sass
        "scss" | "sass" => "scss",
        // JSON
        "json" => "json",
        // YAML
        "yaml" | "yml" => "yaml",
        // TOML
        "toml" => "toml",
        // Markdown
        "md" | "markdown" => "markdown",
        // Vue
        "vue" => "vue",
        // Svelte
        "svelte" => "svelte",
        _ => return None,
    };

    Some(lang.to_string())
}

/// Check if a file path appears to be a test file
///
/// Detects common test file patterns across multiple languages:
/// - Rust: `*_test.rs`, `tests/**/*.rs`
/// - TypeScript/JavaScript: `*.test.ts`, `*.spec.ts`, `__tests__/**`
/// - Python: `test_*.py`, `*_test.py`, `tests/**/*.py`
/// - Go: `*_test.go`
/// - Java: `*Test.java`, `*Tests.java`
///
/// # Example
///
/// ```
/// use semfora_mcp::search::is_test_file;
///
/// assert!(is_test_file("src/lib_test.rs"));
/// assert!(is_test_file("tests/integration.rs"));
/// assert!(is_test_file("src/button.test.ts"));
/// assert!(is_test_file("test_utils.py"));
/// assert!(!is_test_file("src/lib.rs"));
/// ```
pub fn is_test_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    let normalized_path = path_lower.replace('\\', "/");
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Directory-based patterns (always test files)
    if normalized_path.contains("/tests/")
        || normalized_path.contains("/__tests__/")
        || normalized_path.contains("/test/")
        || normalized_path.starts_with("tests/")
        || normalized_path.starts_with("__tests__/")
        || normalized_path.starts_with("test/")
    {
        return true;
    }

    // File name patterns by extension
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            // Rust: *_test.rs
            "rs" => {
                return file_name.ends_with("_test.rs")
                    || file_name.starts_with("test_");
            }
            // TypeScript/JavaScript: *.test.ts, *.spec.ts, *.test.js, *.spec.js
            "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => {
                return file_name.contains(".test.")
                    || file_name.contains(".spec.")
                    || file_name.starts_with("test_")
                    || file_name.starts_with("test.");
            }
            // Python: test_*.py, *_test.py
            "py" => {
                return file_name.starts_with("test_")
                    || file_name.ends_with("_test.py")
                    || file_name == "conftest.py";
            }
            // Go: *_test.go
            "go" => {
                return file_name.ends_with("_test.go");
            }
            // Java/Kotlin: *Test.java, *Tests.java, *Test.kt
            // Use original (non-lowercased) name for case-sensitive Java conventions
            "java" | "kt" | "kts" => {
                let original_name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                return original_name.ends_with("Test.java")
                    || original_name.ends_with("Tests.java")
                    || original_name.ends_with("Test.kt")
                    || original_name.ends_with("Tests.kt")
                    || original_name.ends_with("Test.kts")
                    || original_name.ends_with("Tests.kts")
                    || file_name.starts_with("test_")
                    || file_name.starts_with("test.");
            }
            // C/C++: test_*.c, *_test.cpp, *_test.c, *_test.cc, *_test.cxx
            "c" | "cpp" | "cc" | "cxx" => {
                return file_name.starts_with("test_")
                    || file_name.ends_with("_test.c")
                    || file_name.ends_with("_test.cpp")
                    || file_name.ends_with("_test.cc")
                    || file_name.ends_with("_test.cxx");
            }
            _ => {}
        }
    }

    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_hints_match_all() {
        let hints = SearchHints::new();
        assert!(hints.matches("any/file/path.rs"));
        assert!(hints.matches("foo.py"));
        assert!(hints.matches(""));
    }

    #[test]
    fn test_ext_filter() {
        let hints = SearchHints::new().with_ext("rs");
        assert!(hints.matches("src/lib.rs"));
        assert!(hints.matches("foo.rs"));
        assert!(!hints.matches("src/lib.ts"));
        assert!(!hints.matches("rs")); // No dot
    }

    #[test]
    fn test_dir_filter() {
        let hints = SearchHints::new().with_dir("src");
        assert!(hints.matches("src/lib.rs"));
        assert!(hints.matches("project/src/main.rs"));
        assert!(!hints.matches("tests/test.rs"));
        assert!(!hints.matches("lib.rs"));
    }

    #[test]
    fn test_file_pattern_suffix() {
        let hints = SearchHints::new().with_file("*.test.ts");
        assert!(hints.matches("src/button.test.ts"));
        assert!(hints.matches("foo.test.ts"));
        assert!(!hints.matches("src/button.ts"));
        assert!(!hints.matches("test.ts"));
    }

    #[test]
    fn test_file_pattern_prefix() {
        let hints = SearchHints::new().with_file("test_*");
        assert!(hints.matches("tests/test_foo.py"));
        assert!(hints.matches("test_bar.rs"));
        assert!(!hints.matches("foo_test.py"));
    }

    #[test]
    fn test_combined_filters() {
        let hints = SearchHints::new()
            .with_ext("rs")
            .with_dir("src");

        assert!(hints.matches("src/lib.rs"));
        assert!(hints.matches("project/src/main.rs"));
        assert!(!hints.matches("src/lib.ts")); // Wrong extension
        assert!(!hints.matches("tests/test.rs")); // Wrong directory
    }

    #[test]
    fn test_lang_filter_excludes_wrong_language() {
        let hints = SearchHints::new().with_lang("rust");
        assert!(!hints.matches("src/utils.py"));
        assert!(!hints.matches("src/app.ts"));
        assert!(!hints.matches("src/main.go"));
    }

    #[test]
    fn test_lang_filter_includes_correct_language() {
        let hints = SearchHints::new().with_lang("rust");
        assert!(hints.matches("src/lib.rs"));
        assert!(hints.matches("src/main.rs"));
    }

    #[test]
    fn test_lang_filter_typescript_vs_rust() {
        let hints_ts = SearchHints::new().with_lang("typescript");
        assert!(!hints_ts.matches("src/lib.rs"));
        assert!(hints_ts.matches("src/app.ts"));
        assert!(hints_ts.matches("src/component.tsx"));
    }

    #[test]
    fn test_lang_filter_case_insensitive() {
        assert!(SearchHints::new().with_lang("RUST").matches("src/lib.rs"));
        assert!(SearchHints::new().with_lang("rust").matches("src/lib.rs"));
        assert!(SearchHints::new().with_lang("Rust").matches("src/lib.rs"));
    }

    #[test]
    fn test_lang_filter_various_languages() {
        assert!(SearchHints::new().with_lang("python").matches("src/main.py"));
        assert!(SearchHints::new().with_lang("javascript").matches("src/app.js"));
        assert!(SearchHints::new().with_lang("javascript").matches("src/component.jsx"));
        assert!(SearchHints::new().with_lang("go").matches("src/main.go"));
        assert!(SearchHints::new().with_lang("java").matches("src/Main.java"));
        assert!(SearchHints::new().with_lang("cpp").matches("src/main.cpp"));
        assert!(SearchHints::new().with_lang("c").matches("src/main.c"));
    }

    #[test]
    fn test_lang_from_extension() {
        assert_eq!(lang_from_extension("foo.rs"), Some("rust".to_string()));
        assert_eq!(lang_from_extension("foo.py"), Some("python".to_string()));
        assert_eq!(lang_from_extension("foo.ts"), Some("typescript".to_string()));
        assert_eq!(lang_from_extension("foo.tsx"), Some("typescript".to_string()));
        assert_eq!(lang_from_extension("foo.unknown"), None);
    }

    // ========================================================================
    // Test file detection tests
    // ========================================================================

    #[test]
    fn test_is_test_file_rust() {
        assert!(is_test_file("src/lib_test.rs"));
        assert!(is_test_file("src/test_utils.rs"));
        assert!(is_test_file("tests/integration.rs"));
        assert!(!is_test_file("src/lib.rs"));
        assert!(!is_test_file("src/main.rs"));
    }

    #[test]
    fn test_is_test_file_typescript() {
        assert!(is_test_file("src/button.test.ts"));
        assert!(is_test_file("src/button.spec.ts"));
        assert!(is_test_file("src/button.test.tsx"));
        assert!(is_test_file("__tests__/button.ts"));
        assert!(!is_test_file("src/button.ts"));
        assert!(!is_test_file("src/components/Button.tsx"));
    }

    #[test]
    fn test_is_test_file_javascript() {
        assert!(is_test_file("src/utils.test.js"));
        assert!(is_test_file("src/utils.spec.js"));
        assert!(is_test_file("__tests__/utils.js"));
        assert!(!is_test_file("src/utils.js"));
    }

    #[test]
    fn test_is_test_file_python() {
        assert!(is_test_file("test_utils.py"));
        assert!(is_test_file("utils_test.py"));
        assert!(is_test_file("tests/test_api.py"));
        assert!(is_test_file("conftest.py"));
        assert!(!is_test_file("src/utils.py"));
        assert!(!is_test_file("main.py"));
    }

    #[test]
    fn test_is_test_file_go() {
        assert!(is_test_file("main_test.go"));
        assert!(is_test_file("handler_test.go"));
        assert!(!is_test_file("main.go"));
        assert!(!is_test_file("handler.go"));
    }

    #[test]
    fn test_is_test_file_java() {
        assert!(is_test_file("UserServiceTest.java"));
        assert!(is_test_file("UserServiceTests.java"));
        assert!(is_test_file("test/UserService.java"));
        assert!(is_test_file("test_utils.java"));
        assert!(!is_test_file("UserService.java"));
        // False positive checks - these should NOT be detected as tests
        assert!(!is_test_file("contest.java"));
        assert!(!is_test_file("attest.java"));
        assert!(!is_test_file("latest.java"));
        assert!(!is_test_file("testable.java"));
        assert!(!is_test_file("testimony.java"));
    }

    #[test]
    fn test_is_test_file_directory_patterns() {
        // tests/ directory
        assert!(is_test_file("tests/foo.rs"));
        assert!(is_test_file("project/tests/bar.py"));
        // __tests__/ directory (Jest convention)
        assert!(is_test_file("__tests__/component.tsx"));
        assert!(is_test_file("src/__tests__/utils.js"));
        // test/ directory
        assert!(is_test_file("test/unit.java"));
    }

    #[test]
    fn test_is_test_file_non_test_files() {
        assert!(!is_test_file("src/lib.rs"));
        assert!(!is_test_file("src/main.py"));
        assert!(!is_test_file("src/index.ts"));
        assert!(!is_test_file("README.md"));
        assert!(!is_test_file("Cargo.toml"));
    }

    #[test]
    fn test_is_test_file_cpp_false_positives() {
        // These should NOT be detected as test files
        assert!(!is_test_file("my_test.backup.cpp"));
        assert!(!is_test_file("old_test.2.c"));
        assert!(!is_test_file("latest.cpp"));
        assert!(!is_test_file("contest.c"));
        // But these SHOULD be detected
        assert!(is_test_file("my_test.cpp"));
        assert!(is_test_file("test_main.c"));
        assert!(is_test_file("parser_test.cc"));
    }
}
