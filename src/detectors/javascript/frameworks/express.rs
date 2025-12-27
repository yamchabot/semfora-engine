//! Express.js Framework Detector
//!
//! Specialized extraction for Express.js/Node.js applications including:
//! - Route handlers (GET, POST, PUT, DELETE, PATCH)
//! - Middleware functions
//! - Router definitions
//! - Error handling middleware
//! - Static file serving

use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::schema::{FrameworkEntryPoint, SemanticSummary, SymbolKind};

/// Enhance semantic summary with Express-specific information
///
/// This is called when Express is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Extract route handlers
    extract_route_handlers(summary, root, source);

    // Extract middleware
    extract_middleware(summary, root, source);

    // Detect router usage
    detect_router_patterns(summary, source);

    // Detect error handlers
    detect_error_handlers(summary, source);

    // Detect common patterns
    detect_common_patterns(summary, source);

    // Set framework entry points
    detect_entry_points(summary, source);
}

/// Detect and mark Express entry points
fn detect_entry_points(summary: &mut SemanticSummary, source: &str) {
    // Entry point detection (app.listen)
    if is_entry_point(source) {
        summary.framework_entry_point = FrameworkEntryPoint::ExpressRoute;

        // Mark listen-related symbols
        for symbol in &mut summary.symbols {
            if symbol.name == "app"
                || symbol.name.contains("server")
                || symbol.name.contains("listen")
            {
                symbol.framework_entry_point = FrameworkEntryPoint::ExpressRoute;
            }
        }
    }

    // Route file detection
    if is_route_file(source) {
        summary.framework_entry_point = FrameworkEntryPoint::ExpressRoute;

        // Mark exported functions as route handlers
        for symbol in &mut summary.symbols {
            if symbol.is_exported && symbol.kind == SymbolKind::Function {
                symbol.framework_entry_point = FrameworkEntryPoint::ExpressRoute;
            }
        }
    }

    // Middleware file detection
    if is_middleware_file(source) {
        summary.framework_entry_point = FrameworkEntryPoint::ExpressMiddleware;

        // Mark exported functions as middleware
        for symbol in &mut summary.symbols {
            if symbol.is_exported && symbol.kind == SymbolKind::Function {
                symbol.framework_entry_point = FrameworkEntryPoint::ExpressMiddleware;
            }
        }
    }
}

// =============================================================================
// Route Handler Detection
// =============================================================================

/// HTTP methods supported by Express
const HTTP_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "options", "head", "all",
];

/// Extract Express route handlers
///
/// Detects patterns like:
/// ```javascript
/// app.get('/users', handler);
/// router.post('/api/data', middleware, handler);
/// ```
pub fn extract_route_handlers(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut routes: Vec<(String, String)> = Vec::new(); // (method, path)

    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some((method, path)) = extract_route_info(node, source) {
                routes.push((method, path));
            }
        }
    });

    // Summarize routes
    if !routes.is_empty() {
        let method_counts = count_methods(&routes);

        for (method, count) in method_counts {
            push_unique_insertion(
                &mut summary.insertions,
                format!("{} {} route handlers", count, method.to_uppercase()),
                &format!("{} routes", method),
            );
        }

        // Note if there are many routes
        if routes.len() >= 5 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("{} total routes defined", routes.len()),
                "routes total",
            );
        }
    }
}

/// Extract route information from a call expression
fn extract_route_info(node: &Node, source: &str) -> Option<(String, String)> {
    let func = node.child_by_field_name("function")?;

    // Check for member expression (app.get, router.post, etc.)
    if func.kind() == "member_expression" {
        let property = func.child_by_field_name("property")?;
        let method = get_node_text(&property, source).to_lowercase();

        if HTTP_METHODS.contains(&method.as_str()) {
            // Extract the path from arguments
            if let Some(args) = node.child_by_field_name("arguments") {
                if let Some(first_arg) = args.child(1) {
                    // Skip '('
                    let path = get_node_text(&first_arg, source);
                    let path = path.trim_matches('"').trim_matches('\'').trim_matches('`');
                    return Some((method, path.to_string()));
                }
            }
            return Some((method, "/*".to_string()));
        }
    }

    None
}

/// Count routes by HTTP method
fn count_methods(routes: &[(String, String)]) -> Vec<(String, usize)> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for (method, _) in routes {
        *counts.entry(method.clone()).or_insert(0) += 1;
    }

    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count descending
    result
}

// =============================================================================
// Middleware Detection
// =============================================================================

/// Extract middleware usage
fn extract_middleware(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut middleware_count = 0;

    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if is_middleware_use(node, source) {
                middleware_count += 1;
            }
        }
    });

    if middleware_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} middleware registered", middleware_count),
            "middleware",
        );
    }
}

/// Check if a call expression is app.use() or router.use()
fn is_middleware_use(node: &Node, source: &str) -> bool {
    if let Some(func) = node.child_by_field_name("function") {
        if func.kind() == "member_expression" {
            if let Some(property) = func.child_by_field_name("property") {
                let method = get_node_text(&property, source);
                return method == "use";
            }
        }
    }
    false
}

// =============================================================================
// Router Pattern Detection
// =============================================================================

/// Detect Express Router usage patterns
fn detect_router_patterns(summary: &mut SemanticSummary, source: &str) {
    // Router instantiation
    if source.contains("express.Router()") || source.contains("Router()") {
        push_unique_insertion(
            &mut summary.insertions,
            "Express Router module".to_string(),
            "Router",
        );
    }

    // Router mounting
    if source.contains(".use(") && (source.contains("'/api") || source.contains("\"/api")) {
        push_unique_insertion(
            &mut summary.insertions,
            "API router mounted".to_string(),
            "API router",
        );
    }
}

// =============================================================================
// Error Handler Detection
// =============================================================================

/// Detect error handling middleware
fn detect_error_handlers(summary: &mut SemanticSummary, source: &str) {
    // Error handler signature: (err, req, res, next) => or function(err, req, res, next)
    if source.contains("(err, req, res, next)")
        || source.contains("(error, req, res, next)")
        || source.contains("(err,req,res,next)")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "error handling middleware".to_string(),
            "error handler",
        );
    }

    // Common error packages
    if source.contains("http-errors") || source.contains("createError") {
        push_unique_insertion(
            &mut summary.insertions,
            "HTTP error handling".to_string(),
            "http-errors",
        );
    }
}

// =============================================================================
// Common Pattern Detection
// =============================================================================

/// Detect common Express patterns
fn detect_common_patterns(summary: &mut SemanticSummary, source: &str) {
    // JSON body parsing
    if source.contains("express.json()") || source.contains("bodyParser.json()") {
        push_unique_insertion(
            &mut summary.insertions,
            "JSON body parsing".to_string(),
            "JSON body",
        );
    }

    // URL encoded body parsing
    if source.contains("express.urlencoded") || source.contains("bodyParser.urlencoded") {
        push_unique_insertion(
            &mut summary.insertions,
            "URL-encoded body parsing".to_string(),
            "urlencoded",
        );
    }

    // Static file serving
    if source.contains("express.static") {
        push_unique_insertion(
            &mut summary.insertions,
            "static file serving".to_string(),
            "static",
        );
    }

    // CORS
    if source.contains("cors(") || source.contains("cors()") {
        push_unique_insertion(&mut summary.insertions, "CORS enabled".to_string(), "CORS");
    }

    // Helmet security
    if source.contains("helmet(") || source.contains("helmet()") {
        push_unique_insertion(
            &mut summary.insertions,
            "Helmet security headers".to_string(),
            "Helmet",
        );
    }

    // Session management
    if source.contains("express-session") || source.contains("session(") {
        push_unique_insertion(
            &mut summary.insertions,
            "session management".to_string(),
            "session",
        );
    }

    // Cookie parsing
    if source.contains("cookie-parser") || source.contains("cookieParser") {
        push_unique_insertion(
            &mut summary.insertions,
            "cookie parsing".to_string(),
            "cookies",
        );
    }

    // Compression
    if source.contains("compression(") || source.contains("compression()") {
        push_unique_insertion(
            &mut summary.insertions,
            "response compression".to_string(),
            "compression",
        );
    }

    // Morgan logging
    if source.contains("morgan(") {
        push_unique_insertion(
            &mut summary.insertions,
            "request logging (morgan)".to_string(),
            "morgan",
        );
    }

    // Rate limiting
    if source.contains("rateLimit") || source.contains("express-rate-limit") {
        push_unique_insertion(
            &mut summary.insertions,
            "rate limiting".to_string(),
            "rate limit",
        );
    }

    // Server listening
    if source.contains(".listen(") {
        push_unique_insertion(
            &mut summary.insertions,
            "HTTP server entry point".to_string(),
            "listen",
        );
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Check if file appears to be an Express app entry point
pub fn is_entry_point(source: &str) -> bool {
    source.contains(".listen(") && (source.contains("express()") || source.contains("createServer"))
}

/// Check if file is a route definitions file
pub fn is_route_file(source: &str) -> bool {
    let route_patterns: usize = HTTP_METHODS
        .iter()
        .filter(|method| source.contains(&format!(".{}(", method)))
        .count();
    route_patterns >= 2
}

/// Check if file is a middleware file
pub fn is_middleware_file(source: &str) -> bool {
    // Middleware typically exports a function with (req, res, next) signature
    source.contains("(req, res, next)") || source.contains("(req,res,next)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_entry_point() {
        assert!(is_entry_point("const app = express(); app.listen(3000);"));
        assert!(!is_entry_point("router.get('/api', handler);"));
    }

    #[test]
    fn test_is_route_file() {
        let routes = r#"
            router.get('/users', getUsers);
            router.post('/users', createUser);
            router.delete('/users/:id', deleteUser);
        "#;
        assert!(is_route_file(routes));
        assert!(!is_route_file("const x = 1;"));
    }

    #[test]
    fn test_is_middleware_file() {
        assert!(is_middleware_file(
            "module.exports = (req, res, next) => { next(); }"
        ));
        assert!(!is_middleware_file("const handler = (data) => data;"));
    }
}
