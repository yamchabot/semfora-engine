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
//! - **Redux/RTK**: createSlice, useSelector, selectors
//! - **Validation Schema**: Zod, Yup, Joi schemas
//! - **Test Mocks**: jest.mock, vi.mock, spyOn
//! - **Next.js Data**: getServerSideProps, getStaticProps, generateMetadata
//! - **React Wrappers**: React.memo, forwardRef

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
    PatternMatcher {
        category: BoilerplateCategory::ReduxPattern,
        languages: &[Lang::JavaScript],
        detector: is_redux_pattern,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::ValidationSchema,
        languages: &[Lang::JavaScript],
        detector: is_validation_schema,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::TestMock,
        languages: &[Lang::JavaScript],
        detector: is_test_mock,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::NextjsDataFetching,
        languages: &[Lang::JavaScript],
        detector: is_nextjs_data_fetching,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::ReactWrapper,
        languages: &[Lang::JavaScript],
        detector: is_react_wrapper,
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

/// Redux/RTK: createSlice, createAction, useSelector, useDispatch, selectors
pub fn is_redux_pattern(info: &SymbolInfo) -> bool {
    // RTK slice/action creators
    let rtk_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "createSlice"
                    | "createReducer"
                    | "createAction"
                    | "createAsyncThunk"
                    | "createSelector"
                    | "createEntityAdapter"
                    | "configureStore"
            )
        })
        .collect();

    if !rtk_calls.is_empty() {
        // RTK setup with minimal additional logic
        return info.calls.len() <= rtk_calls.len() + 2 && info.control_flow.len() <= 1;
    }

    // Redux hooks: useSelector, useDispatch, useStore
    let redux_hooks: Vec<_> = info
        .calls
        .iter()
        .filter(|c| matches!(c.name.as_str(), "useSelector" | "useDispatch" | "useStore"))
        .collect();

    if !redux_hooks.is_empty() {
        // Hook usage with minimal logic
        return info.calls.len() <= redux_hooks.len() + 2 && info.control_flow.len() <= 1;
    }

    // Selector pattern: select* functions with minimal logic
    if info.name.starts_with("select") && info.name.len() > 6 {
        // Selectors typically just access state with minimal transformations
        return info.calls.len() <= 2 && info.control_flow.len() <= 1;
    }

    false
}

/// Validation schema: Zod, Yup, Joi schema definitions
pub fn is_validation_schema(info: &SymbolInfo) -> bool {
    // Check for validation library calls
    let validation_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            let name = c.name.as_str();
            // Zod patterns: z.object, z.string, z.number, etc.
            name.starts_with("z.")
                // Yup patterns: yup.object, yup.string, string().required()
                || name.starts_with("yup.")
                || name.starts_with("Yup.")
                // Joi patterns: Joi.object, Joi.string
                || name.starts_with("Joi.")
                || name.starts_with("joi.")
                // Common chained methods on schema objects
                || matches!(
                    name,
                    "object" | "string" | "number" | "boolean" | "array" | "date"
                    | "required" | "optional" | "nullable" | "email" | "min" | "max"
                    | "shape" | "extend" | "pick" | "omit" | "partial" | "strict"
                    | "coerce" | "transform" | "refine" | "superRefine"
                )
        })
        .collect();

    if validation_calls.is_empty() {
        return false;
    }

    // Schema name pattern: ends with Schema, Validator, or Validation
    let is_schema_name = info.name.ends_with("Schema")
        || info.name.ends_with("Validator")
        || info.name.ends_with("Validation")
        || info.name.contains("schema")
        || info.name.contains("Schema");

    // If has validation calls and schema-like name, it's likely a schema definition
    // OR if dominated by validation calls (80%+ of calls are validation-related)
    let validation_ratio = validation_calls.len() as f64 / info.calls.len().max(1) as f64;

    (is_schema_name && !validation_calls.is_empty())
        || (validation_ratio >= 0.8 && validation_calls.len() >= 2)
}

/// Test mocks: jest.mock, vi.mock, mockImplementation, spyOn
pub fn is_test_mock(info: &SymbolInfo) -> bool {
    // Check for mock-related calls
    let mock_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            let name = c.name.as_str();
            // Jest mock patterns
            name.starts_with("jest.")
                || name.starts_with("vi.")
                // Specific mock methods
                || matches!(
                    name,
                    "mock" | "fn" | "spyOn" | "mockImplementation" | "mockImplementationOnce"
                    | "mockReturnValue" | "mockReturnValueOnce" | "mockResolvedValue"
                    | "mockResolvedValueOnce" | "mockRejectedValue" | "mockRejectedValueOnce"
                    | "mockClear" | "mockReset" | "mockRestore"
                )
        })
        .collect();

    if !mock_calls.is_empty() {
        // Has mock calls with minimal other logic
        return info.calls.len() <= mock_calls.len() + 3 && info.control_flow.len() <= 2;
    }

    // Check function name patterns for mock factories
    let name_lower = info.name.to_lowercase();
    let is_mock_name = name_lower.starts_with("mock")
        || name_lower.starts_with("stub")
        || name_lower.starts_with("fake")
        || name_lower.starts_with("spy")
        || name_lower.ends_with("mock")
        || name_lower.ends_with("stub");

    // Mock factory with minimal logic
    is_mock_name && info.calls.len() <= 3 && info.control_flow.len() <= 1
}

/// Next.js data fetching: getServerSideProps, getStaticProps, generateMetadata
pub fn is_nextjs_data_fetching(info: &SymbolInfo) -> bool {
    // Exact function name matches for Next.js data fetching
    matches!(
        info.name.as_str(),
        // Pages Router
        "getServerSideProps" | "getStaticProps" | "getStaticPaths" | "getInitialProps"
        // App Router
        | "generateMetadata" | "generateStaticParams" | "generateViewport"
        // Remix/similar patterns
        | "loader" | "action"
    )
}

/// React wrapper: React.memo, forwardRef with minimal logic
pub fn is_react_wrapper(info: &SymbolInfo) -> bool {
    // Check for wrapper calls
    let wrapper_calls: Vec<_> = info
        .calls
        .iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "memo" | "React.memo" | "forwardRef" | "React.forwardRef"
                | "lazy" | "React.lazy" | "Suspense"
            )
        })
        .collect();

    if wrapper_calls.is_empty() {
        return false;
    }

    // Wrapper with minimal additional logic (wrapper call + maybe 1-2 other calls like displayName)
    info.calls.len() <= wrapper_calls.len() + 2 && info.control_flow.len() <= 1
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

    // =========================================================================
    // Redux/RTK Tests
    // =========================================================================

    #[test]
    fn test_redux_create_slice() {
        let symbol = make_symbol("userSlice", vec!["createSlice"], 0);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_create_async_thunk() {
        let symbol = make_symbol("fetchUsers", vec!["createAsyncThunk", "api.get"], 0);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_create_selector() {
        let symbol = make_symbol("selectUserById", vec!["createSelector"], 0);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_use_selector() {
        let symbol = make_symbol("useUser", vec!["useSelector"], 0);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_use_dispatch() {
        let symbol = make_symbol("useUserActions", vec!["useDispatch", "useCallback"], 1);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_selector_pattern() {
        let symbol = make_symbol("selectAllUsers", vec!["state.users"], 0);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_selector_with_filter() {
        let symbol = make_symbol("selectActiveUsers", vec!["filter"], 1);
        assert!(is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_too_complex() {
        let symbol = make_symbol(
            "userSlice",
            vec!["createSlice", "fetch", "process", "validate", "transform"],
            2,
        );
        assert!(!is_redux_pattern(&symbol));
    }

    #[test]
    fn test_redux_not_redux() {
        let symbol = make_symbol("fetchData", vec!["fetch", "json"], 1);
        assert!(!is_redux_pattern(&symbol));
    }

    // =========================================================================
    // Validation Schema Tests
    // =========================================================================

    #[test]
    fn test_validation_zod_schema() {
        let symbol = make_symbol("userSchema", vec!["z.object", "z.string", "z.number"], 0);
        assert!(is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_yup_schema() {
        let symbol = make_symbol("loginValidator", vec!["yup.object", "yup.string", "required"], 0);
        assert!(is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_joi_schema() {
        let symbol = make_symbol("configSchema", vec!["Joi.object", "Joi.string", "Joi.number"], 0);
        assert!(is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_schema_name_with_calls() {
        let symbol = make_symbol("createUserSchema", vec!["object", "string", "email"], 0);
        assert!(is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_dominated_by_calls() {
        // Even without Schema in name, if 80%+ are validation calls
        let symbol = make_symbol("validateUser", vec!["z.object", "z.string", "z.email"], 0);
        assert!(is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_not_schema_no_calls() {
        let symbol = make_symbol("userSchema", vec![], 0);
        assert!(!is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_not_schema_wrong_calls() {
        let symbol = make_symbol("processData", vec!["fetch", "parse", "transform"], 1);
        assert!(!is_validation_schema(&symbol));
    }

    #[test]
    fn test_validation_mixed_calls_not_dominated() {
        // Less than 80% validation calls without schema name
        let symbol = make_symbol(
            "processUser",
            vec!["z.object", "fetch", "transform", "save", "log"],
            0,
        );
        assert!(!is_validation_schema(&symbol));
    }

    // =========================================================================
    // Test Mock Tests
    // =========================================================================

    #[test]
    fn test_mock_jest_mock() {
        let symbol = make_symbol("setupMocks", vec!["jest.mock", "jest.fn"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_vitest_mock() {
        let symbol = make_symbol("createMock", vec!["vi.mock", "vi.fn"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_spy_on() {
        let symbol = make_symbol("spyOnService", vec!["spyOn", "mockImplementation"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_return_value() {
        let symbol = make_symbol("mockApiResponse", vec!["mockReturnValue", "mockResolvedValue"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_factory_name() {
        let symbol = make_symbol("mockUserService", vec!["createUser"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_stub_factory() {
        let symbol = make_symbol("stubDatabase", vec!["connect"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_fake_factory() {
        let symbol = make_symbol("fakeTimer", vec!["setTimeout"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_suffix() {
        let symbol = make_symbol("userServiceMock", vec!["getUser"], 0);
        assert!(is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_too_complex() {
        let symbol = make_symbol(
            "setupMocks",
            vec!["jest.mock", "fetch", "process", "validate", "transform", "save"],
            3,
        );
        assert!(!is_test_mock(&symbol));
    }

    #[test]
    fn test_mock_not_mock() {
        let symbol = make_symbol("processData", vec!["fetch", "parse"], 1);
        assert!(!is_test_mock(&symbol));
    }

    // =========================================================================
    // Next.js Data Fetching Tests
    // =========================================================================

    #[test]
    fn test_nextjs_get_server_side_props() {
        let symbol = make_symbol("getServerSideProps", vec!["fetch", "db.query"], 1);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_get_static_props() {
        let symbol = make_symbol("getStaticProps", vec!["fetch"], 0);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_get_static_paths() {
        let symbol = make_symbol("getStaticPaths", vec!["db.query"], 1);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_generate_metadata() {
        let symbol = make_symbol("generateMetadata", vec!["getPageTitle"], 0);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_generate_static_params() {
        let symbol = make_symbol("generateStaticParams", vec!["db.findAll"], 1);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_loader_remix() {
        let symbol = make_symbol("loader", vec!["fetch", "json"], 1);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_action_remix() {
        let symbol = make_symbol("action", vec!["formData", "db.create"], 1);
        assert!(is_nextjs_data_fetching(&symbol));
    }

    #[test]
    fn test_nextjs_not_data_fetching() {
        let symbol = make_symbol("fetchData", vec!["fetch"], 0);
        assert!(!is_nextjs_data_fetching(&symbol));
    }

    // =========================================================================
    // React Wrapper Tests
    // =========================================================================

    #[test]
    fn test_react_memo() {
        let symbol = make_symbol("MemoizedComponent", vec!["memo"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_memo_full() {
        let symbol = make_symbol("OptimizedList", vec!["React.memo"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_forward_ref() {
        let symbol = make_symbol("InputWithRef", vec!["forwardRef"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_forward_ref_full() {
        let symbol = make_symbol("ButtonWithRef", vec!["React.forwardRef"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_lazy() {
        let symbol = make_symbol("LazyComponent", vec!["lazy", "import"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_memo_with_display_name() {
        let symbol = make_symbol("NamedMemo", vec!["memo", "displayName"], 0);
        assert!(is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_wrapper_too_complex() {
        let symbol = make_symbol(
            "ComplexWrapper",
            vec!["memo", "useState", "useEffect", "fetch", "process"],
            2,
        );
        assert!(!is_react_wrapper(&symbol));
    }

    #[test]
    fn test_react_wrapper_not_wrapper() {
        let symbol = make_symbol("Component", vec!["useState", "useEffect"], 1);
        assert!(!is_react_wrapper(&symbol));
    }
}
