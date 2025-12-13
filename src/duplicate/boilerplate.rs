//! Boilerplate detection and classification
//!
//! Functions are classified as "expected duplicates" based on patterns.
//! These are excluded from duplicate detection by default.

use crate::schema::SymbolInfo;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Category of boilerplate code
///
/// These patterns represent code that is commonly duplicated by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Built-in boilerplate pattern toggles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinBoilerplate {
    // JavaScript/TypeScript patterns
    pub react_query: bool,
    pub react_hooks: bool,
    pub event_handlers: bool,
    pub test_setup: bool,
    pub type_guards: bool,
    pub api_routes: bool,
    pub config_export: bool,
    // Rust patterns
    #[serde(default = "default_true")]
    pub rust_trait_impl: bool,
    #[serde(default = "default_true")]
    pub rust_builder: bool,
    #[serde(default = "default_true")]
    pub rust_getter: bool,
    #[serde(default = "default_true")]
    pub rust_setter: bool,
    #[serde(default = "default_true")]
    pub rust_constructor: bool,
    #[serde(default = "default_true")]
    pub rust_conversion: bool,
    #[serde(default = "default_true")]
    pub rust_derived: bool,
    #[serde(default = "default_true")]
    pub rust_error_from: bool,
    #[serde(default = "default_true")]
    pub rust_iterator: bool,
    #[serde(default = "default_true")]
    pub rust_deref: bool,
    #[serde(default = "default_true")]
    pub rust_drop: bool,
    #[serde(default = "default_true")]
    pub rust_test: bool,
    #[serde(default = "default_true")]
    pub rust_serde: bool,
}

fn default_true() -> bool {
    true
}

impl Default for BuiltinBoilerplate {
    fn default() -> Self {
        Self {
            // JavaScript/TypeScript
            react_query: true,
            react_hooks: true,
            event_handlers: true,
            test_setup: true,
            type_guards: true,
            api_routes: true,
            config_export: true,
            // Rust
            rust_trait_impl: true,
            rust_builder: true,
            rust_getter: true,
            rust_setter: true,
            rust_constructor: true,
            rust_conversion: true,
            rust_derived: true,
            rust_error_from: true,
            rust_iterator: true,
            rust_deref: true,
            rust_drop: true,
            rust_test: true,
            rust_serde: true,
        }
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
            builtin: BuiltinBoilerplate::default(),
            custom: Vec::new(),
        }
    }

    /// Create with all built-in patterns disabled
    pub fn all_disabled() -> Self {
        Self {
            builtin: BuiltinBoilerplate {
                // JavaScript/TypeScript
                react_query: false,
                react_hooks: false,
                event_handlers: false,
                test_setup: false,
                type_guards: false,
                api_routes: false,
                config_export: false,
                // Rust
                rust_trait_impl: false,
                rust_builder: false,
                rust_getter: false,
                rust_setter: false,
                rust_constructor: false,
                rust_conversion: false,
                rust_derived: false,
                rust_error_from: false,
                rust_iterator: false,
                rust_deref: false,
                rust_drop: false,
                rust_test: false,
                rust_serde: false,
            },
            custom: Vec::new(),
        }
    }

    /// Add a custom rule
    pub fn add_custom_rule(&mut self, rule: CustomBoilerplateRule) {
        self.custom.push(rule);
    }
}

/// Classify a symbol as boilerplate if it matches patterns
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

    // Check built-in patterns
    if config.builtin.react_query && is_react_query(info) {
        return Some(BoilerplateCategory::ReactQuery);
    }

    if config.builtin.react_hooks && is_react_hook_wrapper(info) {
        return Some(BoilerplateCategory::ReactHook);
    }

    if config.builtin.event_handlers && is_event_handler(info) {
        return Some(BoilerplateCategory::EventHandler);
    }

    if config.builtin.test_setup && is_test_setup(info) {
        return Some(BoilerplateCategory::TestSetup);
    }

    if config.builtin.type_guards && is_type_guard(info) {
        return Some(BoilerplateCategory::TypeGuard);
    }

    if config.builtin.api_routes && is_api_route(info) {
        return Some(BoilerplateCategory::ApiRoute);
    }

    if config.builtin.config_export && is_config_export(info) {
        return Some(BoilerplateCategory::ConfigExport);
    }

    // =========================================================================
    // Rust Patterns
    // =========================================================================

    if config.builtin.rust_test && is_rust_test(info) {
        return Some(BoilerplateCategory::RustTest);
    }

    if config.builtin.rust_trait_impl && is_rust_trait_impl(info) {
        return Some(BoilerplateCategory::RustTraitImpl);
    }

    if config.builtin.rust_builder && is_rust_builder(info) {
        return Some(BoilerplateCategory::RustBuilder);
    }

    if config.builtin.rust_getter && is_rust_getter(info) {
        return Some(BoilerplateCategory::RustGetter);
    }

    if config.builtin.rust_setter && is_rust_setter(info) {
        return Some(BoilerplateCategory::RustSetter);
    }

    if config.builtin.rust_constructor && is_rust_constructor(info) {
        return Some(BoilerplateCategory::RustConstructor);
    }

    if config.builtin.rust_conversion && is_rust_conversion(info) {
        return Some(BoilerplateCategory::RustConversion);
    }

    if config.builtin.rust_derived && is_rust_derived(info) {
        return Some(BoilerplateCategory::RustDerived);
    }

    if config.builtin.rust_error_from && is_rust_error_from(info) {
        return Some(BoilerplateCategory::RustErrorFrom);
    }

    if config.builtin.rust_iterator && is_rust_iterator(info) {
        return Some(BoilerplateCategory::RustIterator);
    }

    if config.builtin.rust_deref && is_rust_deref(info) {
        return Some(BoilerplateCategory::RustDeref);
    }

    if config.builtin.rust_drop && is_rust_drop(info) {
        return Some(BoilerplateCategory::RustDrop);
    }

    if config.builtin.rust_serde && is_rust_serde(info) {
        return Some(BoilerplateCategory::RustSerde);
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

// =============================================================================
// Built-in Pattern Detection
// =============================================================================

/// React Query: useQuery/useMutation with minimal other logic
fn is_react_query(info: &SymbolInfo) -> bool {
    let query_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "useQuery" | "useMutation" | "useQueryClient" | "useSuspenseQuery"
                    | "useInfiniteQuery" | "usePrefetchQuery"
            )
        })
        .collect();

    if query_calls.is_empty() {
        return false;
    }

    // Must have query calls and minimal other logic (query calls + 2 max)
    info.calls.len() <= query_calls.len() + 2 && info.control_flow.len() <= 1
}

/// React hook wrapper: custom hook with useState/useEffect
fn is_react_hook_wrapper(info: &SymbolInfo) -> bool {
    // Name must start with "use"
    if !info.name.starts_with("use") {
        return false;
    }

    let hook_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| c.name.starts_with("use"))
        .collect();

    // Must have React hooks and minimal other logic
    !hook_calls.is_empty() && info.calls.len() <= hook_calls.len() + 3 && info.control_flow.len() <= 2
}

/// Event handler: handle*/on* with minimal calls
fn is_event_handler(info: &SymbolInfo) -> bool {
    let name_lower = info.name.to_lowercase();

    // Name pattern
    let is_handler_name =
        name_lower.starts_with("handle") || name_lower.starts_with("on") && info.name.len() > 2;

    if !is_handler_name {
        return false;
    }

    // Minimal calls (2 or fewer)
    info.calls.len() <= 2 && info.control_flow.len() <= 1
}

/// Test setup: beforeEach, afterEach, setup, teardown
fn is_test_setup(info: &SymbolInfo) -> bool {
    matches!(
        info.name.as_str(),
        "beforeEach"
            | "afterEach"
            | "beforeAll"
            | "afterAll"
            | "setup"
            | "teardown"
            | "setUp"
            | "tearDown"
    )
}

/// Type guard: isX() with single type check
fn is_type_guard(info: &SymbolInfo) -> bool {
    // Name starts with "is" and is short
    if !info.name.starts_with("is") || info.name.len() < 3 {
        return false;
    }

    // Minimal logic: few control flow, few calls
    info.control_flow.len() <= 1 && info.calls.len() <= 1
}

/// API route: Express/Next.js route handlers
fn is_api_route(info: &SymbolInfo) -> bool {
    // Check for common API patterns
    let api_methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];
    if api_methods.iter().any(|m| info.name.contains(m)) {
        return true;
    }

    // Check for route handler calls
    let route_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "send" | "json" | "status" | "redirect" | "render" | "NextResponse"
            )
        })
        .collect();

    // Must have route calls and minimal control flow
    !route_calls.is_empty() && info.control_flow.len() <= 2
}

/// Config/export: module.exports patterns
fn is_config_export(info: &SymbolInfo) -> bool {
    // Config names
    let config_names = [
        "config",
        "options",
        "settings",
        "defaults",
        "configuration",
    ];

    let name_lower = info.name.to_lowercase();

    // Must be a config-like name
    if !config_names.iter().any(|c| name_lower.contains(c)) {
        return false;
    }

    // Minimal logic
    info.calls.is_empty() && info.control_flow.is_empty()
}

// =============================================================================
// Rust Pattern Detection
// =============================================================================

/// Rust test function: test_* or functions with #[test]
fn is_rust_test(info: &SymbolInfo) -> bool {
    // Name starts with test_
    if info.name.starts_with("test_") {
        return true;
    }

    // Common test helper patterns
    if matches!(
        info.name.as_str(),
        "setup" | "teardown" | "setup_test" | "teardown_test" | "init_test" | "cleanup_test"
    ) {
        return true;
    }

    false
}

/// Rust trait implementation: common trait methods
fn is_rust_trait_impl(info: &SymbolInfo) -> bool {
    // Standard library trait implementations
    let trait_methods = [
        // Display/Debug
        "fmt",
        // Default
        "default",
        // Clone
        "clone",
        "clone_from",
        // PartialEq/Eq
        "eq",
        "ne",
        // PartialOrd/Ord
        "partial_cmp",
        "cmp",
        "lt",
        "le",
        "gt",
        "ge",
        // Hash
        "hash",
        // AsRef/AsMut
        "as_ref",
        "as_mut",
        // Borrow/BorrowMut
        "borrow",
        "borrow_mut",
        // From/Into
        "from",
        "into",
        // TryFrom/TryInto
        "try_from",
        "try_into",
        // FromStr
        "from_str",
        // ToString
        "to_string",
        // Error
        "source",
        "description",
        "cause",
        // Index/IndexMut
        "index",
        "index_mut",
        // Add/Sub/Mul/Div etc (operator overloading)
        "add",
        "sub",
        "mul",
        "div",
        "rem",
        "neg",
        "not",
        "bitand",
        "bitor",
        "bitxor",
        "shl",
        "shr",
    ];

    // Check if it's a trait method name AND has low complexity
    // Note: to_string is excluded here as it's classified as conversion
    if !trait_methods.contains(&info.name.as_str()) {
        return false;
    }

    // Skip to_string - it's a conversion pattern
    if info.name == "to_string" {
        return false;
    }

    // Trait implementations should be simple
    info.control_flow.len() <= 2 && info.calls.len() <= 4
}

/// Rust builder pattern: with_* or set_* methods that return Self
fn is_rust_builder(info: &SymbolInfo) -> bool {
    let name = &info.name;

    // Builder patterns
    if name.starts_with("with_")
        || name.starts_with("set_")
        || name == "builder"
        || name == "build"
    {
        // Should have minimal logic - typically just field assignment
        return info.control_flow.len() <= 1 && info.calls.len() <= 3;
    }

    false
}

/// Rust getter: get_*, is_*, has_*, contains_* methods
fn is_rust_getter(info: &SymbolInfo) -> bool {
    let name = &info.name;

    // Getter patterns
    let is_getter_name = name.starts_with("get_")
        || name.starts_with("is_")
        || name.starts_with("has_")
        || name.starts_with("can_")
        || name.starts_with("should_")
        || name.starts_with("was_")
        || name.starts_with("will_")
        || name.starts_with("did_")
        || name.starts_with("contains_")
        || name.starts_with("exists_")
        || name.starts_with("needs_");

    if !is_getter_name {
        return false;
    }

    // Getters should have minimal logic
    info.control_flow.len() <= 1 && info.calls.len() <= 2
}

/// Rust setter: set_* methods (not builder pattern - no return)
fn is_rust_setter(info: &SymbolInfo) -> bool {
    if !info.name.starts_with("set_") {
        return false;
    }

    // Setters should have minimal logic - just assignment, maybe with simple validation
    info.control_flow.len() <= 1 && info.calls.len() <= 2
}

/// Rust constructor: new, default, from_*, try_from_*, with_*
fn is_rust_constructor(info: &SymbolInfo) -> bool {
    let name = &info.name;

    // Constructor patterns
    if name == "new"
        || name == "default"
        || name.starts_with("from_")
        || name.starts_with("try_from_")
        || name.starts_with("create_")
        || name.starts_with("make_")
        || name.starts_with("init_")
        || name.starts_with("with_")
        || name == "create"
        || name == "init"
        || name == "open"
        || name == "connect"
    {
        // Constructors can have moderate logic but shouldn't be too complex
        return info.control_flow.len() <= 2 && info.calls.len() <= 4;
    }

    false
}

/// Rust conversion: to_*, as_*, into_* methods
fn is_rust_conversion(info: &SymbolInfo) -> bool {
    let name = &info.name;

    // Conversion patterns
    if name.starts_with("to_")
        || name.starts_with("as_")
        || name.starts_with("into_")
        || name.starts_with("try_to_")
        || name.starts_with("try_as_")
        || name.starts_with("try_into_")
    {
        // Conversions should be simple
        return info.control_flow.len() <= 2 && info.calls.len() <= 3;
    }

    false
}

/// Rust derived: methods that look like derive-generated code
fn is_rust_derived(info: &SymbolInfo) -> bool {
    // Derive-generated methods are typically exact names
    matches!(
        info.name.as_str(),
        "clone" | "default" | "eq" | "ne" | "hash" | "cmp" | "partial_cmp" | "fmt"
    ) && info.control_flow.len() <= 1
        && info.calls.len() <= 2
}

/// Rust Error From implementation: from method with error conversion
fn is_rust_error_from(info: &SymbolInfo) -> bool {
    // From implementations typically named "from"
    if info.name != "from" {
        return false;
    }

    // Error conversions are usually simple wrappers
    info.control_flow.is_empty() && info.calls.len() <= 2
}

/// Rust iterator: next, into_iter, iter, iter_mut
fn is_rust_iterator(info: &SymbolInfo) -> bool {
    let iterator_methods = [
        "next",
        "into_iter",
        "iter",
        "iter_mut",
        "size_hint",
        "count",
        "last",
        "nth",
        "fold",
        "for_each",
        "collect",
        "partition",
        "all",
        "any",
        "find",
        "position",
        "max",
        "min",
        "sum",
        "product",
    ];

    // Check name matches and complexity is reasonable
    iterator_methods.contains(&info.name.as_str())
        && info.control_flow.len() <= 2
        && info.calls.len() <= 4
}

/// Rust Deref: deref and deref_mut
fn is_rust_deref(info: &SymbolInfo) -> bool {
    matches!(info.name.as_str(), "deref" | "deref_mut") && info.control_flow.is_empty()
}

/// Rust Drop: drop method
fn is_rust_drop(info: &SymbolInfo) -> bool {
    info.name == "drop" && info.control_flow.len() <= 1
}

/// Rust serde helpers: serialize_*, deserialize_*, with_*
fn is_rust_serde(info: &SymbolInfo) -> bool {
    let name = &info.name;

    // Serde patterns
    if name.starts_with("serialize_")
        || name.starts_with("deserialize_")
        || name == "serialize"
        || name == "deserialize"
        || name.starts_with("visit_")
        || name == "expecting"
    {
        return true;
    }

    // Serde attribute helpers
    if name.starts_with("default_")
        || name.starts_with("skip_")
        || name.starts_with("rename_")
    {
        return info.control_flow.is_empty() && info.calls.len() <= 1;
    }

    false
}

// =============================================================================
// Utilities
// =============================================================================

/// Simple glob matching (supports * and **)
fn matches_glob(pattern: &str, path: &str) -> bool {
    // Convert glob to regex
    let regex_pattern = pattern
        .replace(".", "\\.")
        .replace("**", "{{DOUBLESTAR}}")
        .replace("*", "[^/]*")
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

    fn make_symbol(name: &str, calls: Vec<&str>, control_flow: usize) -> SymbolInfo {
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
    fn test_react_query_detection() {
        let symbol = make_symbol("useUserData", vec!["useQuery", "console.log"], 0);
        assert!(is_react_query(&symbol));
    }

    #[test]
    fn test_react_query_too_many_calls() {
        let symbol = make_symbol(
            "useUserData",
            vec!["useQuery", "fetch", "process", "validate", "transform"],
            0,
        );
        assert!(!is_react_query(&symbol));
    }

    #[test]
    fn test_event_handler_detection() {
        let symbol = make_symbol("handleClick", vec!["setState"], 0);
        assert!(is_event_handler(&symbol));
    }

    #[test]
    fn test_event_handler_too_complex() {
        let symbol = make_symbol("handleClick", vec!["validate", "fetch", "process"], 2);
        assert!(!is_event_handler(&symbol));
    }

    #[test]
    fn test_type_guard_detection() {
        let symbol = make_symbol("isString", vec![], 1);
        assert!(is_type_guard(&symbol));
    }

    #[test]
    fn test_test_setup_detection() {
        let symbol = make_symbol("beforeEach", vec!["mockDb", "seedData"], 0);
        assert!(is_test_setup(&symbol));
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
    fn test_glob_matching() {
        assert!(matches_glob("**/resolvers/**", "src/api/resolvers/user.ts"));
        assert!(matches_glob("*.ts", "file.ts"));
        assert!(!matches_glob("*.ts", "file.js"));
        assert!(matches_glob("src/**/*.tsx", "src/components/Button.tsx"));
    }

    // =========================================================================
    // Rust Boilerplate Tests
    // =========================================================================

    #[test]
    fn test_rust_test_detection() {
        // Basic test function
        let symbol = make_symbol("test_user_creation", vec!["create_user", "assert_eq"], 0);
        assert!(is_rust_test(&symbol));

        // Test with setup
        let symbol2 = make_symbol("test_handles_errors", vec!["setup", "run", "assert"], 1);
        assert!(is_rust_test(&symbol2));

        // Non-test function (no test_ prefix)
        let symbol3 = make_symbol("user_creation", vec!["create_user"], 0);
        assert!(!is_rust_test(&symbol3));
    }

    #[test]
    fn test_rust_trait_impl_fmt() {
        // Display/Debug fmt implementation
        let symbol = make_symbol("fmt", vec!["write!", "f.write_str"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_clone() {
        let symbol = make_symbol("clone", vec!["clone"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_eq() {
        let symbol = make_symbol("eq", vec![], 1);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_hash() {
        let symbol = make_symbol("hash", vec!["state.write", "hash"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_from() {
        let symbol = make_symbol("from", vec!["new"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_into() {
        let symbol = make_symbol("into", vec![], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_try_from() {
        let symbol = make_symbol("try_from", vec!["validate"], 1);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_try_into() {
        let symbol = make_symbol("try_into", vec![], 1);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_default() {
        let symbol = make_symbol("default", vec![], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_cmp() {
        let symbol = make_symbol("cmp", vec!["cmp"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_partial_cmp() {
        let symbol = make_symbol("partial_cmp", vec!["cmp"], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_description() {
        // Error trait description() method
        let symbol = make_symbol("description", vec![], 0);
        assert!(is_rust_trait_impl(&symbol));
    }

    #[test]
    fn test_rust_trait_impl_not_matched() {
        // Too many calls for simple trait impl
        let symbol = make_symbol(
            "fmt",
            vec!["validate", "process", "transform", "encode", "write"],
            3,
        );
        assert!(!is_rust_trait_impl(&symbol));

        // Not a trait method name
        let symbol2 = make_symbol("process_data", vec!["run"], 0);
        assert!(!is_rust_trait_impl(&symbol2));
    }

    #[test]
    fn test_rust_builder_with_prefix() {
        let symbol = make_symbol("with_name", vec!["self"], 0);
        assert!(is_rust_builder(&symbol));

        let symbol2 = make_symbol("with_config", vec![], 0);
        assert!(is_rust_builder(&symbol2));

        let symbol3 = make_symbol("with_capacity", vec!["Vec::with_capacity"], 0);
        assert!(is_rust_builder(&symbol3));
    }

    #[test]
    fn test_rust_builder_builder_methods() {
        let symbol = make_symbol("builder", vec!["Default::default"], 0);
        assert!(is_rust_builder(&symbol));

        let symbol2 = make_symbol("build", vec!["validate", "construct"], 0);
        assert!(is_rust_builder(&symbol2));
    }

    #[test]
    fn test_rust_builder_set_prefix() {
        // set_* can be builder in builder pattern context
        let symbol = make_symbol("set_name", vec!["self"], 0);
        assert!(is_rust_builder(&symbol));
    }

    #[test]
    fn test_rust_builder_too_complex() {
        // Too complex for builder pattern
        let symbol = make_symbol(
            "with_validation",
            vec!["validate", "process", "transform", "encode"],
            2,
        );
        assert!(!is_rust_builder(&symbol));
    }

    #[test]
    fn test_rust_getter_get_prefix() {
        let symbol = make_symbol("get_name", vec![], 0);
        assert!(is_rust_getter(&symbol));

        let symbol2 = make_symbol("get_user_id", vec!["clone"], 0);
        assert!(is_rust_getter(&symbol2));
    }

    #[test]
    fn test_rust_getter_is_prefix() {
        let symbol = make_symbol("is_empty", vec![], 0);
        assert!(is_rust_getter(&symbol));

        let symbol2 = make_symbol("is_valid", vec![], 1);
        assert!(is_rust_getter(&symbol2));
    }

    #[test]
    fn test_rust_getter_has_prefix() {
        let symbol = make_symbol("has_children", vec!["len"], 0);
        assert!(is_rust_getter(&symbol));
    }

    #[test]
    fn test_rust_getter_can_prefix() {
        let symbol = make_symbol("can_read", vec![], 1);
        assert!(is_rust_getter(&symbol));
    }

    #[test]
    fn test_rust_getter_should_prefix() {
        let symbol = make_symbol("should_retry", vec![], 1);
        assert!(is_rust_getter(&symbol));
    }

    #[test]
    fn test_rust_getter_contains_prefix() {
        let symbol = make_symbol("contains_key", vec!["get"], 0);
        assert!(is_rust_getter(&symbol));
    }

    #[test]
    fn test_rust_getter_too_complex() {
        let symbol = make_symbol(
            "get_computed_value",
            vec!["fetch", "parse", "validate", "transform"],
            2,
        );
        assert!(!is_rust_getter(&symbol));
    }

    #[test]
    fn test_rust_setter_basic() {
        let symbol = make_symbol("set_name", vec![], 0);
        assert!(is_rust_setter(&symbol));

        let symbol2 = make_symbol("set_user_id", vec![], 0);
        assert!(is_rust_setter(&symbol2));
    }

    #[test]
    fn test_rust_setter_with_validation() {
        // Simple validation is ok
        let symbol = make_symbol("set_age", vec!["validate"], 1);
        assert!(is_rust_setter(&symbol));
    }

    #[test]
    fn test_rust_setter_too_complex() {
        let symbol = make_symbol(
            "set_config",
            vec!["parse", "validate", "transform", "persist"],
            2,
        );
        assert!(!is_rust_setter(&symbol));
    }

    #[test]
    fn test_rust_setter_not_setter() {
        // get_* is not a setter
        let symbol = make_symbol("get_name", vec![], 0);
        assert!(!is_rust_setter(&symbol));
    }

    #[test]
    fn test_rust_constructor_new() {
        let symbol = make_symbol("new", vec!["Default::default"], 0);
        assert!(is_rust_constructor(&symbol));

        let symbol2 = make_symbol("new", vec!["init"], 0);
        assert!(is_rust_constructor(&symbol2));
    }

    #[test]
    fn test_rust_constructor_default() {
        let symbol = make_symbol("default", vec![], 0);
        assert!(is_rust_constructor(&symbol));
    }

    #[test]
    fn test_rust_constructor_from_prefix() {
        let symbol = make_symbol("from_str", vec!["parse"], 1);
        assert!(is_rust_constructor(&symbol));

        let symbol2 = make_symbol("from_bytes", vec!["decode"], 0);
        assert!(is_rust_constructor(&symbol2));

        let symbol3 = make_symbol("from_parts", vec![], 0);
        assert!(is_rust_constructor(&symbol3));
    }

    #[test]
    fn test_rust_constructor_create_prefix() {
        let symbol = make_symbol("create_instance", vec!["allocate"], 0);
        assert!(is_rust_constructor(&symbol));
    }

    #[test]
    fn test_rust_constructor_init_prefix() {
        let symbol = make_symbol("init", vec!["setup"], 0);
        assert!(is_rust_constructor(&symbol));

        let symbol2 = make_symbol("init_with_config", vec!["load_config"], 0);
        assert!(is_rust_constructor(&symbol2));
    }

    #[test]
    fn test_rust_constructor_make_prefix() {
        let symbol = make_symbol("make_server", vec!["bind"], 0);
        assert!(is_rust_constructor(&symbol));
    }

    #[test]
    fn test_rust_constructor_too_complex() {
        let symbol = make_symbol(
            "new",
            vec!["validate", "setup", "connect", "authenticate", "configure"],
            3,
        );
        assert!(!is_rust_constructor(&symbol));
    }

    #[test]
    fn test_rust_conversion_to_prefix() {
        let symbol = make_symbol("to_string", vec!["format!"], 0);
        assert!(is_rust_conversion(&symbol));

        let symbol2 = make_symbol("to_vec", vec!["clone"], 0);
        assert!(is_rust_conversion(&symbol2));

        let symbol3 = make_symbol("to_owned", vec![], 0);
        assert!(is_rust_conversion(&symbol3));
    }

    #[test]
    fn test_rust_conversion_as_prefix() {
        let symbol = make_symbol("as_ref", vec![], 0);
        assert!(is_rust_conversion(&symbol));

        let symbol2 = make_symbol("as_slice", vec![], 0);
        assert!(is_rust_conversion(&symbol2));

        let symbol3 = make_symbol("as_bytes", vec![], 0);
        assert!(is_rust_conversion(&symbol3));
    }

    #[test]
    fn test_rust_conversion_into_prefix() {
        let symbol = make_symbol("into_inner", vec![], 0);
        assert!(is_rust_conversion(&symbol));

        let symbol2 = make_symbol("into_vec", vec![], 0);
        assert!(is_rust_conversion(&symbol2));
    }

    #[test]
    fn test_rust_conversion_too_complex() {
        let symbol = make_symbol(
            "to_json",
            vec!["serialize", "validate", "transform", "encode"],
            2,
        );
        assert!(!is_rust_conversion(&symbol));
    }

    #[test]
    fn test_rust_derived_clone() {
        let symbol = make_symbol("clone", vec!["clone"], 0);
        assert!(is_rust_derived(&symbol));
    }

    #[test]
    fn test_rust_derived_default() {
        let symbol = make_symbol("default", vec![], 0);
        assert!(is_rust_derived(&symbol));
    }

    #[test]
    fn test_rust_derived_with_simple_call() {
        let symbol = make_symbol("clone", vec!["clone", "clone"], 0);
        assert!(is_rust_derived(&symbol));
    }

    #[test]
    fn test_rust_derived_not_derived() {
        // Too many calls
        let symbol = make_symbol("clone", vec!["validate", "transform", "deep_clone"], 1);
        assert!(!is_rust_derived(&symbol));

        // Not a derived method name
        let symbol2 = make_symbol("process", vec![], 0);
        assert!(!is_rust_derived(&symbol2));
    }

    #[test]
    fn test_rust_error_from() {
        let symbol = make_symbol("from", vec!["new", "Error::new"], 0);
        assert!(is_rust_error_from(&symbol));

        let symbol2 = make_symbol("from", vec!["into"], 0);
        assert!(is_rust_error_from(&symbol2));
    }

    #[test]
    fn test_rust_error_from_too_complex() {
        let symbol = make_symbol(
            "from",
            vec!["validate", "parse", "transform", "wrap_error"],
            2,
        );
        assert!(!is_rust_error_from(&symbol));
    }

    #[test]
    fn test_rust_iterator_next() {
        let symbol = make_symbol("next", vec!["next"], 1);
        assert!(is_rust_iterator(&symbol));

        let symbol2 = make_symbol("next", vec![], 1);
        assert!(is_rust_iterator(&symbol2));
    }

    #[test]
    fn test_rust_iterator_into_iter() {
        let symbol = make_symbol("into_iter", vec!["IntoIterator::into_iter"], 0);
        assert!(is_rust_iterator(&symbol));
    }

    #[test]
    fn test_rust_iterator_iter() {
        let symbol = make_symbol("iter", vec![], 0);
        assert!(is_rust_iterator(&symbol));
    }

    #[test]
    fn test_rust_iterator_iter_mut() {
        let symbol = make_symbol("iter_mut", vec![], 0);
        assert!(is_rust_iterator(&symbol));
    }

    #[test]
    fn test_rust_iterator_too_complex() {
        let symbol = make_symbol("next", vec!["fetch", "parse", "transform", "cache", "emit"], 3);
        assert!(!is_rust_iterator(&symbol));
    }

    #[test]
    fn test_rust_deref() {
        let symbol = make_symbol("deref", vec![], 0);
        assert!(is_rust_deref(&symbol));
    }

    #[test]
    fn test_rust_deref_mut() {
        let symbol = make_symbol("deref_mut", vec![], 0);
        assert!(is_rust_deref(&symbol));
    }

    #[test]
    fn test_rust_deref_not_matched() {
        // Not deref/deref_mut
        let symbol = make_symbol("dereference", vec![], 0);
        assert!(!is_rust_deref(&symbol));
    }

    #[test]
    fn test_rust_drop() {
        let symbol = make_symbol("drop", vec!["close", "cleanup"], 0);
        assert!(is_rust_drop(&symbol));
    }

    #[test]
    fn test_rust_drop_simple() {
        let symbol = make_symbol("drop", vec![], 0);
        assert!(is_rust_drop(&symbol));
    }

    #[test]
    fn test_rust_drop_not_matched() {
        // Not "drop" exactly
        let symbol = make_symbol("drop_all", vec![], 0);
        assert!(!is_rust_drop(&symbol));
    }

    #[test]
    fn test_rust_serde_serialize() {
        let symbol = make_symbol("serialize", vec!["serializer.serialize_struct"], 0);
        assert!(is_rust_serde(&symbol));

        let symbol2 = make_symbol("serialize_field", vec!["write"], 0);
        assert!(is_rust_serde(&symbol2));
    }

    #[test]
    fn test_rust_serde_deserialize() {
        let symbol = make_symbol("deserialize", vec!["deserializer.deserialize_struct"], 1);
        assert!(is_rust_serde(&symbol));

        let symbol2 = make_symbol("deserialize_field", vec!["read"], 0);
        assert!(is_rust_serde(&symbol2));
    }

    #[test]
    fn test_rust_serde_visitor() {
        let symbol = make_symbol("visit_str", vec!["from_str"], 1);
        assert!(is_rust_serde(&symbol));

        let symbol2 = make_symbol("visit_map", vec!["next_key", "next_value"], 1);
        assert!(is_rust_serde(&symbol2));

        let symbol3 = make_symbol("visit_seq", vec!["next_element"], 1);
        assert!(is_rust_serde(&symbol3));
    }

    #[test]
    fn test_rust_serde_expecting() {
        let symbol = make_symbol("expecting", vec!["write_str"], 0);
        assert!(is_rust_serde(&symbol));
    }

    #[test]
    fn test_rust_serde_not_matched() {
        // Not a serde method
        let symbol = make_symbol("process_data", vec!["parse"], 0);
        assert!(!is_rust_serde(&symbol));
    }

    // =========================================================================
    // Integration Tests for Rust Classification
    // =========================================================================

    #[test]
    fn test_classify_rust_boilerplate_all_types() {
        // Use default config (all patterns enabled)
        // Test function
        let test = make_symbol("test_something", vec!["assert"], 0);
        assert_eq!(
            classify_boilerplate(&test, None),
            Some(BoilerplateCategory::RustTest)
        );

        // Trait impl
        let fmt = make_symbol("fmt", vec!["write!"], 0);
        assert_eq!(
            classify_boilerplate(&fmt, None),
            Some(BoilerplateCategory::RustTraitImpl)
        );

        // Builder
        let with_name = make_symbol("with_name", vec![], 0);
        assert_eq!(
            classify_boilerplate(&with_name, None),
            Some(BoilerplateCategory::RustBuilder)
        );

        // Getter
        let get_name = make_symbol("get_name", vec![], 0);
        assert_eq!(
            classify_boilerplate(&get_name, None),
            Some(BoilerplateCategory::RustGetter)
        );

        // Setter
        let set_name = make_symbol("set_name", vec![], 0);
        // Note: set_name matches both RustBuilder and RustSetter, builder is checked first
        let result = classify_boilerplate(&set_name, None);
        assert!(
            result == Some(BoilerplateCategory::RustBuilder)
                || result == Some(BoilerplateCategory::RustSetter)
        );

        // Constructor
        let new = make_symbol("new", vec![], 0);
        assert_eq!(
            classify_boilerplate(&new, None),
            Some(BoilerplateCategory::RustConstructor)
        );

        // Conversion
        let to_string = make_symbol("to_string", vec![], 0);
        assert_eq!(
            classify_boilerplate(&to_string, None),
            Some(BoilerplateCategory::RustConversion)
        );

        // Iterator
        let next = make_symbol("next", vec![], 1);
        assert_eq!(
            classify_boilerplate(&next, None),
            Some(BoilerplateCategory::RustIterator)
        );

        // Deref
        let deref = make_symbol("deref", vec![], 0);
        assert_eq!(
            classify_boilerplate(&deref, None),
            Some(BoilerplateCategory::RustDeref)
        );

        // Drop
        let drop = make_symbol("drop", vec![], 0);
        assert_eq!(
            classify_boilerplate(&drop, None),
            Some(BoilerplateCategory::RustDrop)
        );

        // Serde
        let serialize = make_symbol("serialize", vec![], 0);
        assert_eq!(
            classify_boilerplate(&serialize, None),
            Some(BoilerplateCategory::RustSerde)
        );
    }

    #[test]
    fn test_rust_boilerplate_disabled() {
        // Create config with some Rust patterns disabled
        let mut builtin = BuiltinBoilerplate::default();
        builtin.rust_test = false;
        builtin.rust_trait_impl = false;
        builtin.rust_builder = false;
        builtin.rust_getter = false;

        let config = BoilerplateConfig {
            builtin,
            custom: vec![],
        };

        // Should not match when disabled
        let test = make_symbol("test_something", vec!["assert"], 0);
        assert_ne!(
            classify_boilerplate(&test, Some(&config)),
            Some(BoilerplateCategory::RustTest)
        );

        let fmt = make_symbol("fmt", vec!["write!"], 0);
        assert_ne!(
            classify_boilerplate(&fmt, Some(&config)),
            Some(BoilerplateCategory::RustTraitImpl)
        );
    }
}
