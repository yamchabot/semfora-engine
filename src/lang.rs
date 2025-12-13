//! Language detection and tree-sitter grammar loading

use std::path::Path;
use tree_sitter::Language;

use crate::error::{McpDiffError, Result};

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Rust,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Kotlin,
    Html,
    Css,
    Scss,
    Json,
    Yaml,
    Toml,
    Xml,
    Hcl,
    Markdown,
    /// Vue Single File Component (.vue)
    Vue,
    /// Shell/Bash scripts
    Bash,
    /// Gradle build files (Groovy-based)
    Gradle,
}

impl Lang {
    /// Detect language from file path extension
    pub fn from_path(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| McpDiffError::UnsupportedLanguage {
                extension: "none".to_string(),
            })?;

        Self::from_extension(ext)
    }

    /// Detect language from file extension string
    pub fn from_extension(ext: &str) -> Result<Self> {
        match ext.to_lowercase().as_str() {
            "ts" | "mts" | "cts" => Ok(Self::TypeScript),
            "tsx" => Ok(Self::Tsx),
            "js" | "mjs" | "cjs" => Ok(Self::JavaScript),
            "jsx" => Ok(Self::Jsx),
            "rs" => Ok(Self::Rust),
            "py" | "pyi" => Ok(Self::Python),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "c" | "h" => Ok(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Ok(Self::Cpp),
            "kt" | "kts" => Ok(Self::Kotlin),
            "html" | "htm" => Ok(Self::Html),
            "css" => Ok(Self::Css),
            "scss" | "sass" => Ok(Self::Scss),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            "toml" => Ok(Self::Toml),
            "xml" | "xsd" | "xsl" | "xslt" | "svg" | "plist" | "pom" => Ok(Self::Xml),
            "tf" | "hcl" | "tfvars" => Ok(Self::Hcl),
            "md" | "markdown" => Ok(Self::Markdown),
            "vue" => Ok(Self::Vue),
            "sh" | "bash" | "zsh" | "fish" => Ok(Self::Bash),
            "gradle" => Ok(Self::Gradle),
            _ => Err(McpDiffError::UnsupportedLanguage {
                extension: ext.to_string(),
            }),
        }
    }

    /// Get the canonical name of the language
    pub fn name(&self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Kotlin => "kotlin",
            Self::Html => "html",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Xml => "xml",
            Self::Hcl => "hcl",
            Self::Markdown => "markdown",
            Self::Vue => "vue",
            Self::Bash => "bash",
            Self::Gradle => "gradle",
        }
    }

    /// Get the tree-sitter Language for parsing
    ///
    /// Note: Vue SFCs use HTML grammar for overall structure.
    /// The script section is extracted and parsed separately.
    pub fn tree_sitter_language(&self) -> Language {
        match self {
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript | Self::Jsx => tree_sitter_javascript::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Self::Html | Self::Vue => tree_sitter_html::LANGUAGE.into(),
            Self::Css => tree_sitter_css::language().into(),
            Self::Scss => tree_sitter_scss::language().into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Yaml => tree_sitter_yaml::language().into(),
            Self::Toml => tree_sitter_toml_ng::language().into(),
            Self::Xml => tree_sitter_xml::LANGUAGE_XML.into(),
            Self::Hcl => tree_sitter_hcl::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Gradle => tree_sitter_groovy::LANGUAGE.into(),
        }
    }

    /// Get the language family for shared extraction logic
    pub fn family(&self) -> LangFamily {
        match self {
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx | Self::Vue => {
                LangFamily::JavaScript
            }
            Self::Rust => LangFamily::Rust,
            Self::Python => LangFamily::Python,
            Self::Go => LangFamily::Go,
            Self::Java => LangFamily::Java,
            Self::Kotlin => LangFamily::Kotlin,
            Self::C | Self::Cpp => LangFamily::CFamily,
            Self::Html | Self::Css | Self::Scss | Self::Markdown => LangFamily::Markup,
            Self::Json | Self::Yaml | Self::Toml | Self::Xml => LangFamily::Config,
            Self::Hcl => LangFamily::Hcl,
            Self::Bash => LangFamily::Shell,
            Self::Gradle => LangFamily::Gradle,
        }
    }

    /// Check if this language supports JSX syntax
    pub fn supports_jsx(&self) -> bool {
        matches!(self, Self::Tsx | Self::Jsx | Self::Vue)
    }

    /// Check if this is a programming language (vs markup/config)
    pub fn is_programming_language(&self) -> bool {
        matches!(
            self.family(),
            LangFamily::JavaScript
                | LangFamily::Rust
                | LangFamily::Python
                | LangFamily::Go
                | LangFamily::Java
                | LangFamily::Kotlin
                | LangFamily::CFamily
                | LangFamily::Shell
                | LangFamily::Gradle
        )
    }

    /// Get common file extensions for this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::TypeScript => &["ts", "mts", "cts"],
            Self::Tsx => &["tsx"],
            Self::JavaScript => &["js", "mjs", "cjs"],
            Self::Jsx => &["jsx"],
            Self::Rust => &["rs"],
            Self::Python => &["py", "pyi"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cc", "cxx", "hpp", "hxx", "hh"],
            Self::Kotlin => &["kt", "kts"],
            Self::Html => &["html", "htm"],
            Self::Css => &["css"],
            Self::Scss => &["scss", "sass"],
            Self::Json => &["json"],
            Self::Yaml => &["yaml", "yml"],
            Self::Toml => &["toml"],
            Self::Xml => &["xml", "xsd", "xsl", "xslt", "svg", "plist", "pom"],
            Self::Hcl => &["tf", "hcl", "tfvars"],
            Self::Markdown => &["md", "markdown"],
            Self::Vue => &["vue"],
            Self::Bash => &["sh", "bash", "zsh", "fish"],
            Self::Gradle => &["gradle"],
        }
    }

    /// Check if this is a Vue Single File Component
    pub fn is_vue_sfc(&self) -> bool {
        matches!(self, Self::Vue)
    }
}

/// Language families for grouping similar extraction logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangFamily {
    /// JavaScript, TypeScript, JSX, TSX
    JavaScript,
    /// Rust
    Rust,
    /// Python
    Python,
    /// Go
    Go,
    /// Java
    Java,
    /// Kotlin
    Kotlin,
    /// C and C++
    CFamily,
    /// HTML, CSS, SCSS, Markdown
    Markup,
    /// JSON, YAML, TOML, XML
    Config,
    /// HCL/Terraform
    Hcl,
    /// Shell/Bash
    Shell,
    /// Gradle (Groovy-based)
    Gradle,
}

impl LangFamily {
    /// Get the canonical name of the language family
    pub fn name(&self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Go => "go",
            Self::Java => "java",
            Self::Kotlin => "kotlin",
            Self::CFamily => "c_family",
            Self::Markup => "markup",
            Self::Config => "config",
            Self::Hcl => "hcl",
            Self::Shell => "shell",
            Self::Gradle => "gradle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_language_detection() {
        assert_eq!(Lang::from_extension("ts").unwrap(), Lang::TypeScript);
        assert_eq!(Lang::from_extension("tsx").unwrap(), Lang::Tsx);
        assert_eq!(Lang::from_extension("js").unwrap(), Lang::JavaScript);
        assert_eq!(Lang::from_extension("jsx").unwrap(), Lang::Jsx);
        assert_eq!(Lang::from_extension("rs").unwrap(), Lang::Rust);
        assert_eq!(Lang::from_extension("py").unwrap(), Lang::Python);
        assert_eq!(Lang::from_extension("go").unwrap(), Lang::Go);
        assert_eq!(Lang::from_extension("java").unwrap(), Lang::Java);
        assert_eq!(Lang::from_extension("c").unwrap(), Lang::C);
        assert_eq!(Lang::from_extension("cpp").unwrap(), Lang::Cpp);
        assert_eq!(Lang::from_extension("html").unwrap(), Lang::Html);
        assert_eq!(Lang::from_extension("css").unwrap(), Lang::Css);
        assert_eq!(Lang::from_extension("json").unwrap(), Lang::Json);
        assert_eq!(Lang::from_extension("yaml").unwrap(), Lang::Yaml);
        assert_eq!(Lang::from_extension("toml").unwrap(), Lang::Toml);
        assert_eq!(Lang::from_extension("md").unwrap(), Lang::Markdown);
        assert_eq!(Lang::from_extension("vue").unwrap(), Lang::Vue);
    }

    #[test]
    fn test_language_from_path() {
        let path = PathBuf::from("src/components/App.tsx");
        assert_eq!(Lang::from_path(&path).unwrap(), Lang::Tsx);

        let path = PathBuf::from("main.rs");
        assert_eq!(Lang::from_path(&path).unwrap(), Lang::Rust);
    }

    #[test]
    fn test_unsupported_extension() {
        assert!(Lang::from_extension("xyz").is_err());
    }

    #[test]
    fn test_jsx_support() {
        assert!(Lang::Tsx.supports_jsx());
        assert!(Lang::Jsx.supports_jsx());
        assert!(Lang::Vue.supports_jsx()); // Vue can contain JSX in script
        assert!(!Lang::TypeScript.supports_jsx());
        assert!(!Lang::JavaScript.supports_jsx());
        assert!(!Lang::Rust.supports_jsx());
    }

    #[test]
    fn test_language_families() {
        assert_eq!(Lang::TypeScript.family(), LangFamily::JavaScript);
        assert_eq!(Lang::Tsx.family(), LangFamily::JavaScript);
        assert_eq!(Lang::Vue.family(), LangFamily::JavaScript);
        assert_eq!(Lang::Rust.family(), LangFamily::Rust);
        assert_eq!(Lang::Python.family(), LangFamily::Python);
        assert_eq!(Lang::C.family(), LangFamily::CFamily);
        assert_eq!(Lang::Cpp.family(), LangFamily::CFamily);
        assert_eq!(Lang::Json.family(), LangFamily::Config);
        assert_eq!(Lang::Html.family(), LangFamily::Markup);
    }

    #[test]
    fn test_vue_sfc() {
        assert!(Lang::Vue.is_vue_sfc());
        assert!(!Lang::TypeScript.is_vue_sfc());
        assert!(!Lang::JavaScript.is_vue_sfc());

        // Vue should be detected from path
        let path = PathBuf::from("src/components/HelloWorld.vue");
        assert_eq!(Lang::from_path(&path).unwrap(), Lang::Vue);
    }
}
