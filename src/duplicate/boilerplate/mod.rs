//! Boilerplate detection and classification
//!
//! Functions are classified as "expected duplicates" based on patterns.
//! These are excluded from duplicate detection by default.
//!
//! # Architecture
//!
//! This module uses a **registry pattern** for scalable pattern management:
//! - Each language has its own submodule (javascript.rs, rust.rs, etc.)
//! - Patterns are registered as static arrays with function pointers
//! - `all_patterns()` collects from all language modules
//! - Easy to add new languages without modifying core logic
//!
//! # Adding a New Language
//!
//! 1. Create `{language}.rs` with detection functions
//! 2. Define `pub static PATTERNS: &[PatternMatcher]`
//! 3. Add `mod {language};` and chain in `all_patterns()`
//! 4. Add categories to `BoilerplateCategory` enum

pub mod javascript;
pub mod rust;

use crate::lang::Lang;
use crate::schema::SymbolInfo;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// =============================================================================
// Boilerplate Category Enum
// =============================================================================

/// Category of boilerplate code
///
/// These patterns represent code that is commonly duplicated by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoilerplateCategory {
    // =========================================================================
    // JavaScript/TypeScript Patterns
    // =========================================================================
    /// React Query hooks (useQuery/useMutation with minimal logic)
    ReactQuery,
    /// React hook wrappers (useState/useEffect patterns)
    ReactHook,
    /// Event handlers (handleClick, onChange with 1-2 calls)
    EventHandler,
    /// API route handlers (Express/Next.js patterns)
    ApiRoute,
    /// Test setup functions (beforeEach, setup, teardown)
    TestSetup,
    /// Type guard functions (isX() type checking)
    TypeGuard,
    /// Config/export boilerplate (module.exports patterns)
    ConfigExport,

    // =========================================================================
    // Rust Patterns
    // =========================================================================
    /// Trait implementation (Default, Clone, From, Into, Display, etc.)
    RustTraitImpl,
    /// Builder pattern method (with_*, set_*, builder)
    RustBuilder,
    /// Getter method (get_*, is_*, has_*)
    RustGetter,
    /// Setter method (set_*)
    RustSetter,
    /// Constructor (new, default, from_*, try_from_*)
    RustConstructor,
    /// Conversion method (to_*, as_*, into_*)
    RustConversion,
    /// Derive-generated method (clone, default, etc.)
    RustDerived,
    /// Error From implementation
    RustErrorFrom,
    /// Iterator implementation (next, into_iter, iter, iter_mut)
    RustIterator,
    /// Deref/DerefMut implementation
    RustDeref,
    /// Drop implementation
    RustDrop,
    /// Test function (#[test])
    RustTest,
    /// Serde serialization helpers (serialize_*, deserialize_*)
    RustSerde,

    // =========================================================================
    // Cross-Language Patterns
    // =========================================================================
    /// Custom user-defined boilerplate category
    Custom,
}

impl BoilerplateCategory {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            // JavaScript/TypeScript
            BoilerplateCategory::ReactQuery => "React Query hook pattern",
            BoilerplateCategory::ReactHook => "React hook wrapper",
            BoilerplateCategory::EventHandler => "Event handler with minimal logic",
            BoilerplateCategory::ApiRoute => "API route handler",
            BoilerplateCategory::TestSetup => "Test setup/teardown function",
            BoilerplateCategory::TypeGuard => "Type guard function",
            BoilerplateCategory::ConfigExport => "Config/export boilerplate",
            // Rust
            BoilerplateCategory::RustTraitImpl => "Rust trait implementation",
            BoilerplateCategory::RustBuilder => "Rust builder pattern method",
            BoilerplateCategory::RustGetter => "Rust getter method",
            BoilerplateCategory::RustSetter => "Rust setter method",
            BoilerplateCategory::RustConstructor => "Rust constructor function",
            BoilerplateCategory::RustConversion => "Rust conversion method",
            BoilerplateCategory::RustDerived => "Rust derive-generated method",
            BoilerplateCategory::RustErrorFrom => "Rust Error From implementation",
            BoilerplateCategory::RustIterator => "Rust iterator implementation",
            BoilerplateCategory::RustDeref => "Rust Deref implementation",
            BoilerplateCategory::RustDrop => "Rust Drop implementation",
            BoilerplateCategory::RustTest => "Rust test function",
            BoilerplateCategory::RustSerde => "Rust serde helper",
            // Cross-language
            BoilerplateCategory::Custom => "Custom boilerplate pattern",
        }
    }

    /// Get the language this category applies to (None = cross-language)
    pub fn language(&self) -> Option<Lang> {
        match self {
            // JavaScript/TypeScript patterns
            BoilerplateCategory::ReactQuery
            | BoilerplateCategory::ReactHook
            | BoilerplateCategory::EventHandler
            | BoilerplateCategory::ApiRoute
            | BoilerplateCategory::TestSetup
            | BoilerplateCategory::TypeGuard
            | BoilerplateCategory::ConfigExport => Some(Lang::JavaScript),
            // Rust patterns
            BoilerplateCategory::RustTraitImpl
            | BoilerplateCategory::RustBuilder
            | BoilerplateCategory::RustGetter
            | BoilerplateCategory::RustSetter
            | BoilerplateCategory::RustConstructor
            | BoilerplateCategory::RustConversion
            | BoilerplateCategory::RustDerived
            | BoilerplateCategory::RustErrorFrom
            | BoilerplateCategory::RustIterator
            | BoilerplateCategory::RustDeref
            | BoilerplateCategory::RustDrop
            | BoilerplateCategory::RustTest
            | BoilerplateCategory::RustSerde => Some(Lang::Rust),
            // Cross-language
            BoilerplateCategory::Custom => None,
        }
    }
}

// =============================================================================
// Pattern Registry
// =============================================================================

/// A pattern matcher registered in the global registry
///
/// Each pattern defines:
/// - Which boilerplate category it detects
/// - Which languages it applies to
/// - A detection function
/// - Whether it's enabled by default
pub struct PatternMatcher {
    /// The category this pattern detects
    pub category: BoilerplateCategory,
    /// Languages this pattern applies to (empty = all languages)
    pub languages: &'static [Lang],
    /// Detection function: returns true if the symbol matches this pattern
    pub detector: fn(&SymbolInfo) -> bool,
    /// Whether this pattern is enabled by default
    pub enabled_by_default: bool,
}

/// Get all registered patterns from all language modules
pub fn all_patterns() -> impl Iterator<Item = &'static PatternMatcher> {
    javascript::PATTERNS
        .iter()
        .chain(rust::PATTERNS.iter())
}

/// Check if a language is compatible with pattern's target languages
fn is_lang_compatible(lang: Option<Lang>, pattern_langs: &[Lang]) -> bool {
    // Empty pattern languages means it applies to all
    if pattern_langs.is_empty() {
        return true;
    }

    let Some(l) = lang else { return false };

    for &pattern_lang in pattern_langs {
        if l == pattern_lang {
            return true;
        }
        // JS patterns also apply to TS/JSX/TSX
        if pattern_lang == Lang::JavaScript {
            if matches!(l, Lang::TypeScript | Lang::Jsx | Lang::Tsx) {
                return true;
            }
        }
    }
    false
}

// =============================================================================
// Configuration
// =============================================================================

/// Built-in boilerplate pattern toggles
///
/// Uses a HashSet of disabled categories for O(1) lookup.
/// All patterns are enabled by default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuiltinBoilerplate {
    /// Categories that have been explicitly disabled
    #[serde(default)]
    disabled: HashSet<BoilerplateCategory>,
}

impl BuiltinBoilerplate {
    /// Create with all patterns enabled
    pub fn all_enabled() -> Self {
        Self {
            disabled: HashSet::new(),
        }
    }

    /// Create with all patterns disabled
    pub fn all_disabled() -> Self {
        let mut disabled = HashSet::new();
        // Add all categories
        for pattern in all_patterns() {
            disabled.insert(pattern.category);
        }
        Self { disabled }
    }

    /// Check if a category is enabled
    pub fn is_enabled(&self, category: BoilerplateCategory) -> bool {
        !self.disabled.contains(&category)
    }

    /// Enable a category
    pub fn enable(&mut self, category: BoilerplateCategory) {
        self.disabled.remove(&category);
    }

    /// Disable a category
    pub fn disable(&mut self, category: BoilerplateCategory) {
        self.disabled.insert(category);
    }

    /// Set a category's enabled state
    pub fn set(&mut self, category: BoilerplateCategory, enabled: bool) {
        if enabled {
            self.enable(category);
        } else {
            self.disable(category);
        }
    }

    // Legacy field accessors for backwards compatibility with existing configs
    // These map the old boolean field names to the new HashSet-based system

    /// Get react_query enabled state
    pub fn react_query(&self) -> bool {
        self.is_enabled(BoilerplateCategory::ReactQuery)
    }
    /// Set react_query enabled state
    pub fn set_react_query(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::ReactQuery, enabled);
    }

    /// Get react_hooks enabled state
    pub fn react_hooks(&self) -> bool {
        self.is_enabled(BoilerplateCategory::ReactHook)
    }
    /// Set react_hooks enabled state
    pub fn set_react_hooks(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::ReactHook, enabled);
    }

    /// Get event_handlers enabled state
    pub fn event_handlers(&self) -> bool {
        self.is_enabled(BoilerplateCategory::EventHandler)
    }
    /// Set event_handlers enabled state
    pub fn set_event_handlers(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::EventHandler, enabled);
    }

    /// Get test_setup enabled state
    pub fn test_setup(&self) -> bool {
        self.is_enabled(BoilerplateCategory::TestSetup)
    }
    /// Set test_setup enabled state
    pub fn set_test_setup(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::TestSetup, enabled);
    }

    /// Get type_guards enabled state
    pub fn type_guards(&self) -> bool {
        self.is_enabled(BoilerplateCategory::TypeGuard)
    }
    /// Set type_guards enabled state
    pub fn set_type_guards(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::TypeGuard, enabled);
    }

    /// Get api_routes enabled state
    pub fn api_routes(&self) -> bool {
        self.is_enabled(BoilerplateCategory::ApiRoute)
    }
    /// Set api_routes enabled state
    pub fn set_api_routes(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::ApiRoute, enabled);
    }

    /// Get config_export enabled state
    pub fn config_export(&self) -> bool {
        self.is_enabled(BoilerplateCategory::ConfigExport)
    }
    /// Set config_export enabled state
    pub fn set_config_export(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::ConfigExport, enabled);
    }

    /// Get rust_trait_impl enabled state
    pub fn rust_trait_impl(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustTraitImpl)
    }
    /// Set rust_trait_impl enabled state
    pub fn set_rust_trait_impl(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustTraitImpl, enabled);
    }

    /// Get rust_builder enabled state
    pub fn rust_builder(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustBuilder)
    }
    /// Set rust_builder enabled state
    pub fn set_rust_builder(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustBuilder, enabled);
    }

    /// Get rust_getter enabled state
    pub fn rust_getter(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustGetter)
    }
    /// Set rust_getter enabled state
    pub fn set_rust_getter(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustGetter, enabled);
    }

    /// Get rust_setter enabled state
    pub fn rust_setter(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustSetter)
    }
    /// Set rust_setter enabled state
    pub fn set_rust_setter(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustSetter, enabled);
    }

    /// Get rust_constructor enabled state
    pub fn rust_constructor(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustConstructor)
    }
    /// Set rust_constructor enabled state
    pub fn set_rust_constructor(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustConstructor, enabled);
    }

    /// Get rust_conversion enabled state
    pub fn rust_conversion(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustConversion)
    }
    /// Set rust_conversion enabled state
    pub fn set_rust_conversion(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustConversion, enabled);
    }

    /// Get rust_derived enabled state
    pub fn rust_derived(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustDerived)
    }
    /// Set rust_derived enabled state
    pub fn set_rust_derived(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustDerived, enabled);
    }

    /// Get rust_error_from enabled state
    pub fn rust_error_from(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustErrorFrom)
    }
    /// Set rust_error_from enabled state
    pub fn set_rust_error_from(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustErrorFrom, enabled);
    }

    /// Get rust_iterator enabled state
    pub fn rust_iterator(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustIterator)
    }
    /// Set rust_iterator enabled state
    pub fn set_rust_iterator(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustIterator, enabled);
    }

    /// Get rust_deref enabled state
    pub fn rust_deref(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustDeref)
    }
    /// Set rust_deref enabled state
    pub fn set_rust_deref(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustDeref, enabled);
    }

    /// Get rust_drop enabled state
    pub fn rust_drop(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustDrop)
    }
    /// Set rust_drop enabled state
    pub fn set_rust_drop(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustDrop, enabled);
    }

    /// Get rust_test enabled state
    pub fn rust_test(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustTest)
    }
    /// Set rust_test enabled state
    pub fn set_rust_test(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustTest, enabled);
    }

    /// Get rust_serde enabled state
    pub fn rust_serde(&self) -> bool {
        self.is_enabled(BoilerplateCategory::RustSerde)
    }
    /// Set rust_serde enabled state
    pub fn set_rust_serde(&mut self, enabled: bool) {
        self.set(BoilerplateCategory::RustSerde, enabled);
    }
}

/// Custom boilerplate rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomBoilerplateRule {
    /// Rule name (for identification)
    pub name: String,
    /// Name pattern (regex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
    /// File path pattern (glob)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,
    /// Maximum number of calls allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<usize>,
    /// Required calls (all must be present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_calls: Option<Vec<String>>,
    /// Required calls (any must be present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls_any: Option<Vec<String>>,
    /// Maximum control flow constructs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_control_flow: Option<usize>,
}

impl CustomBoilerplateRule {
    /// Check if a symbol matches this rule
    pub fn matches(&self, info: &SymbolInfo, file_path: Option<&str>) -> bool {
        // Check name pattern
        if let Some(pattern) = &self.name_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(&info.name) {
                    return false;
                }
            }
        }

        // Check file pattern
        if let Some(file_glob) = &self.file_pattern {
            if let Some(path) = file_path {
                if !matches_glob(file_glob, path) {
                    return false;
                }
            }
        }

        // Check max calls
        if let Some(max) = self.max_calls {
            if info.calls.len() > max {
                return false;
            }
        }

        // Check max control flow
        if let Some(max) = self.max_control_flow {
            if info.control_flow.len() > max {
                return false;
            }
        }

        // Check required calls (all must be present)
        if let Some(required) = &self.required_calls {
            let call_names: Vec<_> = info.calls.iter().map(|c| c.name.as_str()).collect();
            if !required.iter().all(|r| call_names.contains(&r.as_str())) {
                return false;
            }
        }

        // Check calls_any (at least one must be present)
        if let Some(any) = &self.calls_any {
            let call_names: Vec<_> = info.calls.iter().map(|c| c.name.as_str()).collect();
            if !any.iter().any(|r| call_names.contains(&r.as_str())) {
                return false;
            }
        }

        true
    }
}

/// Configuration for boilerplate detection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoilerplateConfig {
    /// Built-in pattern toggles
    #[serde(default)]
    pub builtin: BuiltinBoilerplate,
    /// Custom boilerplate rules
    #[serde(default)]
    pub custom: Vec<CustomBoilerplateRule>,
}

impl BoilerplateConfig {
    /// Create with all built-in patterns enabled
    pub fn all_enabled() -> Self {
        Self {
            builtin: BuiltinBoilerplate::all_enabled(),
            custom: Vec::new(),
        }
    }

    /// Create with all built-in patterns disabled
    pub fn all_disabled() -> Self {
        Self {
            builtin: BuiltinBoilerplate::all_disabled(),
            custom: Vec::new(),
        }
    }

    /// Add a custom rule
    pub fn add_custom_rule(&mut self, rule: CustomBoilerplateRule) {
        self.custom.push(rule);
    }
}

// =============================================================================
// Classification Functions
// =============================================================================

/// Classify a symbol as boilerplate if it matches patterns
///
/// Uses the registry pattern to check all registered patterns.
/// Returns the first matching category or None.
pub fn classify_boilerplate(
    info: &SymbolInfo,
    config: Option<&BoilerplateConfig>,
) -> Option<BoilerplateCategory> {
    let config = config.cloned().unwrap_or_default();

    // Check custom rules first (user takes precedence)
    for rule in &config.custom {
        if rule.matches(info, None) {
            return Some(BoilerplateCategory::Custom);
        }
    }

    // Check all registered patterns
    for pattern in all_patterns() {
        // Skip if this category is disabled
        if !config.builtin.is_enabled(pattern.category) {
            continue;
        }

        // Check if pattern matches
        if (pattern.detector)(info) {
            return Some(pattern.category);
        }
    }

    None
}

/// Classify boilerplate with file path context
pub fn classify_boilerplate_with_path(
    info: &SymbolInfo,
    file_path: &str,
    config: Option<&BoilerplateConfig>,
) -> Option<BoilerplateCategory> {
    let config = config.cloned().unwrap_or_default();

    // Check custom rules first with file path
    for rule in &config.custom {
        if rule.matches(info, Some(file_path)) {
            return Some(BoilerplateCategory::Custom);
        }
    }

    // Fall back to standard classification
    classify_boilerplate(info, Some(&config))
}

/// Classify with language hint for more accurate matching
pub fn classify_boilerplate_with_lang(
    info: &SymbolInfo,
    lang: Option<Lang>,
    config: Option<&BoilerplateConfig>,
) -> Option<BoilerplateCategory> {
    let config = config.cloned().unwrap_or_default();

    // Check custom rules first
    for rule in &config.custom {
        if rule.matches(info, None) {
            return Some(BoilerplateCategory::Custom);
        }
    }

    // Check patterns with language filtering
    for pattern in all_patterns() {
        // Skip if this category is disabled
        if !config.builtin.is_enabled(pattern.category) {
            continue;
        }

        // Skip if language doesn't match
        if !is_lang_compatible(lang, pattern.languages) {
            continue;
        }

        // Check if pattern matches
        if (pattern.detector)(info) {
            return Some(pattern.category);
        }
    }

    None
}

// =============================================================================
// Utilities
// =============================================================================

/// Simple glob matching (supports * and **)
pub fn matches_glob(pattern: &str, path: &str) -> bool {
    // Convert glob to regex
    let regex_pattern = pattern
        .replace('.', "\\.")
        .replace("**", "{{DOUBLESTAR}}")
        .replace('*', "[^/]*")
        .replace("{{DOUBLESTAR}}", ".*");

    if let Ok(re) = Regex::new(&format!("^{}$", regex_pattern)) {
        re.is_match(path)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Call, ControlFlowChange, ControlFlowKind, Location};

    pub fn make_symbol(name: &str, calls: Vec<&str>, control_flow: usize) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            calls: calls
                .into_iter()
                .map(|n| Call {
                    name: n.to_string(),
                    object: None,
                    is_awaited: false,
                    in_try: false,
                    is_hook: false,
                    is_io: false,
                    location: Location::default(),
                })
                .collect(),
            control_flow: (0..control_flow)
                .map(|_| ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                    nesting_depth: 0,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_glob_matching() {
        assert!(matches_glob("**/resolvers/**", "src/api/resolvers/user.ts"));
        assert!(matches_glob("*.ts", "file.ts"));
        assert!(!matches_glob("*.ts", "file.js"));
        assert!(matches_glob("src/**/*.tsx", "src/components/Button.tsx"));
    }

    #[test]
    fn test_custom_rule_name_pattern() {
        let rule = CustomBoilerplateRule {
            name: "redux_action".to_string(),
            name_pattern: Some("^(set|update|reset).*".to_string()),
            file_pattern: None,
            max_calls: Some(2),
            required_calls: None,
            calls_any: None,
            max_control_flow: None,
        };

        let symbol = make_symbol("setUserName", vec!["dispatch"], 0);
        assert!(rule.matches(&symbol, None));

        let symbol2 = make_symbol("getUserName", vec!["dispatch"], 0);
        assert!(!rule.matches(&symbol2, None));
    }

    #[test]
    fn test_builtin_boilerplate_default_all_enabled() {
        let builtin = BuiltinBoilerplate::default();
        assert!(builtin.is_enabled(BoilerplateCategory::ReactQuery));
        assert!(builtin.is_enabled(BoilerplateCategory::RustTraitImpl));
        assert!(builtin.is_enabled(BoilerplateCategory::RustTest));
    }

    #[test]
    fn test_builtin_boilerplate_disable() {
        let mut builtin = BuiltinBoilerplate::default();
        builtin.disable(BoilerplateCategory::RustTest);

        assert!(!builtin.is_enabled(BoilerplateCategory::RustTest));
        assert!(builtin.is_enabled(BoilerplateCategory::RustTraitImpl));
    }

    #[test]
    fn test_builtin_boilerplate_enable() {
        let mut builtin = BuiltinBoilerplate::all_disabled();
        builtin.enable(BoilerplateCategory::RustTest);

        assert!(builtin.is_enabled(BoilerplateCategory::RustTest));
    }

    #[test]
    fn test_lang_compatible_js() {
        // JS patterns should match TS/JSX/TSX
        let js_langs = &[Lang::JavaScript];

        assert!(is_lang_compatible(Some(Lang::JavaScript), js_langs));
        assert!(is_lang_compatible(Some(Lang::TypeScript), js_langs));
        assert!(is_lang_compatible(Some(Lang::Jsx), js_langs));
        assert!(is_lang_compatible(Some(Lang::Tsx), js_langs));
        assert!(!is_lang_compatible(Some(Lang::Rust), js_langs));
    }

    #[test]
    fn test_lang_compatible_rust() {
        let rust_langs = &[Lang::Rust];

        assert!(is_lang_compatible(Some(Lang::Rust), rust_langs));
        assert!(!is_lang_compatible(Some(Lang::JavaScript), rust_langs));
    }

    #[test]
    fn test_lang_compatible_empty() {
        // Empty pattern languages means all languages
        let empty: &[Lang] = &[];

        assert!(is_lang_compatible(Some(Lang::Rust), empty));
        assert!(is_lang_compatible(Some(Lang::JavaScript), empty));
        assert!(is_lang_compatible(None, empty));
    }

    #[test]
    fn test_all_patterns_collects_from_modules() {
        let patterns: Vec<_> = all_patterns().collect();

        // Should have patterns from both JS and Rust modules
        assert!(patterns.iter().any(|p| p.category == BoilerplateCategory::ReactQuery));
        assert!(patterns.iter().any(|p| p.category == BoilerplateCategory::RustTest));
    }
}
