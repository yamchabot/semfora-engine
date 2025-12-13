//! JavaScript/TypeScript boilerplate pattern detection
//!
//! This module contains detection functions for common JavaScript and TypeScript
//! boilerplate patterns that should be excluded from duplicate detection.
//!
//! # Patterns Detected
//!
//! - **React Query**: useQuery/useMutation with minimal logic
//! - **React Hooks**: Custom hooks wrapping useState/useEffect
//! - **Event Handlers**: handleClick, onChange with minimal calls
//! - **API Routes**: Express/Next.js route handlers
//! - **Test Setup**: beforeEach, afterEach, setup, teardown
//! - **Type Guards**: isX() type checking functions
//! - **Config Export**: module.exports patterns

use super::{BoilerplateCategory, PatternMatcher};
use crate::lang::Lang;
use crate::schema::SymbolInfo;

/// All JavaScript/TypeScript boilerplate patterns
pub static PATTERNS: &[PatternMatcher] = &[
    PatternMatcher {
        category: BoilerplateCategory::ReactQuery,
        languages: &[Lang::JavaScript],
        detector: is_react_query,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::ReactHook,
        languages: &[Lang::JavaScript],
        detector: is_react_hook_wrapper,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::EventHandler,
        languages: &[Lang::JavaScript],
        detector: is_event_handler,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::TestSetup,
        languages: &[Lang::JavaScript],
        detector: is_test_setup,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::TypeGuard,
        languages: &[Lang::JavaScript],
        detector: is_type_guard,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::ApiRoute,
        languages: &[Lang::JavaScript],
        detector: is_api_route,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::ConfigExport,
        languages: &[Lang::JavaScript],
        detector: is_config_export,
        enabled_by_default: true,
    },
];

// =============================================================================
// Detection Functions
// =============================================================================

/// React Query: useQuery/useMutation with minimal other logic
pub fn is_react_query(info: &SymbolInfo) -> bool {
    let query_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "useQuery"
                    | "useMutation"
                    | "useQueryClient"
                    | "useSuspenseQuery"
                    | "useInfiniteQuery"
                    | "usePrefetchQuery"
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
pub fn is_react_hook_wrapper(info: &SymbolInfo) -> bool {
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
    !hook_calls.is_empty()
        && info.calls.len() <= hook_calls.len() + 3
        && info.control_flow.len() <= 2
}

/// Event handler: handle*/on* with minimal calls
pub fn is_event_handler(info: &SymbolInfo) -> bool {
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
pub fn is_test_setup(info: &SymbolInfo) -> bool {
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
pub fn is_type_guard(info: &SymbolInfo) -> bool {
    // Name starts with "is" and is short
    if !info.name.starts_with("is") || info.name.len() < 3 {
        return false;
    }

    // Minimal logic: few control flow, few calls
    info.control_flow.len() <= 1 && info.calls.len() <= 1
}

/// API route: Express/Next.js route handlers
pub fn is_api_route(info: &SymbolInfo) -> bool {
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
pub fn is_config_export(info: &SymbolInfo) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate::boilerplate::tests::make_symbol;

    // =========================================================================
    // React Query Tests
    // =========================================================================

    #[test]
    fn test_react_query_detection() {
        let symbol = make_symbol("useUserData", vec!["useQuery", "console.log"], 0);
        assert!(is_react_query(&symbol));
    }

    #[test]
    fn test_react_query_with_mutation() {
        let symbol = make_symbol("useCreateUser", vec!["useMutation"], 0);
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
    fn test_react_query_too_much_control_flow() {
        let symbol = make_symbol("useUserData", vec!["useQuery"], 2);
        assert!(!is_react_query(&symbol));
    }

    // =========================================================================
    // React Hook Tests
    // =========================================================================

    #[test]
    fn test_react_hook_wrapper() {
        let symbol = make_symbol("useUserState", vec!["useState", "useEffect"], 0);
        assert!(is_react_hook_wrapper(&symbol));
    }

    #[test]
    fn test_react_hook_custom() {
        let symbol = make_symbol("useDebounce", vec!["useState", "useEffect", "setTimeout"], 1);
        assert!(is_react_hook_wrapper(&symbol));
    }

    #[test]
    fn test_react_hook_not_hook_name() {
        let symbol = make_symbol("getUserState", vec!["useState", "useEffect"], 0);
        assert!(!is_react_hook_wrapper(&symbol));
    }

    #[test]
    fn test_react_hook_no_hook_calls() {
        let symbol = make_symbol("useHelper", vec!["fetch", "process"], 0);
        assert!(!is_react_hook_wrapper(&symbol));
    }

    // =========================================================================
    // Event Handler Tests
    // =========================================================================

    #[test]
    fn test_event_handler_detection() {
        let symbol = make_symbol("handleClick", vec!["setState"], 0);
        assert!(is_event_handler(&symbol));
    }

    #[test]
    fn test_event_handler_on_prefix() {
        let symbol = make_symbol("onChange", vec!["setValue"], 0);
        assert!(is_event_handler(&symbol));
    }

    #[test]
    fn test_event_handler_too_complex() {
        let symbol = make_symbol("handleClick", vec!["validate", "fetch", "process"], 2);
        assert!(!is_event_handler(&symbol));
    }

    #[test]
    fn test_event_handler_minimal() {
        let symbol = make_symbol("handleSubmit", vec![], 0);
        assert!(is_event_handler(&symbol));
    }

    // =========================================================================
    // Test Setup Tests
    // =========================================================================

    #[test]
    fn test_test_setup_detection() {
        let symbol = make_symbol("beforeEach", vec!["mockDb", "seedData"], 0);
        assert!(is_test_setup(&symbol));
    }

    #[test]
    fn test_test_setup_after_each() {
        let symbol = make_symbol("afterEach", vec!["cleanup"], 0);
        assert!(is_test_setup(&symbol));
    }

    #[test]
    fn test_test_setup_setup() {
        let symbol = make_symbol("setup", vec!["init"], 0);
        assert!(is_test_setup(&symbol));
    }

    #[test]
    fn test_test_setup_teardown() {
        let symbol = make_symbol("teardown", vec!["cleanup"], 0);
        assert!(is_test_setup(&symbol));
    }

    // =========================================================================
    // Type Guard Tests
    // =========================================================================

    #[test]
    fn test_type_guard_detection() {
        let symbol = make_symbol("isString", vec![], 1);
        assert!(is_type_guard(&symbol));
    }

    #[test]
    fn test_type_guard_with_call() {
        let symbol = make_symbol("isArray", vec!["Array.isArray"], 0);
        assert!(is_type_guard(&symbol));
    }

    #[test]
    fn test_type_guard_too_complex() {
        let symbol = make_symbol("isValidUser", vec!["validate", "checkDb"], 2);
        assert!(!is_type_guard(&symbol));
    }

    #[test]
    fn test_type_guard_short_name() {
        let symbol = make_symbol("is", vec![], 0);
        assert!(!is_type_guard(&symbol));
    }

    // =========================================================================
    // API Route Tests
    // =========================================================================

    #[test]
    fn test_api_route_get() {
        let symbol = make_symbol("GET", vec!["json"], 0);
        assert!(is_api_route(&symbol));
    }

    #[test]
    fn test_api_route_post() {
        let symbol = make_symbol("POST", vec!["json"], 1);
        assert!(is_api_route(&symbol));
    }

    #[test]
    fn test_api_route_with_response() {
        let symbol = make_symbol("handleRequest", vec!["json", "status"], 1);
        assert!(is_api_route(&symbol));
    }

    #[test]
    fn test_api_route_too_complex() {
        let symbol = make_symbol("handleRequest", vec!["json"], 3);
        assert!(!is_api_route(&symbol));
    }

    // =========================================================================
    // Config Export Tests
    // =========================================================================

    #[test]
    fn test_config_export_detection() {
        let symbol = make_symbol("config", vec![], 0);
        assert!(is_config_export(&symbol));
    }

    #[test]
    fn test_config_export_app_config() {
        let symbol = make_symbol("appConfig", vec![], 0);
        assert!(is_config_export(&symbol));
    }

    #[test]
    fn test_config_export_with_calls() {
        let symbol = make_symbol("config", vec!["process.env"], 0);
        assert!(!is_config_export(&symbol));
    }

    #[test]
    fn test_config_export_settings() {
        let symbol = make_symbol("defaultSettings", vec![], 0);
        assert!(is_config_export(&symbol));
    }
}
