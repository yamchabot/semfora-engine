//! JavaScript Framework Detection and Enhancement
//!
//! This module provides framework detection and specialized extractors for
//! popular JavaScript/TypeScript frameworks and libraries.
//!
//! # Framework Detection
//!
//! Frameworks are detected based on multiple signals:
//! - Import statements (e.g., `import React from 'react'`)
//! - File path patterns (e.g., `/app/api/route.ts` for Next.js)
//! - Decorator patterns (e.g., `@Component` for Angular)
//! - Function call patterns (e.g., `app.get()` for Express)
//!
//! # Supported Frameworks
//!
//! - **React**: JSX, hooks (useState, useEffect), forwardRef, memo, styled-components
//! - **Next.js**: App router, API routes, layouts, pages, server components
//! - **Express**: Route handlers, middleware, Router
//! - **Angular**: Component/Injectable/NgModule decorators, services
//! - **Vue**: Composition API (ref, reactive, computed), defineComponent

pub mod angular;
pub mod express;
pub mod nestjs;
pub mod nextjs;
pub mod react;
pub mod redux;
pub mod vue;

use crate::schema::SemanticSummary;

/// Propagate the framework_entry_point from summary to its symbols
///
/// This shared function is used by framework enhancers (Next.js, NestJS, etc.)
/// to propagate the file-level framework entry point to individual symbols.
/// It sets the entry point on exported symbols (including default exports)
/// that don't already have one set.
pub fn propagate_entry_point_to_symbols(summary: &mut SemanticSummary) {
    if summary.framework_entry_point.is_entry_point() {
        for symbol in &mut summary.symbols {
            // Set framework entry point on default exports and exported symbols
            if (symbol.is_default_export || symbol.is_exported)
                && symbol.framework_entry_point.is_none()
            {
                symbol.framework_entry_point = summary.framework_entry_point;
            }
        }
    }
}

/// Framework detection context
///
/// This struct tracks which frameworks are detected in a file based on
/// imports, file paths, and code patterns.
#[derive(Debug, Default)]
pub struct FrameworkContext {
    /// React detected (via import or JSX usage)
    pub is_react: bool,
    /// Next.js detected (via file path or imports)
    pub is_nextjs: bool,
    /// Express.js detected (via imports or patterns)
    pub is_express: bool,
    /// Angular detected (via decorators or imports)
    pub is_angular: bool,
    /// Vue.js detected (via imports or patterns)
    pub is_vue: bool,
    /// Svelte detected
    pub is_svelte: bool,
    /// NestJS detected (via decorators)
    pub is_nestjs: bool,
    /// Fastify detected
    pub is_fastify: bool,
    /// Hono detected
    pub is_hono: bool,
    /// Remix detected
    pub is_remix: bool,
    /// Redux detected (via imports or patterns)
    pub is_redux: bool,
    /// Redux Toolkit detected (modern Redux)
    pub is_redux_toolkit: bool,
}

/// Detect which frameworks are in use based on imports and file patterns
///
/// This is called after core extraction to determine which framework-specific
/// enhancers should run.
pub fn detect_frameworks(summary: &SemanticSummary, source: &str) -> FrameworkContext {
    let mut ctx = FrameworkContext::default();
    let file_lower = summary.file.to_lowercase();

    // Check imports for framework detection
    for dep in &summary.added_dependencies {
        let dep_lower = dep.to_lowercase();

        // React ecosystem
        if dep == "React" || dep_lower == "react" || dep_lower.starts_with("react-") {
            ctx.is_react = true;
        }

        // Next.js specific imports
        if dep_lower.starts_with("next/") || dep_lower == "next" {
            ctx.is_nextjs = true;
            ctx.is_react = true; // Next.js implies React
        }

        // Express
        if dep_lower == "express" || dep == "Router" {
            ctx.is_express = true;
        }

        // Angular
        if dep_lower.starts_with("@angular/") || dep == "Component" || dep == "Injectable" {
            ctx.is_angular = true;
        }

        // Vue
        if dep_lower == "vue" || dep == "defineComponent" || dep == "ref" || dep == "reactive" {
            ctx.is_vue = true;
        }

        // Svelte
        if dep_lower == "svelte" || dep_lower.starts_with("svelte/") {
            ctx.is_svelte = true;
        }

        // NestJS
        if dep_lower.starts_with("@nestjs/") || dep == "Controller" || dep == "Get" || dep == "Post"
        {
            ctx.is_nestjs = true;
        }

        // Fastify
        if dep_lower == "fastify" {
            ctx.is_fastify = true;
        }

        // Hono
        if dep_lower == "hono" {
            ctx.is_hono = true;
        }

        // Remix
        if dep_lower.starts_with("@remix-run/") {
            ctx.is_remix = true;
            ctx.is_react = true; // Remix implies React
        }

        // Redux (old-style)
        if dep_lower == "redux"
            || dep_lower == "react-redux"
            || dep == "createStore"
            || dep == "combineReducers"
            || dep == "applyMiddleware"
            || dep == "connect"
            || dep == "useSelector"
            || dep == "useDispatch"
        {
            ctx.is_redux = true;
        }

        // Redux Toolkit (modern)
        if dep_lower == "@reduxjs/toolkit"
            || dep == "createSlice"
            || dep == "configureStore"
            || dep == "createAsyncThunk"
            || dep == "createApi"
            || dep == "createSelector"
        {
            ctx.is_redux = true;
            ctx.is_redux_toolkit = true;
        }

        // Hooks indicate React
        if dep.starts_with("use") && dep.len() > 3 {
            ctx.is_react = true;
        }
    }

    // File path patterns for Next.js
    if is_nextjs_file_path(&file_lower) {
        ctx.is_nextjs = true;
        ctx.is_react = true;
    }

    // Source code patterns for additional detection
    detect_from_source(&mut ctx, source);

    ctx
}

/// Check if file path indicates Next.js
fn is_nextjs_file_path(file_path: &str) -> bool {
    // App router patterns
    file_path.contains("/app/") && (
        file_path.ends_with("/page.tsx")
        || file_path.ends_with("/page.jsx")
        || file_path.ends_with("/layout.tsx")
        || file_path.ends_with("/layout.jsx")
        || file_path.ends_with("/loading.tsx")
        || file_path.ends_with("/error.tsx")
        || file_path.ends_with("/not-found.tsx")
        || file_path.ends_with("/route.ts")
        || file_path.ends_with("/route.js")
    )
    // Pages router patterns
    || file_path.contains("/pages/")
    // Config files
    || file_path.contains("next.config")
}

/// Detect frameworks from source code patterns
fn detect_from_source(ctx: &mut FrameworkContext, source: &str) {
    // React JSX detection
    if source.contains("<") && (source.contains("/>") || source.contains("</")) {
        // Likely JSX - but could be comparison operators, so check more carefully
        if source.contains("return (") || source.contains("return(") {
            ctx.is_react = true;
        }
    }

    // React hooks
    if source.contains("useState(") || source.contains("useEffect(") || source.contains("useRef(") {
        ctx.is_react = true;
    }

    // Angular decorators
    if source.contains("@Component(")
        || source.contains("@Injectable(")
        || source.contains("@NgModule(")
    {
        ctx.is_angular = true;
    }

    // Express patterns
    if source.contains("app.get(") || source.contains("app.post(") || source.contains("router.") {
        ctx.is_express = true;
    }

    // Vue patterns
    if source.contains("defineComponent(")
        || source.contains("ref(") && source.contains("reactive(")
    {
        ctx.is_vue = true;
    }

    // NestJS decorators
    if source.contains("@Controller(") || source.contains("@Get(") || source.contains("@Post(") {
        ctx.is_nestjs = true;
    }

    // Redux patterns (old-style)
    if source.contains("switch (action.type)")
        || source.contains("switch(action.type)")
        || source.contains("action.type ===")
        || source.contains("combineReducers(")
        || source.contains("createStore(")
        || source.contains("useSelector(")
        || source.contains("useDispatch(")
        || source.contains("connect(")
    {
        ctx.is_redux = true;
    }

    // Redux Toolkit patterns
    if source.contains("createSlice(")
        || source.contains("configureStore(")
        || source.contains("createAsyncThunk(")
        || source.contains("createApi(")
    {
        ctx.is_redux = true;
        ctx.is_redux_toolkit = true;
    }

    // Redux action type constants (old-style)
    // Pattern: export const SET_ACCESS_TOKEN = 'SET_ACCESS_TOKEN'
    if has_action_type_constants(source) {
        ctx.is_redux = true;
    }
}

/// Check if source contains Redux action type constant patterns
fn has_action_type_constants(source: &str) -> bool {
    // Look for SCREAMING_SNAKE_CASE exports with action-like prefixes
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with("export const ") || line.starts_with("const ") {
            // Extract the constant name
            let after_const = if line.starts_with("export const ") {
                &line[13..]
            } else {
                &line[6..]
            };

            // Find the name (before = or :)
            if let Some(name_end) = after_const.find(|c| c == '=' || c == ':' || c == ' ') {
                let name = after_const[..name_end].trim();

                // Check if it's SCREAMING_SNAKE_CASE with action-like prefix
                if is_action_type_constant(name) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a constant name looks like a Redux action type
fn is_action_type_constant(name: &str) -> bool {
    // Must be SCREAMING_SNAKE_CASE (all caps, underscores, maybe numbers)
    if !name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
    {
        return false;
    }

    // Must have at least one underscore (to distinguish from single-word constants)
    if !name.contains('_') {
        return false;
    }

    // Must contain action-like prefixes/suffixes
    let lower = name.to_lowercase();
    lower.starts_with("set_")
        || lower.starts_with("get_")
        || lower.starts_with("fetch_")
        || lower.starts_with("load_")
        || lower.starts_with("update_")
        || lower.starts_with("delete_")
        || lower.starts_with("add_")
        || lower.starts_with("remove_")
        || lower.starts_with("clear_")
        || lower.starts_with("reset_")
        || lower.starts_with("toggle_")
        || lower.ends_with("_request")
        || lower.ends_with("_success")
        || lower.ends_with("_failure")
        || lower.ends_with("_error")
        || lower.ends_with("_pending")
        || lower.ends_with("_fulfilled")
        || lower.ends_with("_rejected")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nextjs_file_detection() {
        assert!(is_nextjs_file_path("/app/page.tsx"));
        assert!(is_nextjs_file_path("/app/api/route.ts"));
        assert!(is_nextjs_file_path("/app/dashboard/layout.tsx"));
        assert!(is_nextjs_file_path("/pages/index.tsx"));
        assert!(!is_nextjs_file_path("/src/components/Button.tsx"));
    }

    #[test]
    fn test_framework_detection_react() {
        let mut summary = SemanticSummary::default();
        summary.added_dependencies.push("React".to_string());
        summary.added_dependencies.push("useState".to_string());

        let ctx = detect_frameworks(&summary, "");
        assert!(ctx.is_react);
        assert!(!ctx.is_nextjs);
    }

    #[test]
    fn test_framework_detection_nextjs() {
        let mut summary = SemanticSummary::default();
        summary.file = "/app/page.tsx".to_string();

        let ctx = detect_frameworks(&summary, "");
        assert!(ctx.is_nextjs);
        assert!(ctx.is_react); // Next.js implies React
    }

    #[test]
    fn test_framework_detection_express() {
        let mut summary = SemanticSummary::default();
        summary.added_dependencies.push("express".to_string());

        let ctx = detect_frameworks(&summary, "app.get('/api')");
        assert!(ctx.is_express);
    }

    #[test]
    fn test_framework_detection_angular() {
        let summary = SemanticSummary::default();
        let source = "@Component({ selector: 'app-root' })";

        let ctx = detect_frameworks(&summary, source);
        assert!(ctx.is_angular);
    }

    #[test]
    fn test_framework_detection_redux_switch() {
        let summary = SemanticSummary::default();
        let source = r#"
            export const reducer = (state, action) => {
                switch (action.type) {
                    case SET_TOKEN:
                        return { ...state, token: action.payload }
                }
            }
        "#;

        let ctx = detect_frameworks(&summary, source);
        assert!(
            ctx.is_redux,
            "Redux should be detected from switch (action.type)"
        );
    }

    #[test]
    fn test_framework_detection_redux_imports() {
        let mut summary = SemanticSummary::default();
        summary.added_dependencies.push("useSelector".to_string());
        summary.added_dependencies.push("useDispatch".to_string());

        let ctx = detect_frameworks(&summary, "");
        assert!(
            ctx.is_redux,
            "Redux should be detected from useSelector/useDispatch imports"
        );
    }

    #[test]
    fn test_framework_detection_redux_toolkit() {
        let summary = SemanticSummary::default();
        let source = "const slice = createSlice({ name: 'auth', reducers: {} })";

        let ctx = detect_frameworks(&summary, source);
        assert!(ctx.is_redux, "Redux should be detected from createSlice");
        assert!(
            ctx.is_redux_toolkit,
            "Redux Toolkit should be detected from createSlice"
        );
    }

    #[test]
    fn test_framework_detection_redux_action_types() {
        let summary = SemanticSummary::default();
        let source = r#"
            export const SET_ACCESS_TOKEN = 'SET_ACCESS_TOKEN'
            export const FETCH_USER_REQUEST = 'user/FETCH_REQUEST'
        "#;

        let ctx = detect_frameworks(&summary, source);
        assert!(
            ctx.is_redux,
            "Redux should be detected from action type constants"
        );
    }

    #[test]
    fn test_action_type_constant_detection() {
        assert!(is_action_type_constant("SET_ACCESS_TOKEN"));
        assert!(is_action_type_constant("FETCH_USER_REQUEST"));
        assert!(is_action_type_constant("ADD_TODO"));
        assert!(is_action_type_constant("CLEAR_ERRORS"));
        assert!(is_action_type_constant("UPDATE_USER_SUCCESS"));
        assert!(!is_action_type_constant("setAccessToken")); // camelCase
        assert!(!is_action_type_constant("CONSTANT")); // no underscore
        assert!(!is_action_type_constant("SOME_RANDOM_THING")); // no action-like keyword
    }
}
