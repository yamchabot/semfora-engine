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
    /// Custom user-defined boilerplate category
    Custom,
}

impl BoilerplateCategory {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            BoilerplateCategory::ReactQuery => "React Query hook pattern",
            BoilerplateCategory::ReactHook => "React hook wrapper",
            BoilerplateCategory::EventHandler => "Event handler with minimal logic",
            BoilerplateCategory::ApiRoute => "API route handler",
            BoilerplateCategory::TestSetup => "Test setup/teardown function",
            BoilerplateCategory::TypeGuard => "Type guard function",
            BoilerplateCategory::ConfigExport => "Config/export boilerplate",
            BoilerplateCategory::Custom => "Custom boilerplate pattern",
        }
    }
}

/// Built-in boilerplate pattern toggles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinBoilerplate {
    pub react_query: bool,
    pub react_hooks: bool,
    pub event_handlers: bool,
    pub test_setup: bool,
    pub type_guards: bool,
    pub api_routes: bool,
    pub config_export: bool,
}

impl Default for BuiltinBoilerplate {
    fn default() -> Self {
        Self {
            react_query: true,
            react_hooks: true,
            event_handlers: true,
            test_setup: true,
            type_guards: true,
            api_routes: true,
            config_export: true,
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
                react_query: false,
                react_hooks: false,
                event_handlers: false,
                test_setup: false,
                type_guards: false,
                api_routes: false,
                config_export: false,
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
    use crate::schema::{Call, ControlFlowChange, ControlFlowKind, Location, RiskLevel};

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
}
