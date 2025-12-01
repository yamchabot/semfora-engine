//! Semantic model data structures for code analysis

use serde::{Deserialize, Serialize};

/// Complete semantic summary of a file
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SemanticSummary {
    /// File path
    pub file: String,

    /// Language name
    pub language: String,

    /// Primary symbol name (function, class, component)
    pub symbol: Option<String>,

    /// Kind of the primary symbol
    pub symbol_kind: Option<SymbolKind>,

    /// Component props (for React/Vue components)
    pub props: Vec<Prop>,

    /// Function arguments
    pub arguments: Vec<Argument>,

    /// Return type annotation
    pub return_type: Option<String>,

    /// Descriptive insertions (rule-based summaries)
    pub insertions: Vec<String>,

    /// Added imports/dependencies
    pub added_dependencies: Vec<String>,

    /// State variable changes
    pub state_changes: Vec<StateChange>,

    /// Control flow changes
    pub control_flow_changes: Vec<ControlFlowChange>,

    /// Function calls detected
    pub calls: Vec<Call>,

    /// Whether the public API surface changed
    pub public_surface_changed: bool,

    /// Behavioral risk level
    pub behavioral_risk: RiskLevel,

    /// Raw source fallback (for incomplete extraction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_fallback: Option<String>,

    /// Whether extraction was complete
    #[serde(skip)]
    pub extraction_complete: bool,
}

/// Kind of symbol being analyzed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    /// Regular function
    #[default]
    Function,
    /// React/Vue component
    Component,
    /// Class definition
    Class,
    /// Method inside a class/impl
    Method,
    /// TypeScript/Java interface
    Interface,
    /// Rust trait
    Trait,
    /// Rust/Go struct
    Struct,
    /// Enum definition
    Enum,
    /// Module/namespace
    Module,
    /// Type alias
    TypeAlias,
}

impl SymbolKind {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Component => "component",
            Self::Class => "class",
            Self::Method => "method",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Module => "module",
            Self::TypeAlias => "type_alias",
        }
    }
}

/// Component prop definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prop {
    /// Prop name
    pub name: String,

    /// Type annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop_type: Option<String>,

    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,

    /// Whether the prop is required
    pub required: bool,
}

/// Function argument definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Argument {
    /// Argument name
    pub name: String,

    /// Type annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_type: Option<String>,

    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// State variable change
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateChange {
    /// Variable name
    pub name: String,

    /// Variable type
    pub state_type: String,

    /// Initializer expression
    pub initializer: String,
}

/// Control flow change
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlFlowChange {
    /// Kind of control flow
    pub kind: ControlFlowKind,

    /// Location in source
    pub location: Location,
}

/// Kind of control flow construct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlFlowKind {
    /// If statement/expression
    #[default]
    If,
    /// For loop
    For,
    /// While loop
    While,
    /// Switch statement
    Switch,
    /// Match expression (Rust)
    Match,
    /// Try-catch block
    Try,
    /// Infinite loop (Rust)
    Loop,
}

impl ControlFlowKind {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::If => "if",
            Self::For => "for",
            Self::While => "while",
            Self::Switch => "switch",
            Self::Match => "match",
            Self::Try => "try",
            Self::Loop => "loop",
        }
    }
}

/// Source code location
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    /// Line number (1-indexed)
    pub line: usize,

    /// Column number (0-indexed)
    pub column: usize,
}

impl Location {
    /// Create a new location
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Behavioral risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low risk (0-1 points)
    #[default]
    Low,
    /// Medium risk (2-3 points)
    Medium,
    /// High risk (4+ points)
    High,
}

impl RiskLevel {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Calculate risk level from a score
    pub fn from_score(score: usize) -> Self {
        match score {
            0..=1 => Self::Low,
            2..=3 => Self::Medium,
            _ => Self::High,
        }
    }
}

/// JSX element for insertion rule processing
#[derive(Debug, Clone, Default)]
pub struct JsxElement {
    /// Tag name
    pub tag: String,

    /// Props/attributes
    pub props: Vec<(String, Option<String>)>,

    /// Whether it's self-closing
    pub is_self_closing: bool,

    /// Source location
    pub location: Location,
}

/// Function/method call for analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Call {
    /// Function/method name
    pub name: String,

    /// Object for method calls (e.g., "console" for console.log)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,

    /// Whether this call is awaited
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_awaited: bool,

    /// Whether this call is inside a try block
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub in_try: bool,

    /// Whether this is a React hook
    #[serde(skip)]
    pub is_hook: bool,

    /// Whether this is an I/O operation
    #[serde(skip)]
    pub is_io: bool,

    /// Source location
    #[serde(skip)]
    pub location: Location,
}

impl Call {
    /// Check if this call is a React hook based on naming convention
    pub fn check_is_hook(name: &str) -> bool {
        name.starts_with("use")
            && name
                .chars()
                .nth(3)
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
    }

    /// Check if this call is an I/O operation
    pub fn check_is_io(name: &str) -> bool {
        matches!(
            name,
            "fetch"
                | "invoke"
                | "axios"
                | "request"
                | "get"
                | "post"
                | "put"
                | "delete"
                | "open"
                | "read"
                | "write"
                | "readFile"
                | "writeFile"
                | "readFileSync"
                | "writeFileSync"
        )
    }
}

/// Import statement
#[derive(Debug, Clone, Default)]
pub struct Import {
    /// Module source path
    pub source: String,

    /// Imported names
    pub names: Vec<ImportedName>,

    /// Whether this is a default import
    pub is_default: bool,

    /// Whether this is a namespace import
    pub is_namespace: bool,
}

/// Individual imported name
#[derive(Debug, Clone, Default)]
pub struct ImportedName {
    /// Original name
    pub name: String,

    /// Alias (if renamed)
    pub alias: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(1), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(2), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(3), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(4), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(10), RiskLevel::High);
    }

    #[test]
    fn test_call_is_hook() {
        assert!(Call::check_is_hook("useState"));
        assert!(Call::check_is_hook("useEffect"));
        assert!(Call::check_is_hook("useCallback"));
        assert!(!Call::check_is_hook("use")); // Too short
        assert!(!Call::check_is_hook("usestuff")); // Lowercase after "use"
        assert!(!Call::check_is_hook("fetch"));
    }

    #[test]
    fn test_call_is_io() {
        assert!(Call::check_is_io("fetch"));
        assert!(Call::check_is_io("invoke"));
        assert!(Call::check_is_io("readFile"));
        assert!(!Call::check_is_io("useState"));
        assert!(!Call::check_is_io("map"));
    }

    #[test]
    fn test_symbol_kind_str() {
        assert_eq!(SymbolKind::Function.as_str(), "function");
        assert_eq!(SymbolKind::Component.as_str(), "component");
        assert_eq!(SymbolKind::Class.as_str(), "class");
    }

    #[test]
    fn test_control_flow_kind_str() {
        assert_eq!(ControlFlowKind::If.as_str(), "if");
        assert_eq!(ControlFlowKind::For.as_str(), "for");
        assert_eq!(ControlFlowKind::Match.as_str(), "match");
    }
}
