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
pub mod nextjs;
pub mod react;
pub mod vue;

use crate::schema::SemanticSummary;

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
}
