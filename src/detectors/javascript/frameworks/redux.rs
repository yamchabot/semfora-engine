//! Redux Framework Detector
//!
//! Comprehensive detection for Redux state management including:
//!
//! ## Old-Style Redux (Pre-RTK)
//! - Action type constants (SET_USER, FETCH_DATA_SUCCESS, etc.)
//! - Action creators returning action objects
//! - Reducers with switch statements
//! - combineReducers composition
//! - connect() HOC with mapStateToProps/mapDispatchToProps
//!
//! ## Modern Redux (Redux Toolkit - 2020+)
//! - createSlice with auto-generated actions
//! - createAsyncThunk for async operations
//! - configureStore setup
//! - useSelector/useDispatch hooks
//! - RTK Query (createApi, endpoints)
//!
//! ## Key Relationships Tracked
//! - Action type → State properties it modifies (via reducer)
//! - Reducer → Action types it handles
//! - Selector → State paths it reads
//! - Thunk → API calls and state updates

use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::schema::{Call, FrameworkEntryPoint, RefKind, SemanticSummary, SymbolInfo, SymbolKind};

/// An action type constant with location information
#[derive(Debug, Clone)]
pub struct ActionType {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Redux-specific context extracted from a file
#[derive(Debug, Default)]
pub struct ReduxContext {
    /// Action type constants found (e.g., SET_ACCESS_TOKEN) with locations
    pub action_types: HashMap<String, ActionType>,
    /// Reducer cases: action_type -> state properties modified
    pub reducer_cases: HashMap<String, Vec<StateProperty>>,
    /// Action types from reducer case statements (with line numbers for symbol creation)
    /// This captures action types that are USED in switch cases, even when imported
    pub reducer_case_action_types: HashMap<String, ActionType>,
    /// Slices: slice_name -> (state properties, action names)
    pub slices: HashMap<String, SliceInfo>,
    /// Selectors: selector_name -> state paths accessed
    pub selectors: HashMap<String, Vec<String>>,
    /// Thunks: thunk_name -> action type prefix
    pub thunks: HashSet<String>,
    /// Is this a Redux store configuration file?
    pub is_store_config: bool,
    /// Is this a reducer file?
    pub is_reducer: bool,
    /// Is this a slice file?
    pub is_slice: bool,
}

/// A state property with location information for symbol indexing
#[derive(Debug, Default, Clone)]
pub struct StateProperty {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Default, Clone)]
pub struct SliceInfo {
    pub name: String,
    pub initial_state_props: Vec<StateProperty>,
    pub reducer_names: Vec<String>,
    pub extra_reducer_actions: Vec<String>,
    /// Line where the slice is defined
    pub start_line: usize,
}

/// Enhance semantic summary with Redux-specific information
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut ctx = ReduxContext::default();

    // Detect Redux patterns
    detect_action_types(&mut ctx, root, source);
    detect_reducers(&mut ctx, summary, root, source);
    detect_create_slice(&mut ctx, summary, root, source);
    detect_create_async_thunk(&mut ctx, summary, root, source);
    detect_selectors(&mut ctx, summary, root, source);
    detect_connect_hoc(&mut ctx, summary, root, source);
    detect_use_selector(&mut ctx, summary, root, source);
    detect_store_config(&mut ctx, summary, root, source);
    detect_combine_reducers(&mut ctx, summary, root, source);

    // Add Redux-specific insertions based on what was found
    add_redux_insertions(summary, &ctx);

    // Add state properties as searchable symbols
    add_state_property_symbols(summary, &ctx);

    // Create edges between action types and state properties
    create_action_state_edges(summary, &ctx);
}

// =============================================================================
// Action Type Detection
// =============================================================================

/// Detect action type constants
///
/// Patterns:
/// ```javascript
/// export const SET_ACCESS_TOKEN = 'SET_ACCESS_TOKEN'
/// const FETCH_USER_REQUEST = 'user/FETCH_REQUEST'
/// ```
fn detect_action_types(ctx: &mut ReduxContext, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "variable_declarator" {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(&name_node, source);

                // Action types are typically SCREAMING_SNAKE_CASE
                if is_action_type_name(&name) {
                    if let Some(value_node) = node.child_by_field_name("value") {
                        // Check if it's a string literal (action type constant)
                        if value_node.kind() == "string" || value_node.kind() == "template_string" {
                            ctx.action_types.insert(
                                name.clone(),
                                ActionType {
                                    name,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                },
                            );
                        }
                    }
                }
            }
        }
    });
}

/// Check if a name looks like an action type constant
fn is_action_type_name(name: &str) -> bool {
    // Must be SCREAMING_SNAKE_CASE and contain action-like words
    if name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
    {
        let lower = name.to_lowercase();
        lower.contains("set")
            || lower.contains("get")
            || lower.contains("fetch")
            || lower.contains("load")
            || lower.contains("update")
            || lower.contains("delete")
            || lower.contains("add")
            || lower.contains("remove")
            || lower.contains("clear")
            || lower.contains("reset")
            || lower.contains("toggle")
            || lower.contains("request")
            || lower.contains("success")
            || lower.contains("failure")
            || lower.contains("error")
            || lower.contains("pending")
            || lower.contains("fulfilled")
            || lower.contains("rejected")
    } else {
        false
    }
}

// =============================================================================
// Reducer Detection (Old-Style)
// =============================================================================

/// Detect reducer functions with switch statements
///
/// Pattern:
/// ```javascript
/// const reducer = (state = initialState, action) => {
///   switch (action.type) {
///     case SET_ACCESS_TOKEN:
///       return { ...state, accessToken: action.payload }
///     default:
///       return state
///   }
/// }
/// ```
fn detect_reducers(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        // Look for functions with (state, action) parameters and switch statements
        if matches!(
            node.kind(),
            "arrow_function" | "function_declaration" | "function"
        ) {
            if is_reducer_function(node, source) {
                ctx.is_reducer = true;

                // Find the function name
                let func_name = get_reducer_name(node, source);

                // Extract switch cases
                if let Some(body) = node.child_by_field_name("body") {
                    extract_reducer_cases(ctx, &body, source);
                }

                // Mark reducer as framework entry point
                if let Some(name) = &func_name {
                    for symbol in &mut summary.symbols {
                        if &symbol.name == name {
                            symbol.framework_entry_point = FrameworkEntryPoint::ReduxReducer;
                        }
                    }
                }
            }
        }
    });
}

/// Check if a function looks like a reducer
fn is_reducer_function(node: &Node, source: &str) -> bool {
    // Check parameters: (state, action) or (state = initialState, action)
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = get_node_text(&params, source);
        let has_state_param = params_text.contains("state") || params_text.contains("State");
        let has_action_param = params_text.contains("action");

        if has_state_param && has_action_param {
            // Check for switch statement in body
            if let Some(body) = node.child_by_field_name("body") {
                let body_text = get_node_text(&body, source);
                return body_text.contains("switch") && body_text.contains("action.type");
            }
        }
    }
    false
}

/// Get the name of a reducer function
fn get_reducer_name(node: &Node, source: &str) -> Option<String> {
    // For function declarations, get the name directly
    if node.kind() == "function_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(get_node_text(&name_node, source));
        }
    }

    // For arrow functions, look at parent variable_declarator
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return Some(get_node_text(&name_node, source));
            }
        }
    }

    None
}

/// Extract switch cases from a reducer body
fn extract_reducer_cases(ctx: &mut ReduxContext, body: &Node, source: &str) {
    visit_all(body, |node| {
        if node.kind() == "switch_case" {
            // Get the case value (action type)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let action_type = get_node_text(&child, source);

                    // Capture action type location from the case statement
                    // This allows us to create symbols for imported action types
                    ctx.reducer_case_action_types
                        .entry(action_type.clone())
                        .or_insert_with(|| ActionType {
                            name: action_type.clone(),
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                        });

                    // Extract state properties modified in this case
                    let state_props = extract_state_modifications(node, source);

                    if !state_props.is_empty() {
                        ctx.reducer_cases
                            .entry(action_type)
                            .or_default()
                            .extend(state_props);
                    }
                    break;
                }
            }
        }
    });
}

/// Extract state property names with line numbers from a reducer case body
fn extract_state_modifications(case_node: &Node, source: &str) -> Vec<StateProperty> {
    let mut props = Vec::new();

    visit_all(case_node, |node| {
        // Look for object properties in return statements
        // Pattern: return { ...state, propertyName: value }
        if node.kind() == "pair" || node.kind() == "property" {
            if let Some(key) = node.child_by_field_name("key") {
                let key_text = get_node_text(&key, source);
                // Exclude spread operator
                if !key_text.starts_with("...") && !key_text.is_empty() {
                    props.push(StateProperty {
                        name: key_text,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                }
            }
        }

        // Also look for shorthand properties
        if node.kind() == "shorthand_property_identifier" {
            let prop = get_node_text(node, source);
            if !prop.is_empty() {
                props.push(StateProperty {
                    name: prop,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                });
            }
        }
    });

    props
}

// =============================================================================
// createSlice Detection (RTK)
// =============================================================================

/// Detect createSlice from Redux Toolkit
///
/// Pattern:
/// ```javascript
/// const authSlice = createSlice({
///   name: 'auth',
///   initialState: { accessToken: null, user: null },
///   reducers: {
///     setAccessToken: (state, action) => { state.accessToken = action.payload },
///     logout: (state) => { state.accessToken = null }
///   },
///   extraReducers: (builder) => {
///     builder.addCase(fetchUser.fulfilled, (state, action) => {
///       state.user = action.payload
///     })
///   }
/// })
/// ```
fn detect_create_slice(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "createSlice" {
                    ctx.is_slice = true;

                    // Get slice variable name
                    let slice_var_name = get_slice_variable_name(node, source);

                    // Parse slice config
                    if let Some(args) = node.child_by_field_name("arguments") {
                        if let Some(config) = find_first_object_arg(&args, source) {
                            let slice_info = parse_slice_config(&config, source);

                            // Add slice info to context
                            let slice_name = slice_var_name
                                .clone()
                                .unwrap_or_else(|| slice_info.name.clone());
                            ctx.slices.insert(slice_name.clone(), slice_info.clone());

                            // Mark slice as framework entry point
                            if let Some(var_name) = &slice_var_name {
                                for symbol in &mut summary.symbols {
                                    if &symbol.name == var_name {
                                        symbol.framework_entry_point =
                                            FrameworkEntryPoint::ReduxSlice;
                                    }
                                }
                            }

                            // Add reducers as framework entry points
                            for reducer_name in &slice_info.reducer_names {
                                // Add as external call (since these become action creators)
                                summary.calls.push(Call {
                                    name: format!(
                                        "{}.actions.{}",
                                        slice_var_name.as_deref().unwrap_or("slice"),
                                        reducer_name
                                    ),
                                    object: None,
                                    is_awaited: false,
                                    in_try: false,
                                    ref_kind: RefKind::Write,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        }
    });
}

/// Get the variable name a slice is assigned to
fn get_slice_variable_name(node: &Node, source: &str) -> Option<String> {
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return Some(get_node_text(&name_node, source));
            }
        }
    }
    None
}

/// Find the first object argument in an arguments list
fn find_first_object_arg<'a>(args: &Node<'a>, _source: &str) -> Option<Node<'a>> {
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "object" {
            return Some(child);
        }
    }
    None
}

/// Parse a createSlice configuration object
fn parse_slice_config(config: &Node, source: &str) -> SliceInfo {
    let mut info = SliceInfo::default();
    // Capture the config object's start line for the slice
    info.start_line = config.start_position().row + 1;

    let mut cursor = config.walk();
    for child in config.children(&mut cursor) {
        if child.kind() == "pair" {
            if let Some(key) = child.child_by_field_name("key") {
                let key_text = get_node_text(&key, source);

                match key_text.as_str() {
                    "name" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            info.name = get_node_text(&value, source)
                                .trim_matches(|c| c == '"' || c == '\'')
                                .to_string();
                        }
                    }
                    "initialState" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            // Use location-aware extraction for symbol indexing
                            info.initial_state_props =
                                extract_object_keys_with_locations(&value, source);
                        }
                    }
                    "reducers" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            info.reducer_names = extract_object_keys(&value, source);
                        }
                    }
                    "extraReducers" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            info.extra_reducer_actions =
                                extract_extra_reducer_actions(&value, source);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    info
}

/// Extract keys from an object node (names only, for backward compatibility)
fn extract_object_keys(node: &Node, source: &str) -> Vec<String> {
    extract_object_keys_with_locations(node, source)
        .into_iter()
        .map(|p| p.name)
        .collect()
}

/// Extract keys from an object node with line number locations
///
/// This is the pattern for framework detectors: always capture locations
/// so properties can be indexed as searchable symbols.
fn extract_object_keys_with_locations(node: &Node, source: &str) -> Vec<StateProperty> {
    let mut props = Vec::new();

    if node.kind() == "object" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "pair" || child.kind() == "method_definition" {
                if let Some(key) = child
                    .child_by_field_name("key")
                    .or_else(|| child.child_by_field_name("name"))
                {
                    let key_text = get_node_text(&key, source);
                    if !key_text.is_empty() {
                        props.push(StateProperty {
                            name: key_text,
                            start_line: child.start_position().row + 1, // 1-indexed
                            end_line: child.end_position().row + 1,
                        });
                    }
                }
            } else if child.kind() == "shorthand_property_identifier" {
                props.push(StateProperty {
                    name: get_node_text(&child, source),
                    start_line: child.start_position().row + 1,
                    end_line: child.end_position().row + 1,
                });
            }
        }
    }

    props
}

/// Extract action references from extraReducers
fn extract_extra_reducer_actions(node: &Node, source: &str) -> Vec<String> {
    let mut actions = Vec::new();

    visit_all(node, |child| {
        // Look for addCase calls
        if child.kind() == "call_expression" {
            let text = get_node_text(child, source);
            if text.contains("addCase")
                || text.contains(".fulfilled")
                || text.contains(".pending")
                || text.contains(".rejected")
            {
                // Extract the action reference
                if let Some(args) = child.child_by_field_name("arguments") {
                    let mut cursor = args.walk();
                    for arg in args.children(&mut cursor) {
                        if arg.kind() == "member_expression" || arg.kind() == "identifier" {
                            actions.push(get_node_text(&arg, source));
                            break;
                        }
                    }
                }
            }
        }
    });

    actions
}

// =============================================================================
// createAsyncThunk Detection
// =============================================================================

/// Detect createAsyncThunk
///
/// Pattern:
/// ```javascript
/// export const fetchUser = createAsyncThunk(
///   'user/fetch',
///   async (userId, { rejectWithValue }) => {
///     const response = await api.fetchUser(userId)
///     return response.data
///   }
/// )
/// ```
fn detect_create_async_thunk(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "createAsyncThunk" {
                    // Get thunk variable name
                    if let Some(thunk_name) = get_thunk_variable_name(node, source) {
                        ctx.thunks.insert(thunk_name.clone());

                        // Mark as framework entry point
                        for symbol in &mut summary.symbols {
                            if symbol.name == thunk_name {
                                symbol.framework_entry_point = FrameworkEntryPoint::ReduxThunk;
                            }
                        }

                        // Add thunk lifecycle states as implicit calls
                        for suffix in &["pending", "fulfilled", "rejected"] {
                            summary.calls.push(Call {
                                name: format!("{}.{}", thunk_name, suffix),
                                object: None,
                                is_awaited: false,
                                in_try: false,
                                ref_kind: RefKind::None,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    });
}

fn get_thunk_variable_name(node: &Node, source: &str) -> Option<String> {
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return Some(get_node_text(&name_node, source));
            }
        }
    }
    None
}

// =============================================================================
// Selector Detection
// =============================================================================

/// Detect selector functions (reselect-style or simple selectors)
///
/// Patterns:
/// ```javascript
/// const selectUser = (state) => state.user
/// const selectAccessToken = createSelector([selectAuth], (auth) => auth.accessToken)
/// ```
fn detect_selectors(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "arrow_function" || node.kind() == "function_declaration" {
            // Check if it's a selector (takes state, returns state path)
            if is_selector_function(node, source) {
                if let Some(name) = get_selector_name(node, source) {
                    let state_paths = extract_state_paths(node, source);
                    if !state_paths.is_empty() {
                        ctx.selectors.insert(name.clone(), state_paths);

                        // Mark as framework entry point
                        for symbol in &mut summary.symbols {
                            if symbol.name == name {
                                symbol.framework_entry_point = FrameworkEntryPoint::ReduxSelector;
                            }
                        }
                    }
                }
            }
        }

        // Also detect createSelector calls
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "createSelector" {
                    if let Some(name) = get_selector_name(node, source) {
                        ctx.selectors
                            .insert(name.clone(), vec!["composed".to_string()]);

                        for symbol in &mut summary.symbols {
                            if symbol.name == name {
                                symbol.framework_entry_point = FrameworkEntryPoint::ReduxSelector;
                            }
                        }
                    }
                }
            }
        }
    });
}

fn is_selector_function(node: &Node, source: &str) -> bool {
    // Check for (state) => state.something pattern
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = get_node_text(&params, source);
        if params_text.contains("state") || params_text.contains("State") {
            // Check body accesses state
            if let Some(body) = node.child_by_field_name("body") {
                let body_text = get_node_text(&body, source);
                return body_text.starts_with("state.") || body_text.contains("state.");
            }
        }
    }
    false
}

fn get_selector_name(node: &Node, source: &str) -> Option<String> {
    // Check parent for variable name
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                let name = get_node_text(&name_node, source);
                // Selector naming convention: select*
                if name.starts_with("select") || name.starts_with("get") {
                    return Some(name);
                }
            }
        }
    }
    None
}

fn extract_state_paths(node: &Node, source: &str) -> Vec<String> {
    let mut paths = Vec::new();

    visit_all(node, |child| {
        if child.kind() == "member_expression" {
            let text = get_node_text(child, source);
            if text.starts_with("state.") {
                // Extract the property path
                let path = text.trim_start_matches("state.");
                if !path.is_empty() {
                    paths.push(path.to_string());
                }
            }
        }
    });

    paths
}

// =============================================================================
// connect() HOC Detection
// =============================================================================

/// Detect connect() HOC usage
///
/// Pattern:
/// ```javascript
/// const mapStateToProps = (state) => ({
///   accessToken: state.auth.accessToken,
///   user: state.user.data
/// })
/// const mapDispatchToProps = { setAccessToken, fetchUser }
/// export default connect(mapStateToProps, mapDispatchToProps)(MyComponent)
/// ```
fn detect_connect_hoc(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            let text = get_node_text(node, source);
            if text.starts_with("connect(") {
                // Found connect() usage
                push_unique_insertion(
                    &mut summary.insertions,
                    "Redux connect() HOC".to_string(),
                    "redux",
                );

                // Extract mapStateToProps state accesses
                if let Some(args) = node.child_by_field_name("arguments") {
                    extract_map_state_paths(ctx, &args, source);
                }
            }
        }

        // Detect mapStateToProps function
        if node.kind() == "arrow_function" || node.kind() == "function_declaration" {
            if let Some(name) = get_function_name(node, source) {
                if name == "mapStateToProps" || name == "mapState" {
                    let state_paths = extract_state_paths(node, source);
                    for path in state_paths {
                        ctx.selectors
                            .entry("mapStateToProps".to_string())
                            .or_default()
                            .push(path);
                    }
                }
            }
        }
    });
}

fn get_function_name(node: &Node, source: &str) -> Option<String> {
    if node.kind() == "function_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(get_node_text(&name_node, source));
        }
    }
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return Some(get_node_text(&name_node, source));
            }
        }
    }
    None
}

fn extract_map_state_paths(ctx: &mut ReduxContext, args: &Node, source: &str) {
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        // If mapStateToProps is inline function
        if child.kind() == "arrow_function" {
            let paths = extract_state_paths(&child, source);
            for path in paths {
                ctx.selectors
                    .entry("mapStateToProps".to_string())
                    .or_default()
                    .push(path);
            }
        }
    }
}

// =============================================================================
// useSelector Detection
// =============================================================================

/// Detect useSelector hook usage
///
/// Pattern:
/// ```javascript
/// const accessToken = useSelector((state) => state.auth.accessToken)
/// const { user, loading } = useSelector(selectUserState)
/// ```
fn detect_use_selector(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                if func_name == "useSelector" {
                    // Get the variable name
                    let var_name = get_hook_variable_name(node, source);

                    // Extract state path from selector
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        for child in args.children(&mut cursor) {
                            if child.kind() == "arrow_function" {
                                let paths = extract_state_paths(&child, source);
                                for path in &paths {
                                    // Add as read reference to state property
                                    summary.calls.push(Call {
                                        name: path.clone(),
                                        object: Some("state".to_string()),
                                        is_awaited: false,
                                        in_try: false,
                                        ref_kind: RefKind::Read,
                                        ..Default::default()
                                    });
                                }

                                if let Some(name) = &var_name {
                                    ctx.selectors.insert(name.clone(), paths);
                                }
                            } else if child.kind() == "identifier" {
                                // Using a named selector
                                let selector_name = get_node_text(&child, source);
                                summary.calls.push(Call {
                                    name: selector_name,
                                    object: None,
                                    is_awaited: false,
                                    in_try: false,
                                    ref_kind: RefKind::None,
                                    ..Default::default()
                                });
                            }
                        }
                    }

                    push_unique_insertion(
                        &mut summary.insertions,
                        format!(
                            "useSelector: {}",
                            var_name.unwrap_or_else(|| "state".to_string())
                        ),
                        "redux",
                    );
                }
            }
        }
    });
}

fn get_hook_variable_name(node: &Node, source: &str) -> Option<String> {
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                let text = get_node_text(&name_node, source);
                if !text.starts_with('{') && !text.starts_with('[') {
                    return Some(text);
                }
            }
        }
    }
    None
}

// =============================================================================
// Store Configuration Detection
// =============================================================================

/// Detect store configuration
///
/// Patterns:
/// ```javascript
/// // Legacy
/// const store = createStore(rootReducer, applyMiddleware(thunk))
///
/// // RTK
/// const store = configureStore({
///   reducer: { auth: authReducer, user: userReducer }
/// })
/// ```
fn detect_store_config(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);

                if func_name == "configureStore" || func_name == "createStore" {
                    ctx.is_store_config = true;

                    push_unique_insertion(
                        &mut summary.insertions,
                        format!("Redux store ({})", func_name),
                        "redux",
                    );

                    // Mark file as store entry point
                    summary.framework_entry_point = FrameworkEntryPoint::ReduxStore;
                }
            }
        }
    });
}

// =============================================================================
// combineReducers Detection
// =============================================================================

/// Detect combineReducers
///
/// Pattern:
/// ```javascript
/// const rootReducer = combineReducers({
///   auth: authReducer,
///   user: userReducer,
///   posts: postsReducer
/// })
/// ```
fn detect_combine_reducers(
    ctx: &mut ReduxContext,
    summary: &mut SemanticSummary,
    root: &Node,
    source: &str,
) {
    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);

                if func_name == "combineReducers" {
                    ctx.is_reducer = true;

                    // Extract reducer keys (these become state slice names)
                    if let Some(args) = node.child_by_field_name("arguments") {
                        if let Some(config) = find_first_object_arg(&args, source) {
                            let slice_names = extract_object_keys(&config, source);

                            for slice_name in &slice_names {
                                push_unique_insertion(
                                    &mut summary.insertions,
                                    format!("state.{}", slice_name),
                                    "redux",
                                );
                            }
                        }
                    }

                    push_unique_insertion(
                        &mut summary.insertions,
                        "combineReducers".to_string(),
                        "redux",
                    );
                }
            }
        }
    });
}

// =============================================================================
// Symbol Creation (Critical for Searchability)
// =============================================================================

/// Add Redux state properties as searchable symbols
///
/// This is the key function that makes state properties (like `accessToken`)
/// appear in search results. Without this, detection happens but symbols
/// aren't indexed.
///
/// Pattern for future framework detectors:
/// 1. Detect framework-specific constructs
/// 2. Extract names WITH line numbers
/// 3. Call add_*_symbols() to create SymbolInfo entries
fn add_state_property_symbols(summary: &mut SemanticSummary, ctx: &ReduxContext) {
    // From RTK slices (createSlice)
    for (slice_name, info) in &ctx.slices {
        for prop in &info.initial_state_props {
            // Check if symbol already exists (avoid duplicates)
            let exists = summary
                .symbols
                .iter()
                .any(|s| s.name == prop.name && s.kind == SymbolKind::Variable);

            if !exists {
                summary.symbols.push(SymbolInfo {
                    name: prop.name.clone(),
                    kind: SymbolKind::Variable,
                    start_line: prop.start_line,
                    end_line: prop.end_line,
                    is_exported: false,
                    is_default_export: false,
                    // Mark as Redux state so it's not flagged as dead code
                    framework_entry_point: FrameworkEntryPoint::ReduxSelector,
                    ..Default::default()
                });
            }
        }

        // Also add reducer action creators as symbols
        for reducer_name in &info.reducer_names {
            let action_name = format!("{}.actions.{}", slice_name, reducer_name);
            let exists = summary.symbols.iter().any(|s| s.name == action_name);

            if !exists {
                summary.symbols.push(SymbolInfo {
                    name: action_name,
                    kind: SymbolKind::Function,
                    start_line: info.start_line,
                    end_line: info.start_line, // Single line for now
                    is_exported: true,
                    framework_entry_point: FrameworkEntryPoint::ReduxReducer,
                    ..Default::default()
                });
            }
        }
    }

    // From old-style reducers (switch/case pattern)
    // Add unique state properties found in reducer cases
    let mut added_props: HashSet<String> = HashSet::new();
    for (_action_type, state_props) in &ctx.reducer_cases {
        for prop in state_props {
            if added_props.insert(prop.name.clone()) {
                let exists = summary
                    .symbols
                    .iter()
                    .any(|s| s.name == prop.name && s.kind == SymbolKind::Variable);

                if !exists {
                    summary.symbols.push(SymbolInfo {
                        name: prop.name.clone(),
                        kind: SymbolKind::Variable,
                        start_line: prop.start_line,
                        end_line: prop.end_line,
                        is_exported: false,
                        framework_entry_point: FrameworkEntryPoint::ReduxSelector,
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Add action type constants as symbols (from definitions in this file)
    for action_type in ctx.action_types.values() {
        let exists = summary
            .symbols
            .iter()
            .any(|s| s.name == action_type.name && s.kind == SymbolKind::Variable);

        if !exists {
            summary.symbols.push(SymbolInfo {
                name: action_type.name.clone(),
                kind: SymbolKind::Variable,
                start_line: action_type.start_line,
                end_line: action_type.end_line,
                is_exported: true, // Action types are usually exported
                framework_entry_point: FrameworkEntryPoint::ReduxReducer,
                ..Default::default()
            });
        }
    }

    // Add action types from reducer case statements (for imported action types)
    // These symbols have their `calls` populated so the call graph builder
    // creates edges FROM the action type TO the state properties it modifies
    for (action_type_name, action_type) in &ctx.reducer_case_action_types {
        // Skip if already added from ctx.action_types (defined in this file)
        let exists = summary
            .symbols
            .iter()
            .any(|s| s.name == *action_type_name && s.kind == SymbolKind::Variable);

        if !exists {
            // Build calls for this action type: writes to each state property it modifies
            let calls: Vec<Call> = ctx
                .reducer_cases
                .get(action_type_name)
                .map(|state_props| {
                    state_props
                        .iter()
                        .map(|prop| Call {
                            name: prop.name.clone(),
                            object: None,
                            is_awaited: false,
                            in_try: false,
                            ref_kind: RefKind::Write,
                            ..Default::default()
                        })
                        .collect()
                })
                .unwrap_or_default();

            summary.symbols.push(SymbolInfo {
                name: action_type_name.clone(),
                kind: SymbolKind::Variable,
                start_line: action_type.start_line,
                end_line: action_type.end_line,
                is_exported: false, // Imported, not exported from this file
                framework_entry_point: FrameworkEntryPoint::ReduxReducer,
                calls, // Populate calls so edges are created FROM this symbol
                ..Default::default()
            });
        }
    }
}

// =============================================================================
// Insertions and Edge Creation
// =============================================================================

/// Add Redux-specific insertions to summary
fn add_redux_insertions(summary: &mut SemanticSummary, ctx: &ReduxContext) {
    if !ctx.action_types.is_empty() {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} action types", ctx.action_types.len()),
            "redux",
        );
    }

    if !ctx.reducer_cases.is_empty() {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} reducer cases", ctx.reducer_cases.len()),
            "redux",
        );
    }

    for (slice_name, info) in &ctx.slices {
        push_unique_insertion(
            &mut summary.insertions,
            format!(
                "slice '{}' with {} reducers",
                slice_name,
                info.reducer_names.len()
            ),
            "redux",
        );
    }

    if !ctx.thunks.is_empty() {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} async thunks", ctx.thunks.len()),
            "redux",
        );
    }

    if !ctx.selectors.is_empty() {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} selectors", ctx.selectors.len()),
            "redux",
        );
    }
}

/// Create edges between action types and state properties they modify
fn create_action_state_edges(summary: &mut SemanticSummary, ctx: &ReduxContext) {
    // NOTE: Old-style reducer cases (switch/case) are now handled in add_state_property_symbols
    // by populating the action type symbol's `calls` field. This ensures edges are created
    // FROM the action type symbol TO the state properties, not from globalReducer.

    // For slices, create edges from slice reducers to initial state props
    // (slices still use summary.calls since they're defined in the same file)
    for (_slice_name, info) in &ctx.slices {
        for reducer_name in &info.reducer_names {
            for prop in &info.initial_state_props {
                summary.calls.push(Call {
                    name: prop.name.clone(),
                    object: Some(reducer_name.clone()),
                    is_awaited: false,
                    in_try: false,
                    ref_kind: RefKind::Write,
                    ..Default::default()
                });
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Failed to set language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_action_type_detection() {
        let source = r#"
            export const SET_ACCESS_TOKEN = 'SET_ACCESS_TOKEN'
            export const FETCH_USER_REQUEST = 'user/FETCH_REQUEST'
            const normalVar = 'not an action'
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        detect_action_types(&mut ctx, &tree.root_node(), source);

        assert!(ctx.action_types.contains_key("SET_ACCESS_TOKEN"));
        assert!(ctx.action_types.contains_key("FETCH_USER_REQUEST"));
        assert!(!ctx.action_types.contains_key("normalVar"));
    }

    #[test]
    fn test_is_action_type_name() {
        assert!(is_action_type_name("SET_ACCESS_TOKEN"));
        assert!(is_action_type_name("FETCH_USER_REQUEST"));
        assert!(is_action_type_name("ADD_TODO"));
        assert!(is_action_type_name("CLEAR_ERRORS"));
        assert!(!is_action_type_name("setAccessToken"));
        assert!(!is_action_type_name("SOME_CONSTANT"));
    }

    #[test]
    fn test_reducer_detection() {
        let source = r#"
            const initialState = { accessToken: null }

            export const authReducer = (state = initialState, action) => {
                switch (action.type) {
                    case SET_ACCESS_TOKEN:
                        return { ...state, accessToken: action.payload }
                    case CLEAR_TOKEN:
                        return { ...state, accessToken: null }
                    default:
                        return state
                }
            }
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_reducers(&mut ctx, &mut summary, &tree.root_node(), source);

        assert!(ctx.is_reducer);
        assert!(ctx.reducer_cases.contains_key("SET_ACCESS_TOKEN"));
        assert!(ctx.reducer_cases.contains_key("CLEAR_TOKEN"));
    }

    #[test]
    fn test_create_slice_detection() {
        let source = r#"
            const authSlice = createSlice({
                name: 'auth',
                initialState: { accessToken: null, user: null },
                reducers: {
                    setAccessToken: (state, action) => {
                        state.accessToken = action.payload
                    },
                    logout: (state) => {
                        state.accessToken = null
                    }
                }
            })
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_create_slice(&mut ctx, &mut summary, &tree.root_node(), source);

        assert!(ctx.is_slice);
        assert!(ctx.slices.contains_key("authSlice"));
        let slice = ctx.slices.get("authSlice").unwrap();
        assert_eq!(slice.name, "auth");
        assert!(
            slice
                .initial_state_props
                .iter()
                .any(|p| p.name == "accessToken"),
            "Expected accessToken in initial_state_props"
        );
        assert!(slice.reducer_names.contains(&"setAccessToken".to_string()));
        assert!(slice.reducer_names.contains(&"logout".to_string()));
    }

    #[test]
    fn test_create_async_thunk_detection() {
        let source = r#"
            export const fetchUser = createAsyncThunk(
                'user/fetch',
                async (userId) => {
                    const response = await api.fetchUser(userId)
                    return response.data
                }
            )
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_create_async_thunk(&mut ctx, &mut summary, &tree.root_node(), source);

        assert!(ctx.thunks.contains("fetchUser"));
        // Should add lifecycle calls
        assert!(summary.calls.iter().any(|c| c.name == "fetchUser.pending"));
        assert!(summary
            .calls
            .iter()
            .any(|c| c.name == "fetchUser.fulfilled"));
        assert!(summary.calls.iter().any(|c| c.name == "fetchUser.rejected"));
    }

    #[test]
    fn test_use_selector_detection() {
        let source = r#"
            function Component() {
                const accessToken = useSelector((state) => state.auth.accessToken)
                const user = useSelector(selectUser)
                return <div>{user.name}</div>
            }
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_use_selector(&mut ctx, &mut summary, &tree.root_node(), source);

        // Should have selector entries
        assert!(!ctx.selectors.is_empty());
        // Should have calls for state access
        assert!(summary.calls.iter().any(|c| c.name == "auth.accessToken"));
        // Should have call to named selector
        assert!(summary.calls.iter().any(|c| c.name == "selectUser"));
    }

    #[test]
    fn test_configure_store_detection() {
        let source = r#"
            export const store = configureStore({
                reducer: {
                    auth: authReducer,
                    user: userReducer
                }
            })
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_store_config(&mut ctx, &mut summary, &tree.root_node(), source);

        assert!(ctx.is_store_config);
        assert!(summary.insertions.iter().any(|i| i.contains("Redux store")));
    }

    #[test]
    fn test_combine_reducers_detection() {
        let source = r#"
            const rootReducer = combineReducers({
                auth: authReducer,
                user: userReducer,
                posts: postsReducer
            })
        "#;
        let tree = parse_ts(source);
        let mut ctx = ReduxContext::default();
        let mut summary = SemanticSummary::default();
        detect_combine_reducers(&mut ctx, &mut summary, &tree.root_node(), source);

        assert!(ctx.is_reducer);
        assert!(summary.insertions.iter().any(|i| i.contains("state.auth")));
        assert!(summary.insertions.iter().any(|i| i.contains("state.user")));
    }

    #[test]
    fn test_enhance_full_flow() {
        // Test the complete enhance() flow with a real-world reducer pattern
        let source = r#"
import {
    SET_ACCESS_TOKEN,
    SET_UNREAD_MESSAGES_INFO,
    GlobalActions
} from '../../Actions/Global'

const globalReducerDefaultState = {
    accessToken: null,
    unreadMessageInfo: null
}

export const globalReducer = (state = globalReducerDefaultState, action: GlobalActions): any => {
    switch (action.type) {
        case SET_ACCESS_TOKEN:
            return {
                ...state,
                accessToken: action.token,
            }
        case SET_UNREAD_MESSAGES_INFO:
            return {
                ...state,
                unreadMessageInfo: action.payload,
            }
        default:
            return state
    }
}
        "#;
        let tree = parse_ts(source);
        let mut summary = SemanticSummary::default();

        // Call enhance directly
        enhance(&mut summary, &tree.root_node(), source);

        // Verify insertions were added
        assert!(
            !summary.insertions.is_empty(),
            "Expected Redux insertions but got none"
        );

        // Verify reducer cases were detected
        assert!(
            summary
                .insertions
                .iter()
                .any(|i| i.contains("reducer cases")),
            "Expected 'reducer cases' insertion but got: {:?}",
            summary.insertions
        );

        // Verify symbols were created for state properties from old-style reducer
        let state_prop_symbols: Vec<_> = summary
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();
        assert!(
            state_prop_symbols.iter().any(|s| s.name == "accessToken"),
            "Expected symbol for accessToken from old-style reducer but got: {:?}",
            state_prop_symbols
        );
        assert!(
            state_prop_symbols
                .iter()
                .any(|s| s.name == "unreadMessageInfo"),
            "Expected symbol for unreadMessageInfo from old-style reducer but got: {:?}",
            state_prop_symbols
        );

        // Verify action type symbols were created with calls to state properties
        // (This is how call graph edges are created FROM action types TO state props)
        let action_type_symbol = summary
            .symbols
            .iter()
            .find(|s| s.name == "SET_ACCESS_TOKEN")
            .expect("Expected SET_ACCESS_TOKEN symbol");
        assert!(
            action_type_symbol
                .calls
                .iter()
                .any(|c| c.name == "accessToken"),
            "Expected SET_ACCESS_TOKEN to have call to accessToken but got: {:?}",
            action_type_symbol.calls
        );
    }

    #[test]
    fn test_state_property_symbols_created() {
        // Test that state properties from createSlice are added as searchable symbols
        // Uses same format as test_create_slice_detection for consistency
        let source = r#"
            const authSlice = createSlice({
                name: 'auth',
                initialState: { accessToken: null, refreshToken: null, isLoading: false },
                reducers: {
                    setAccessToken: (state, action) => {
                        state.accessToken = action.payload
                    },
                    setRefreshToken: (state, action) => {
                        state.refreshToken = action.payload
                    },
                    setLoading: (state, action) => {
                        state.isLoading = action.payload
                    }
                }
            })
        "#;
        let tree = parse_ts(source);
        let mut summary = SemanticSummary::default();

        // Call enhance directly
        enhance(&mut summary, &tree.root_node(), source);

        // Verify symbols were created for state properties
        let state_prop_symbols: Vec<_> = summary
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            state_prop_symbols.iter().any(|s| s.name == "accessToken"),
            "Expected symbol for accessToken state property but got: {:?}",
            state_prop_symbols
        );
        assert!(
            state_prop_symbols.iter().any(|s| s.name == "refreshToken"),
            "Expected symbol for refreshToken state property but got: {:?}",
            state_prop_symbols
        );
        assert!(
            state_prop_symbols.iter().any(|s| s.name == "isLoading"),
            "Expected symbol for isLoading state property but got: {:?}",
            state_prop_symbols
        );

        // Verify reducer action creators are also symbols (they have qualified names)
        let reducer_symbols: Vec<_> = summary
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(
            reducer_symbols
                .iter()
                .any(|s| s.name.contains("setAccessToken")),
            "Expected symbol containing setAccessToken reducer but got: {:?}",
            reducer_symbols
        );
    }
}
