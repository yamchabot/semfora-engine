//! Next.js Framework Detector
//!
//! Specialized extraction for Next.js applications including:
//! - App Router patterns (page.tsx, layout.tsx, route.ts)
//! - Pages Router patterns
//! - API routes (GET, POST, PUT, DELETE handlers)
//! - Server Components vs Client Components
//! - Middleware detection
//! - Config files (next.config.js)

use crate::detectors::common::push_unique_insertion;
use crate::schema::{FrameworkEntryPoint, SemanticSummary, SymbolKind};

/// Enhance semantic summary with Next.js-specific information
///
/// This is called when Next.js is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, source: &str) {
    let file_lower = summary.file.to_lowercase();

    // Detect file type and add appropriate insertions + framework entry points
    detect_app_router_patterns(summary, &file_lower, source);
    detect_pages_router_patterns(summary, &file_lower, source);
    detect_api_routes(summary, &file_lower, source);
    detect_middleware(summary, &file_lower, source);
    detect_config_files(summary, &file_lower, source);
    detect_server_client_components(summary, source);
    detect_data_fetching(summary, source);

    // Also set framework_entry_point on individual symbols
    super::propagate_entry_point_to_symbols(summary);
}

// =============================================================================
// App Router Detection
// =============================================================================

/// Detect App Router file patterns
fn detect_app_router_patterns(summary: &mut SemanticSummary, file_lower: &str, _source: &str) {
    // Page component (app/ directory)
    if file_lower.ends_with("/page.tsx") || file_lower.ends_with("/page.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextPage;
        if summary.symbol_kind == Some(SymbolKind::Component) {
            push_unique_insertion(
                &mut summary.insertions,
                "Next.js page component".to_string(),
                "Next.js page",
            );
        }
    }

    // Layout component
    if file_lower.ends_with("/layout.tsx") || file_lower.ends_with("/layout.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextLayout;
        if summary.symbol_kind == Some(SymbolKind::Component) {
            push_unique_insertion(
                &mut summary.insertions,
                "Next.js layout component".to_string(),
                "Next.js layout",
            );
        }
    }

    // Loading component
    if file_lower.ends_with("/loading.tsx") || file_lower.ends_with("/loading.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextPage;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js loading component".to_string(),
            "Next.js loading",
        );
    }

    // Error component
    if file_lower.ends_with("/error.tsx") || file_lower.ends_with("/error.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js error boundary".to_string(),
            "Next.js error",
        );
    }

    // Not found component
    if file_lower.ends_with("/not-found.tsx") || file_lower.ends_with("/not-found.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js not-found page".to_string(),
            "Next.js not-found",
        );
    }

    // Template component
    if file_lower.ends_with("/template.tsx") || file_lower.ends_with("/template.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextLayout;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js template component".to_string(),
            "Next.js template",
        );
    }

    // Default component (parallel routes)
    if file_lower.ends_with("/default.tsx") || file_lower.ends_with("/default.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextPage;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js default component (parallel route)".to_string(),
            "Next.js default",
        );
    }
}

/// Detect Pages Router file patterns
fn detect_pages_router_patterns(summary: &mut SemanticSummary, file_lower: &str, source: &str) {
    if !file_lower.contains("/pages/") {
        return;
    }

    // All files in pages/ directory are page components (unless special files)
    // Set default to NextPage, may be overridden below
    if summary.framework_entry_point.is_none() {
        summary.framework_entry_point = FrameworkEntryPoint::NextPage;
    }

    // _app.tsx - special file
    if file_lower.ends_with("/_app.tsx") || file_lower.ends_with("/_app.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js custom App component".to_string(),
            "Next.js App",
        );
    }

    // _document.tsx - special file
    if file_lower.ends_with("/_document.tsx") || file_lower.ends_with("/_document.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js custom Document".to_string(),
            "Next.js Document",
        );
    }

    // _error.tsx - special file
    if file_lower.ends_with("/_error.tsx") || file_lower.ends_with("/_error.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
    }

    // 404.tsx - special file
    if file_lower.ends_with("/404.tsx") || file_lower.ends_with("/404.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
    }

    // 500.tsx - special file
    if file_lower.ends_with("/500.tsx") || file_lower.ends_with("/500.jsx") {
        summary.framework_entry_point = FrameworkEntryPoint::NextSpecialFile;
    }

    // getServerSideProps detection - mark as SSR entry point
    if source.contains("getServerSideProps") {
        // Mark individual symbols with getServerSideProps name as data fetching
        for symbol in &mut summary.symbols {
            if symbol.name == "getServerSideProps" {
                symbol.framework_entry_point = FrameworkEntryPoint::NextDataFetching;
            }
        }
        push_unique_insertion(
            &mut summary.insertions,
            "server-side rendering (SSR)".to_string(),
            "SSR",
        );
    }

    // getStaticProps detection
    if source.contains("getStaticProps") {
        for symbol in &mut summary.symbols {
            if symbol.name == "getStaticProps" {
                symbol.framework_entry_point = FrameworkEntryPoint::NextDataFetching;
            }
        }
        push_unique_insertion(
            &mut summary.insertions,
            "static site generation (SSG)".to_string(),
            "SSG",
        );
    }

    // getStaticPaths detection
    if source.contains("getStaticPaths") {
        for symbol in &mut summary.symbols {
            if symbol.name == "getStaticPaths" {
                symbol.framework_entry_point = FrameworkEntryPoint::NextDataFetching;
            }
        }
        push_unique_insertion(
            &mut summary.insertions,
            "dynamic static paths".to_string(),
            "static paths",
        );
    }

    // getInitialProps detection (legacy)
    if source.contains("getInitialProps") {
        for symbol in &mut summary.symbols {
            if symbol.name == "getInitialProps" {
                symbol.framework_entry_point = FrameworkEntryPoint::NextDataFetching;
            }
        }
    }
}

// =============================================================================
// API Route Detection
// =============================================================================

/// Detect API route handlers
fn detect_api_routes(summary: &mut SemanticSummary, file_lower: &str, _source: &str) {
    // App Router API routes (route.ts/route.js in api/ directory)
    if file_lower.contains("/api/")
        && (file_lower.ends_with("/route.ts") || file_lower.ends_with("/route.js"))
    {
        summary.framework_entry_point = FrameworkEntryPoint::NextApiRoute;

        if let Some(ref sym) = summary.symbol {
            let method = sym.to_uppercase();
            if matches!(
                method.as_str(),
                "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
            ) {
                push_unique_insertion(
                    &mut summary.insertions,
                    format!("Next.js API route ({})", method),
                    "API route",
                );
            }
        }

        // Mark HTTP method handlers as API routes
        for symbol in &mut summary.symbols {
            let name_upper = symbol.name.to_uppercase();
            if matches!(
                name_upper.as_str(),
                "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
            ) {
                symbol.framework_entry_point = FrameworkEntryPoint::NextApiRoute;
            }
        }

        // Check for multiple exported methods for insertion
        let methods: Vec<String> = summary
            .symbols
            .iter()
            .map(|s| s.name.to_uppercase())
            .filter(|n| {
                matches!(
                    n.as_str(),
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
                )
            })
            .collect();

        if methods.len() > 1 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("Next.js API route ({} handlers)", methods.join(", ")),
                "API route",
            );
        }
    }

    // Pages Router API routes (files in pages/api/)
    if file_lower.contains("/pages/api/") {
        summary.framework_entry_point = FrameworkEntryPoint::NextApiRoute;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js Pages API route".to_string(),
            "Pages API",
        );
    }
}

// =============================================================================
// Middleware Detection
// =============================================================================

/// Detect Next.js middleware
fn detect_middleware(summary: &mut SemanticSummary, file_lower: &str, source: &str) {
    if file_lower.ends_with("/middleware.ts") || file_lower.ends_with("/middleware.js") {
        summary.framework_entry_point = FrameworkEntryPoint::NextMiddleware;
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js middleware".to_string(),
            "middleware",
        );

        // Mark the middleware function export as framework entry point
        for symbol in &mut summary.symbols {
            if symbol.name == "middleware" || symbol.is_default_export {
                symbol.framework_entry_point = FrameworkEntryPoint::NextMiddleware;
            }
        }

        // Check for matcher config
        if source.contains("matcher") {
            push_unique_insertion(
                &mut summary.insertions,
                "route matcher configured".to_string(),
                "matcher",
            );
        }
    }
}

// =============================================================================
// Config Files
// =============================================================================

/// Detect Next.js config files
fn detect_config_files(summary: &mut SemanticSummary, file_lower: &str, source: &str) {
    if file_lower.contains("next.config") {
        push_unique_insertion(
            &mut summary.insertions,
            "Next.js configuration".to_string(),
            "Next.js config",
        );

        // Detect specific configurations
        if source.contains("images") {
            push_unique_insertion(
                &mut summary.insertions,
                "image optimization config".to_string(),
                "images",
            );
        }

        if source.contains("rewrites") || source.contains("redirects") {
            push_unique_insertion(
                &mut summary.insertions,
                "URL rewrites/redirects".to_string(),
                "rewrites",
            );
        }

        if source.contains("experimental") {
            push_unique_insertion(
                &mut summary.insertions,
                "experimental features enabled".to_string(),
                "experimental",
            );
        }
    }

    // Tailwind config (commonly used with Next.js)
    if file_lower.contains("tailwind.config") {
        push_unique_insertion(
            &mut summary.insertions,
            "Tailwind CSS configuration".to_string(),
            "Tailwind",
        );
    }
}

// =============================================================================
// Server/Client Component Detection
// =============================================================================

/// Detect Server vs Client components
fn detect_server_client_components(summary: &mut SemanticSummary, source: &str) {
    // Client component directive
    if source.trim_start().starts_with("'use client'")
        || source.trim_start().starts_with("\"use client\"")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "client component".to_string(),
            "client",
        );
    }

    // Server component directive (explicit)
    if source.trim_start().starts_with("'use server'")
        || source.trim_start().starts_with("\"use server\"")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "server actions".to_string(),
            "server",
        );
    }
}

// =============================================================================
// Data Fetching Detection
// =============================================================================

/// Detect data fetching patterns
fn detect_data_fetching(summary: &mut SemanticSummary, source: &str) {
    // Network data fetching
    if source.contains("fetch(") {
        push_unique_insertion(
            &mut summary.insertions,
            "network data fetching".to_string(),
            "fetch",
        );

        // Revalidation
        if source.contains("revalidate") || source.contains("next: { revalidate") {
            push_unique_insertion(
                &mut summary.insertions,
                "ISR revalidation".to_string(),
                "ISR",
            );
        }
    }

    // Cache configuration
    if source.contains("cache:") || source.contains("'no-store'") || source.contains("\"no-store\"")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "cache configuration".to_string(),
            "cache",
        );
    }

    // Dynamic rendering
    if source.contains("dynamic =") || source.contains("export const dynamic") {
        push_unique_insertion(
            &mut summary.insertions,
            "dynamic rendering config".to_string(),
            "dynamic",
        );
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Check if file is a Next.js special file
pub fn is_special_file(file_path: &str) -> bool {
    let file_lower = file_path.to_lowercase();
    file_lower.ends_with("/page.tsx")
        || file_lower.ends_with("/page.jsx")
        || file_lower.ends_with("/layout.tsx")
        || file_lower.ends_with("/layout.jsx")
        || file_lower.ends_with("/route.ts")
        || file_lower.ends_with("/route.js")
        || file_lower.ends_with("/loading.tsx")
        || file_lower.ends_with("/error.tsx")
        || file_lower.ends_with("/not-found.tsx")
        || file_lower.ends_with("/middleware.ts")
        || file_lower.ends_with("/middleware.js")
}

/// Extract the route path from a file path
pub fn extract_route_path(file_path: &str) -> Option<String> {
    if let Some(app_index) = file_path.find("/app/") {
        let after_app = &file_path[app_index + 5..];
        let route = after_app
            // Handle both /page.tsx and page.tsx (for root app directory)
            .replace("/page.tsx", "")
            .replace("/page.jsx", "")
            .replace("/route.ts", "")
            .replace("/route.js", "")
            .replace("page.tsx", "")
            .replace("page.jsx", "")
            .replace("route.ts", "")
            .replace("route.js", "");

        if route.is_empty() {
            Some("/".to_string())
        } else {
            Some(format!("/{}", route))
        }
    } else if let Some(pages_index) = file_path.find("/pages/") {
        let after_pages = &file_path[pages_index + 7..];
        let route = after_pages
            .replace(".tsx", "")
            .replace(".jsx", "")
            .replace("/index", "")
            .replace("index", "");

        if route.is_empty() {
            Some("/".to_string())
        } else {
            Some(format!("/{}", route))
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_special_file() {
        assert!(is_special_file("/app/page.tsx"));
        assert!(is_special_file("/app/dashboard/layout.tsx"));
        assert!(is_special_file("/app/api/users/route.ts"));
        assert!(!is_special_file("/src/components/Button.tsx"));
    }

    #[test]
    fn test_extract_route_path() {
        assert_eq!(extract_route_path("/app/page.tsx"), Some("/".to_string()));
        assert_eq!(
            extract_route_path("/app/dashboard/page.tsx"),
            Some("/dashboard".to_string())
        );
        assert_eq!(
            extract_route_path("/app/api/users/route.ts"),
            Some("/api/users".to_string())
        );
        assert_eq!(
            extract_route_path("/pages/index.tsx"),
            Some("/".to_string())
        );
        assert_eq!(
            extract_route_path("/pages/about.tsx"),
            Some("/about".to_string())
        );
    }
}
