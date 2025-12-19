//! TOON (Token-Oriented Object Notation) encoder using rtoon library
//!
//! TOON encoding rules from specification:
//! - Objects -> indented blocks
//! - Uniform arrays -> tabular blocks
//! - Strings quoted only if necessary
//! - Field headers emitted once per array
//! - Stable field ordering enforced

use std::collections::HashMap;

use rtoon::encode_default;
use serde_json::{json, Map, Value};

use crate::analysis::{calculate_cognitive_complexity, max_nesting_depth};
use crate::schema::{ModuleGroup, RepoOverview, RepoStats, RiskLevel, SemanticSummary, SymbolKind};
use crate::shard::extract_module_name;
use crate::utils::truncate_to_char_boundary;

// ============================================================================
// Noisy call filtering - these are implementation details, not architecture
// ============================================================================

/// Array/collection methods that are implementation noise
const NOISY_ARRAY_METHODS: &[&str] = &[
    "includes",
    "filter",
    "map",
    "reduce",
    "forEach",
    "find",
    "findIndex",
    "some",
    "every",
    "slice",
    "splice",
    "push",
    "pop",
    "shift",
    "unshift",
    "concat",
    "join",
    "sort",
    "reverse",
    "indexOf",
    "lastIndexOf",
    "flat",
    "flatMap",
    "fill",
    "copyWithin",
    "entries",
    "keys",
    "values",
    "at",
];

/// Promise chain methods - the actual logic inside is captured separately
const NOISY_PROMISE_METHODS: &[&str] = &["then", "catch", "finally"];

/// ORM/Schema builder methods - these are declarations, not runtime behavior
const NOISY_SCHEMA_METHODS: &[&str] = &[
    "notNull",
    "primaryKey",
    "default",
    "references",
    "unique",
    "index",
    "serial",
    "text",
    "integer",
    "bigint",
    "boolean",
    "timestamp",
    "jsonb",
    "varchar",
    "char",
    "numeric",
    "real",
    "double",
    "date",
    "time",
    "uuid",
];

/// Math methods that are implementation noise
const NOISY_MATH_METHODS: &[&str] = &[
    "floor", "ceil", "round", "random", "abs", "sqrt", "pow", "min", "max", "sin", "cos", "tan",
    "log", "exp",
];

/// String methods that are implementation noise
const NOISY_STRING_METHODS: &[&str] = &[
    "split",
    "trim",
    "toLowerCase",
    "toUpperCase",
    "substring",
    "substr",
    "charAt",
    "charCodeAt",
    "replace",
    "replaceAll",
    "match",
    "search",
    "startsWith",
    "endsWith",
    "padStart",
    "padEnd",
    "repeat",
];

/// Object methods that are implementation noise
const NOISY_OBJECT_METHODS: &[&str] = &[
    "keys",
    "values",
    "entries",
    "assign",
    "freeze",
    "seal",
    "hasOwnProperty",
    "toString",
    "valueOf",
];

/// HTTP methods for API calls
const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// API/HTTP client libraries
const API_CLIENT_NAMES: &[&str] = &[
    "axios",
    "fetch",
    "ky",
    "got",
    "superagent",
    "request",
    "invoke",
];

/// React Query / TanStack Query hooks
const REACT_QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useMutation",
    "useInfiniteQuery",
    "useQueries",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "usePrefetchQuery",
    "queryClient",
    "useQueryClient",
];

/// SWR hooks
const SWR_HOOKS: &[&str] = &["useSWR", "useSWRMutation", "useSWRInfinite", "useSWRConfig"];

/// Apollo GraphQL hooks
const APOLLO_HOOKS: &[&str] = &[
    "useApolloClient",
    "useLazyQuery",
    "useSubscription",
    "useReactiveVar",
    "useSuspenseQuery_experimental",
];

/// Check if a call is meaningful (not noise)
pub fn is_meaningful_call(name: &str, object: Option<&str>) -> bool {
    // Always keep React hooks (useState, useEffect, etc.)
    if name.starts_with("use")
        && name
            .chars()
            .nth(3)
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    {
        return true;
    }

    // Always keep state setters
    if name.starts_with("set")
        && name
            .chars()
            .nth(3)
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    {
        return true;
    }

    // React Query / TanStack Query
    if REACT_QUERY_HOOKS.contains(&name) {
        return true;
    }

    // SWR
    if SWR_HOOKS.contains(&name) {
        return true;
    }

    // Apollo GraphQL
    if APOLLO_HOOKS.contains(&name) {
        return true;
    }

    // Direct API client calls (fetch, axios, ky, etc.)
    if API_CLIENT_NAMES.contains(&name) {
        return true;
    }

    // HTTP methods on API clients (axios.get, ky.post, etc.)
    if let Some(obj) = object {
        if API_CLIENT_NAMES.contains(&obj) && HTTP_METHODS.contains(&name) {
            return true;
        }
    }

    // Always keep I/O and database calls
    if matches!(
        name,
        "insert" | "select" | "update" | "delete" | "query" | "execute" | "migrate" | "mutate"
    ) {
        return true;
    }

    // Filter promise chain methods (logic inside is captured separately)
    if NOISY_PROMISE_METHODS.contains(&name) {
        return false;
    }

    // Filter ORM/schema builder methods (declarations, not runtime)
    if NOISY_SCHEMA_METHODS.contains(&name) {
        return false;
    }

    // Filter based on object
    if let Some(obj) = object {
        // Math methods are noise
        if obj == "Math" && NOISY_MATH_METHODS.contains(&name) {
            return false;
        }

        // Object methods are noise
        if obj == "Object" && NOISY_OBJECT_METHODS.contains(&name) {
            return false;
        }

        // Array-like methods on data objects are noise
        if NOISY_ARRAY_METHODS.contains(&name) {
            return false;
        }

        // String methods are noise
        if NOISY_STRING_METHODS.contains(&name) {
            return false;
        }

        // Keep database, Response, process calls
        if matches!(
            obj,
            "db" | "Response" | "process" | "console" | "document" | "window"
        ) {
            return true;
        }
    }

    // Filter standalone noisy calls
    if NOISY_ARRAY_METHODS.contains(&name) {
        return false;
    }

    // Keep require, drizzle, postgres, etc.
    true
}

/// Filter calls to only meaningful ones
pub fn filter_meaningful_calls(calls: &[crate::schema::Call]) -> Vec<crate::schema::Call> {
    calls
        .iter()
        .filter(|c| is_meaningful_call(&c.name, c.object.as_deref()))
        .cloned()
        .collect()
}

// ============================================================================
// Repository Overview Generation
// ============================================================================

/// Generate a repository overview from analyzed summaries
pub fn generate_repo_overview(summaries: &[SemanticSummary], dir_path: &str) -> RepoOverview {
    generate_repo_overview_with_modules(summaries, dir_path, None)
}

/// Generate a repository overview with an optional file-to-module mapping.
///
/// When `file_to_module` is provided, it's used instead of `extract_module_name`
/// for consistent naming with module shards (conflict-aware stripping).
pub fn generate_repo_overview_with_modules(
    summaries: &[SemanticSummary],
    dir_path: &str,
    file_to_module: Option<&HashMap<String, String>>,
) -> RepoOverview {
    let mut overview = RepoOverview::default();

    // Detect framework
    overview.framework = detect_framework(summaries);

    // Detect database
    overview.database = detect_database(summaries);

    // Detect package manager
    overview.package_manager = detect_package_manager(summaries);

    // Build module groups (using provided mapping if available)
    overview.modules = build_module_groups_with_map(summaries, dir_path, file_to_module);

    // Identify entry points
    overview.entry_points = identify_entry_points(summaries);

    // Build data flow
    overview.data_flow = build_data_flow(summaries);

    // Build stats
    overview.stats = build_stats(summaries);

    // Detect patterns
    overview.patterns = detect_patterns(summaries);

    overview
}

fn detect_framework(summaries: &[SemanticSummary]) -> Option<String> {
    let mut frameworks = Vec::new();

    // Rust detection
    let has_cargo = summaries
        .iter()
        .any(|s| s.file.to_lowercase().ends_with("cargo.toml"));
    let has_main_rs = summaries.iter().any(|s| s.file.ends_with("main.rs"));
    let has_lib_rs = summaries.iter().any(|s| s.file.ends_with("lib.rs"));

    if has_cargo || has_main_rs || has_lib_rs {
        let rust_type = match (has_main_rs, has_lib_rs) {
            (true, true) => "Rust (bin+lib)",
            (true, false) => "Rust (binary)",
            (false, true) => "Rust (library)",
            _ => "Rust",
        };
        frameworks.push(rust_type);
    }

    // Go detection
    let has_go_mod = summaries
        .iter()
        .any(|s| s.file.to_lowercase().ends_with("go.mod"));
    let go_count = summaries.iter().filter(|s| s.language == "go").count();
    if has_go_mod || go_count > 0 {
        frameworks.push("Go");
    }

    // Python detection
    let has_pyproject = summaries
        .iter()
        .any(|s| s.file.to_lowercase().contains("pyproject.toml"));
    let has_setup_py = summaries
        .iter()
        .any(|s| s.file.to_lowercase().contains("setup.py"));
    let python_count = summaries.iter().filter(|s| s.language == "python").count();
    if has_pyproject || has_setup_py || python_count > 2 {
        frameworks.push("Python");
    }

    // JavaScript/TypeScript framework detection
    for s in summaries {
        let file_lower = s.file.to_lowercase();

        // Next.js
        if file_lower.contains("next.config") || file_lower.contains("/app/layout") {
            if !frameworks.iter().any(|f| f.contains("Next.js")) {
                frameworks.push("Next.js (App Router)");
            }
        }
        if file_lower.contains("/pages/")
            && (file_lower.ends_with(".tsx") || file_lower.ends_with(".jsx"))
        {
            if !frameworks.iter().any(|f| f.contains("Next.js")) {
                frameworks.push("Next.js (Pages Router)");
            }
        }

        // Express
        if s.added_dependencies
            .iter()
            .any(|d| d == "express" || d == "Router")
        {
            if !frameworks.contains(&"Express.js") {
                frameworks.push("Express.js");
            }
        }
    }

    // React (only if significant component count, not just test fixtures)
    let component_count = summaries
        .iter()
        .filter(|s| s.symbol_kind == Some(SymbolKind::Component))
        .count();
    if component_count > 2 && !frameworks.iter().any(|f| f.contains("Next.js")) {
        frameworks.push("React");
    }

    // Return combined or None
    if frameworks.is_empty() {
        None
    } else {
        Some(frameworks.join(" + "))
    }
}

fn detect_database(summaries: &[SemanticSummary]) -> Option<String> {
    for s in summaries {
        // Drizzle detection
        if s.added_dependencies
            .iter()
            .any(|d| d == "drizzle" || d == "pgTable" || d == "mysqlTable")
        {
            return Some("PostgreSQL (Drizzle ORM)".to_string());
        }

        // Prisma detection
        if s.file.to_lowercase().contains("prisma") {
            return Some("Prisma".to_string());
        }

        // Raw postgres
        if s.added_dependencies
            .iter()
            .any(|d| d == "postgres" || d == "pg")
        {
            return Some("PostgreSQL".to_string());
        }
    }
    None
}

fn detect_package_manager(summaries: &[SemanticSummary]) -> Option<String> {
    for s in summaries {
        let file_lower = s.file.to_lowercase();

        if file_lower.ends_with("package-lock.json") {
            return Some("npm".to_string());
        }
        if file_lower.ends_with("pnpm-lock.yaml") {
            return Some("pnpm".to_string());
        }
        if file_lower.ends_with("yarn.lock") {
            return Some("yarn".to_string());
        }
        if file_lower.ends_with("cargo.toml") {
            return Some("cargo".to_string());
        }
    }
    None
}

/// Get a human-readable purpose for a module group
fn get_module_purpose(name: &str) -> String {
    match name {
        "tests" => "Test files and fixtures".to_string(),
        "docs" => "Documentation".to_string(),
        "config" => "Configuration files".to_string(),
        "api" => "API route handlers".to_string(),
        "database" => "Database schema and migrations".to_string(),
        "server" => "Server/service implementations".to_string(),
        "entry" => "Application entry points".to_string(),
        "library" => "Library roots and exports".to_string(),
        "components" => "UI components".to_string(),
        "pages" => "Page components and layouts".to_string(),
        "lib" => "Shared utilities and helpers".to_string(),
        "other" => "Other files".to_string(),
        // For dynamic module names (extracted from src/xxx)
        _ => format!("{} module", name),
    }
}

#[allow(dead_code)]
fn build_module_groups(summaries: &[SemanticSummary], dir_path: &str) -> Vec<ModuleGroup> {
    build_module_groups_with_map(summaries, dir_path, None)
}

/// Build module groups with an optional file-to-module mapping.
///
/// When `file_to_module` is provided, uses it for module names instead of `extract_module_name`.
/// This ensures consistency with conflict-aware module name stripping.
fn build_module_groups_with_map(
    summaries: &[SemanticSummary],
    _dir_path: &str,
    file_to_module: Option<&HashMap<String, String>>,
) -> Vec<ModuleGroup> {
    let mut groups: HashMap<String, Vec<&SemanticSummary>> = HashMap::new();

    for s in summaries {
        // Use provided mapping if available, otherwise fall back to extract_module_name
        let module = if let Some(mapping) = file_to_module {
            mapping
                .get(&s.file)
                .cloned()
                .unwrap_or_else(|| extract_module_name(&s.file))
        } else {
            extract_module_name(&s.file)
        };
        groups.entry(module).or_default().push(s);
    }

    groups
        .into_iter()
        .map(|(name, files)| {
            let purpose = get_module_purpose(&name);

            // Calculate aggregate risk
            let high_count = files
                .iter()
                .filter(|f| f.behavioral_risk == RiskLevel::High)
                .count();
            let med_count = files
                .iter()
                .filter(|f| f.behavioral_risk == RiskLevel::Medium)
                .count();
            let risk = if high_count > 0 {
                RiskLevel::High
            } else if med_count > 0 {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            };

            // Get key files (high risk or with symbols)
            let key_files: Vec<String> = files
                .iter()
                .filter(|f| f.behavioral_risk == RiskLevel::High || f.symbol.is_some())
                .take(3)
                .map(|f| f.file.rsplit('/').next().unwrap_or(&f.file).to_string())
                .collect();

            ModuleGroup {
                name,
                purpose,
                file_count: files.len(),
                risk,
                key_files,
            }
        })
        .collect()
}

fn identify_entry_points(summaries: &[SemanticSummary]) -> Vec<String> {
    let mut entries = Vec::new();

    for s in summaries {
        let file_lower = s.file.to_lowercase();

        // Next.js entry points
        if file_lower.ends_with("page.tsx") || file_lower.ends_with("page.jsx") {
            entries.push(s.file.clone());
        }

        // API routes
        if file_lower.contains("/api/") && file_lower.ends_with("route.ts") {
            if let Some(ref sym) = s.symbol {
                let method = sym.to_uppercase();
                if matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH") {
                    entries.push(format!("{} {}", method, s.file));
                }
            }
        }

        // Main/index files
        if file_lower.ends_with("main.rs")
            || file_lower.ends_with("index.ts")
            || file_lower.ends_with("index.js")
        {
            entries.push(s.file.clone());
        }
    }

    entries
}

fn build_data_flow(summaries: &[SemanticSummary]) -> HashMap<String, Vec<String>> {
    let mut flow = HashMap::new();

    for s in summaries {
        if !s.local_imports.is_empty() {
            flow.insert(s.file.clone(), s.local_imports.clone());
        }
    }

    flow
}

fn build_stats(summaries: &[SemanticSummary]) -> RepoStats {
    let mut stats = RepoStats::default();

    stats.total_files = summaries.len();

    for s in summaries {
        // Risk counts
        match s.behavioral_risk {
            RiskLevel::High => stats.high_risk += 1,
            RiskLevel::Medium => stats.medium_risk += 1,
            RiskLevel::Low => stats.low_risk += 1,
        }

        // Language counts
        *stats.by_language.entry(s.language.clone()).or_insert(0) += 1;

        // Component counts
        if s.symbol_kind == Some(SymbolKind::Component) {
            stats.components += 1;
        }

        // API endpoint counts
        if s.insertions.iter().any(|i| i.contains("API route")) {
            stats.api_endpoints += 1;
        }

        // Database table counts
        if s.insertions.iter().any(|i| i.contains("table definition")) {
            // Extract count from "database schema (N table definitions)"
            for insertion in &s.insertions {
                if insertion.contains("table definition") {
                    if let Some(count_str) = insertion.split('(').nth(1) {
                        if let Some(num) = count_str.split_whitespace().next() {
                            if let Ok(n) = num.parse::<usize>() {
                                stats.database_tables += n;
                            }
                        }
                    }
                }
            }
        }
    }

    stats
}

fn detect_patterns(summaries: &[SemanticSummary]) -> Vec<String> {
    let mut patterns = Vec::new();

    // Detect patterns from any language present (accumulate, don't pick winners)

    // CLI patterns (any language)
    let has_cli = summaries.iter().any(|s| {
        s.added_dependencies.iter().any(|d| {
            d.contains("clap")
                || d.contains("argparse")
                || d.contains("commander")
                || d.contains("yargs")
                || d.contains("cobra")
                || d.contains("Parser")
        })
    });
    if has_cli {
        patterns.push("CLI application".to_string());
    }

    // MCP/Protocol patterns
    let has_mcp = summaries.iter().any(|s| {
        s.added_dependencies
            .iter()
            .any(|d| d.contains("rmcp") || d.contains("mcp"))
    });
    if has_mcp {
        patterns.push("MCP server".to_string());
    }

    // Async patterns
    let has_async = summaries.iter().any(|s| {
        s.added_dependencies.iter().any(|d| {
            d.contains("tokio")
                || d.contains("async")
                || d.contains("asyncio")
                || d.contains("goroutine")
        })
    });
    if has_async {
        patterns.push("Async/concurrent".to_string());
    }

    // Serialization patterns
    let has_serialization = summaries.iter().any(|s| {
        s.added_dependencies
            .iter()
            .any(|d| d.contains("Serialize") || d.contains("Deserialize") || d.contains("serde"))
    });
    if has_serialization {
        patterns.push("Data serialization".to_string());
    }

    // AST/code analysis
    let has_ast = summaries.iter().any(|s| {
        s.file.contains("tree_sitter")
            || s.added_dependencies
                .iter()
                .any(|d| d.contains("tree_sitter") || d.contains("ast") || d.contains("parser"))
    });
    if has_ast {
        patterns.push("AST/code analysis".to_string());
    }

    // API patterns
    let has_api = summaries
        .iter()
        .any(|s| s.insertions.iter().any(|i| i.contains("API route")));
    if has_api {
        patterns.push("API endpoints".to_string());
    }

    // Database patterns
    let has_db = summaries
        .iter()
        .any(|s| s.insertions.iter().any(|i| i.contains("database")));
    if has_db {
        patterns.push("Database integration".to_string());
    }

    // React/UI component patterns (require significant count)
    let component_count = summaries
        .iter()
        .filter(|s| s.symbol_kind == Some(SymbolKind::Component))
        .count();
    if component_count > 2 {
        patterns.push("UI components".to_string());
    }

    // Docker
    if summaries
        .iter()
        .any(|s| s.file.to_lowercase().contains("docker"))
    {
        patterns.push("Dockerized".to_string());
    }

    // Git operations
    if summaries.iter().any(|s| {
        s.file.to_lowercase().contains("/git/")
            || s.added_dependencies.iter().any(|d| d.contains("git"))
    }) {
        patterns.push("Git integration".to_string());
    }

    patterns
}

// ============================================================================
// Directory TOON Encoding (with overview)
// ============================================================================

/// Encode a full directory analysis as TOON (overview + files)
///
/// Produces a single valid TOON document with:
/// - Overview fields at root level
/// - files array containing file summaries
pub fn encode_toon_directory(overview: &RepoOverview, _summaries: &[SemanticSummary]) -> String {
    // Build combined JSON structure for single TOON document
    let mut obj = Map::new();

    // Add overview fields
    obj.insert(
        "schema_version".to_string(),
        json!(crate::schema::SCHEMA_VERSION),
    );
    obj.insert("_type".to_string(), json!("repo_overview"));

    if let Some(ref fw) = overview.framework {
        obj.insert("framework".to_string(), json!(fw));
    }

    if let Some(ref db) = overview.database {
        obj.insert("database".to_string(), json!(db));
    }

    if !overview.patterns.is_empty() {
        obj.insert("patterns".to_string(), json!(overview.patterns));
    }

    // Module summary
    if !overview.modules.is_empty() {
        let modules: Vec<Value> = overview
            .modules
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "purpose": m.purpose,
                    "files": m.file_count,
                    "risk": m.risk.as_str()
                })
            })
            .collect();
        obj.insert("modules".to_string(), Value::Array(modules));
    }

    // Stats
    let stats = &overview.stats;
    obj.insert("files".to_string(), json!(stats.total_files));
    obj.insert(
        "risk_breakdown".to_string(),
        json!(format!(
            "high:{},medium:{},low:{}",
            stats.high_risk, stats.medium_risk, stats.low_risk
        )),
    );

    if stats.api_endpoints > 0 {
        obj.insert("api_endpoints".to_string(), json!(stats.api_endpoints));
    }
    if stats.database_tables > 0 {
        obj.insert("database_tables".to_string(), json!(stats.database_tables));
    }
    if stats.components > 0 {
        obj.insert("components".to_string(), json!(stats.components));
    }

    // Entry points
    if !overview.entry_points.is_empty() {
        obj.insert("entry_points".to_string(), json!(overview.entry_points));
    }

    let value = Value::Object(obj);
    encode_default(&value).unwrap_or_else(|e| format!("TOON encoding error: {}", e))
}

/// Encode repository overview as TOON
#[allow(dead_code)]
fn encode_repo_overview(overview: &RepoOverview) -> String {
    let mut obj = Map::new();

    obj.insert(
        "schema_version".to_string(),
        json!(crate::schema::SCHEMA_VERSION),
    );
    obj.insert("_type".to_string(), json!("repo_overview"));

    if let Some(ref fw) = overview.framework {
        obj.insert("framework".to_string(), json!(fw));
    }

    if let Some(ref db) = overview.database {
        obj.insert("database".to_string(), json!(db));
    }

    if !overview.patterns.is_empty() {
        obj.insert("patterns".to_string(), json!(overview.patterns));
    }

    // Module summary
    if !overview.modules.is_empty() {
        let modules: Vec<Value> = overview
            .modules
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "purpose": m.purpose,
                    "files": m.file_count,
                    "risk": m.risk.as_str()
                })
            })
            .collect();
        obj.insert("modules".to_string(), Value::Array(modules));
    }

    // Stats
    let stats = &overview.stats;
    obj.insert("files".to_string(), json!(stats.total_files));
    obj.insert(
        "risk_breakdown".to_string(),
        json!(format!(
            "high:{},medium:{},low:{}",
            stats.high_risk, stats.medium_risk, stats.low_risk
        )),
    );

    if stats.api_endpoints > 0 {
        obj.insert("api_endpoints".to_string(), json!(stats.api_endpoints));
    }
    if stats.database_tables > 0 {
        obj.insert("database_tables".to_string(), json!(stats.database_tables));
    }
    if stats.components > 0 {
        obj.insert("components".to_string(), json!(stats.components));
    }

    // Entry points
    if !overview.entry_points.is_empty() {
        obj.insert("entry_points".to_string(), json!(overview.entry_points));
    }

    let value = Value::Object(obj);
    encode_default(&value).unwrap_or_else(|e| format!("TOON encoding error: {}", e))
}

/// Encode a summary with filtered calls and no meaningless fields
pub fn encode_toon_clean(summary: &SemanticSummary) -> String {
    let mut obj = Map::new();

    // Simple scalar fields
    obj.insert("file".to_string(), json!(summary.file));
    obj.insert("language".to_string(), json!(summary.language));

    // Stable symbol ID for cross-commit tracking
    if let Some(ref id) = summary.symbol_id {
        obj.insert("symbol_id".to_string(), json!(id.hash));
        obj.insert("symbol_namespace".to_string(), json!(id.namespace));
    }

    if let Some(ref sym) = summary.symbol {
        obj.insert("symbol".to_string(), json!(sym));
    }

    if let Some(ref kind) = summary.symbol_kind {
        obj.insert("symbol_kind".to_string(), json!(kind.as_str()));
    }

    // Line range for source extraction
    if let (Some(start), Some(end)) = (summary.start_line, summary.end_line) {
        obj.insert("lines".to_string(), json!(format!("{}-{}", start, end)));
    }

    if let Some(ref ret) = summary.return_type {
        obj.insert("return_type".to_string(), json!(ret));
    }

    // Only include public_surface_changed if true (to save tokens)
    if summary.public_surface_changed {
        obj.insert("public_surface_changed".to_string(), json!(true));
    }

    // Only include behavioral_risk if not low
    if summary.behavioral_risk != RiskLevel::Low {
        obj.insert(
            "behavioral_risk".to_string(),
            json!(risk_to_string(summary.behavioral_risk)),
        );
    }

    // Cognitive complexity metrics (only if significant)
    let cc = calculate_cognitive_complexity(&summary.control_flow_changes);
    let nest = max_nesting_depth(&summary.control_flow_changes);
    if cc > 0 {
        obj.insert("cognitive_complexity".to_string(), json!(cc));
    }
    if nest > 1 {
        obj.insert("max_nesting_depth".to_string(), json!(nest));
    }

    // Insertions array
    if !summary.insertions.is_empty() {
        obj.insert("insertions".to_string(), json!(summary.insertions));
    }

    // Added dependencies (only if non-empty)
    if !summary.added_dependencies.is_empty() {
        obj.insert(
            "added_dependencies".to_string(),
            json!(summary.added_dependencies),
        );
    }

    // Local imports for data flow (only if non-empty)
    if !summary.local_imports.is_empty() {
        obj.insert("imports_from".to_string(), json!(summary.local_imports));
    }

    // State changes
    if !summary.state_changes.is_empty() {
        let state_objs: Vec<Value> = summary
            .state_changes
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "type": s.state_type,
                    "init": s.initializer
                })
            })
            .collect();
        obj.insert("state".to_string(), Value::Array(state_objs));
    }

    // Control flow (only if present)
    if !summary.control_flow_changes.is_empty() {
        let kinds: Vec<&str> = summary
            .control_flow_changes
            .iter()
            .map(|c| c.kind.as_str())
            .collect();
        obj.insert("control_flow".to_string(), json!(kinds));
    }

    // Filtered calls (meaningful only)
    let meaningful_calls = filter_meaningful_calls(&summary.calls);
    if !meaningful_calls.is_empty() {
        let call_objs = build_deduplicated_calls(&meaningful_calls);
        obj.insert("calls".to_string(), Value::Array(call_objs));
    }

    // Raw fallback only if truly needed
    if let Some(ref raw) = summary.raw_fallback {
        if summary.insertions.is_empty()
            && summary.added_dependencies.is_empty()
            && summary.calls.is_empty()
            && summary.symbol.is_none()
        {
            // Handle empty files
            if raw.trim().is_empty() {
                obj.insert("note".to_string(), json!("(empty file)"));
            } else {
                // Compact representation: single line or line count
                let lines: Vec<&str> = raw.lines().collect();
                if lines.len() <= 3 {
                    let content = raw.split_whitespace().collect::<Vec<_>>().join(" ");
                    if content.len() <= 100 {
                        obj.insert("raw".to_string(), json!(content));
                    } else {
                        obj.insert(
                            "raw".to_string(),
                            json!(format!("{}...", truncate_to_char_boundary(&content, 97))),
                        );
                    }
                } else {
                    obj.insert("raw".to_string(), json!(format!("({} lines)", lines.len())));
                }
            }
        }
    }

    let value = Value::Object(obj);
    encode_default(&value).unwrap_or_else(|e| format!("TOON encoding error: {}", e))
}

/// Encode a semantic summary as TOON
pub fn encode_toon(summary: &SemanticSummary) -> String {
    // Build a JSON value that will encode nicely to TOON
    let mut obj = Map::new();

    // Schema version for output stability
    obj.insert(
        "schema_version".to_string(),
        json!(crate::schema::SCHEMA_VERSION),
    );

    // Simple scalar fields
    obj.insert("file".to_string(), json!(summary.file));
    obj.insert("language".to_string(), json!(summary.language));

    // Stable symbol ID for cross-commit tracking
    if let Some(ref id) = summary.symbol_id {
        obj.insert("symbol_id".to_string(), json!(id.hash));
        obj.insert("symbol_namespace".to_string(), json!(id.namespace));
    }

    if let Some(ref sym) = summary.symbol {
        obj.insert("symbol".to_string(), json!(sym));
    }

    if let Some(ref kind) = summary.symbol_kind {
        obj.insert("symbol_kind".to_string(), json!(kind.as_str()));
    }

    // Line range for source extraction
    if let (Some(start), Some(end)) = (summary.start_line, summary.end_line) {
        obj.insert("lines".to_string(), json!(format!("{}-{}", start, end)));
    }

    if let Some(ref ret) = summary.return_type {
        obj.insert("return_type".to_string(), json!(ret));
    }

    obj.insert(
        "public_surface_changed".to_string(),
        json!(summary.public_surface_changed),
    );
    obj.insert(
        "behavioral_risk".to_string(),
        json!(risk_to_string(summary.behavioral_risk)),
    );

    // Cognitive complexity metrics
    let cc = calculate_cognitive_complexity(&summary.control_flow_changes);
    let nest = max_nesting_depth(&summary.control_flow_changes);
    if cc > 0 {
        obj.insert("cognitive_complexity".to_string(), json!(cc));
    }
    if nest > 0 {
        obj.insert("max_nesting_depth".to_string(), json!(nest));
    }

    // Insertions array
    if !summary.insertions.is_empty() {
        obj.insert("insertions".to_string(), json!(summary.insertions));
    }

    // Added dependencies
    if !summary.added_dependencies.is_empty() {
        obj.insert(
            "added_dependencies".to_string(),
            json!(summary.added_dependencies),
        );
    }

    // State changes - convert to uniform array of objects for tabular format
    if !summary.state_changes.is_empty() {
        let state_objs: Vec<Value> = summary
            .state_changes
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "type": s.state_type,
                    "initializer": s.initializer
                })
            })
            .collect();
        obj.insert("state_changes".to_string(), Value::Array(state_objs));
    }

    // Arguments - convert to uniform array of objects for tabular format
    if !summary.arguments.is_empty() {
        let arg_objs: Vec<Value> = summary
            .arguments
            .iter()
            .map(|a| {
                json!({
                    "name": a.name,
                    "type": a.arg_type.as_deref().unwrap_or("_"),
                    "default": a.default_value.as_deref().unwrap_or("_")
                })
            })
            .collect();
        obj.insert("arguments".to_string(), Value::Array(arg_objs));
    }

    // Props - convert to uniform array of objects for tabular format
    if !summary.props.is_empty() {
        let prop_objs: Vec<Value> = summary
            .props
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "type": p.prop_type.as_deref().unwrap_or("_"),
                    "default": p.default_value.as_deref().unwrap_or("_"),
                    "required": p.required
                })
            })
            .collect();
        obj.insert("props".to_string(), Value::Array(prop_objs));
    }

    // Control flow changes - just extract the kinds
    if !summary.control_flow_changes.is_empty() {
        let kinds: Vec<&str> = summary
            .control_flow_changes
            .iter()
            .map(|c| c.kind.as_str())
            .collect();
        obj.insert("control_flow".to_string(), json!(kinds));
    }

    // Function calls with context (deduplicated, counted)
    if !summary.calls.is_empty() {
        let call_objs = build_deduplicated_calls(&summary.calls);
        obj.insert("calls".to_string(), Value::Array(call_objs));
    }

    // Raw fallback - only include if we have no semantic data at all
    if let Some(ref raw) = summary.raw_fallback {
        if summary.added_dependencies.is_empty()
            && summary.calls.is_empty()
            && summary.state_changes.is_empty()
            && summary.control_flow_changes.is_empty()
            && summary.symbol.is_none()
            && summary.insertions.is_empty()
        {
            // For non-code files, provide a compact single-line summary
            let lines: Vec<&str> = raw.lines().collect();
            let line_count = lines.len();

            if line_count <= 3 {
                // Very short files: include as single line
                let content = raw.split_whitespace().collect::<Vec<_>>().join(" ");
                if content.len() <= 200 {
                    obj.insert("raw_source".to_string(), json!(content));
                } else {
                    obj.insert(
                        "raw_source".to_string(),
                        json!(format!("{}...", truncate_to_char_boundary(&content, 197))),
                    );
                }
            } else {
                // Longer files: just report structure
                obj.insert(
                    "raw_source".to_string(),
                    json!(format!("({} lines)", line_count)),
                );
            }
        }
    }

    // Encode to TOON using rtoon
    let value = Value::Object(obj);
    encode_default(&value).unwrap_or_else(|e| format!("TOON encoding error: {}", e))
}

/// Build deduplicated and counted call objects
fn build_deduplicated_calls(calls: &[crate::schema::Call]) -> Vec<Value> {
    // Deduplicate calls by (name, object, awaited, in_try) and count occurrences
    let mut call_counts: HashMap<(String, String, bool, bool), usize> = HashMap::new();

    for call in calls {
        let key = (
            call.name.clone(),
            call.object.clone().unwrap_or_default(),
            call.is_awaited,
            call.in_try,
        );
        *call_counts.entry(key).or_insert(0) += 1;
    }

    // Convert to sorted vec for deterministic output
    let mut unique_calls: Vec<_> = call_counts.into_iter().collect();
    unique_calls.sort_by(|a, b| b.1.cmp(&a.1).then(a.0 .0.cmp(&b.0 .0))); // Sort by count desc, then name

    unique_calls
        .into_iter()
        .map(|((name, obj, awaited, in_try), count)| {
            json!({
                "name": name,
                "obj": if obj.is_empty() { "_".to_string() } else { obj },
                "await": if awaited { "Y" } else { "_" },
                "try": if in_try { "Y" } else { "_" },
                "count": if count > 1 { count.to_string() } else { "_".to_string() }
            })
        })
        .collect()
}

/// Convert risk level to string
fn risk_to_string(risk: RiskLevel) -> &'static str {
    risk.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ControlFlowChange, ControlFlowKind, Location, StateChange, SymbolKind};

    #[test]
    fn test_basic_toon_output() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("AppLayout".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            return_type: Some("JSX.Element".to_string()),
            public_surface_changed: false,
            behavioral_risk: RiskLevel::Medium,
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("file:"));
        assert!(toon.contains("test.tsx"));
        assert!(toon.contains("language:"));
        assert!(toon.contains("tsx"));
        assert!(toon.contains("symbol:"));
        assert!(toon.contains("AppLayout"));
        assert!(toon.contains("symbol_kind:"));
        assert!(toon.contains("component"));
        assert!(toon.contains("return_type:"));
        assert!(toon.contains("JSX.Element"));
        assert!(toon.contains("public_surface_changed:"));
        assert!(toon.contains("false"));
        assert!(toon.contains("behavioral_risk:"));
        assert!(toon.contains("medium"));
    }

    #[test]
    fn test_insertions_format() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            insertions: vec![
                "header container with nav".to_string(),
                "6 route links".to_string(),
            ],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("insertions"));
        assert!(toon.contains("header container with nav"));
        assert!(toon.contains("6 route links"));
    }

    #[test]
    fn test_state_changes_tabular() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            state_changes: vec![StateChange {
                name: "open".to_string(),
                state_type: "boolean".to_string(),
                initializer: "false".to_string(),
            }],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("state_changes"));
        assert!(toon.contains("open"));
        assert!(toon.contains("boolean"));
        assert!(toon.contains("false"));
    }

    #[test]
    fn test_dependencies_inline() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            added_dependencies: vec!["useState".to_string(), "Link".to_string()],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("added_dependencies"));
        assert!(toon.contains("useState"));
        assert!(toon.contains("Link"));
    }

    #[test]
    fn test_control_flow_inline() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            control_flow_changes: vec![
                ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                    nesting_depth: 0,
                },
                ControlFlowChange {
                    kind: ControlFlowKind::For,
                    location: Location::default(),
                    nesting_depth: 0,
                },
            ],
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("control_flow"));
        assert!(toon.contains("if"));
        assert!(toon.contains("for"));
    }

    #[test]
    fn test_raw_fallback() {
        let summary = SemanticSummary {
            file: "test.tsx".to_string(),
            language: "tsx".to_string(),
            raw_fallback: Some("function foo() {}".to_string()),
            ..Default::default()
        };

        let toon = encode_toon(&summary);

        assert!(toon.contains("raw_source"));
        assert!(toon.contains("function foo() {}"));
    }

    // ========================================================================
    // is_meaningful_call() tests
    // ========================================================================

    #[test]
    fn test_meaningful_call_filters_usestate() {
        // React hooks like useState should be kept (starts with use + capital letter)
        assert!(is_meaningful_call("useState", None));
        assert!(is_meaningful_call("useEffect", None));
        assert!(is_meaningful_call("useCallback", None));
        assert!(is_meaningful_call("useMemo", None));
    }

    #[test]
    fn test_meaningful_call_keeps_custom_hooks() {
        // Custom hooks should be kept
        assert!(is_meaningful_call("useUser", None));
        assert!(is_meaningful_call("useAuth", None));
        assert!(is_meaningful_call("useFetchData", None));
    }

    #[test]
    fn test_meaningful_call_keeps_react_query_hooks() {
        // React Query / TanStack Query hooks
        assert!(is_meaningful_call("useQuery", None));
        assert!(is_meaningful_call("useMutation", None));
        assert!(is_meaningful_call("useQueryClient", None));
        assert!(is_meaningful_call("useInfiniteQuery", None));
    }

    #[test]
    fn test_meaningful_call_keeps_swr_hooks() {
        // SWR hooks
        assert!(is_meaningful_call("useSWR", None));
        assert!(is_meaningful_call("useSWRMutation", None));
        assert!(is_meaningful_call("useSWRInfinite", None));
    }

    #[test]
    fn test_meaningful_call_keeps_apollo_hooks() {
        // Apollo GraphQL hooks
        assert!(is_meaningful_call("useApolloClient", None));
    }

    #[test]
    fn test_meaningful_call_keeps_api_clients() {
        // Direct API client calls
        assert!(is_meaningful_call("fetch", None));
        assert!(is_meaningful_call("axios", None));
        assert!(is_meaningful_call("ky", None));
    }

    #[test]
    fn test_meaningful_call_keeps_http_methods_on_api_clients() {
        // HTTP methods on API clients (axios.get, ky.post, etc.)
        assert!(is_meaningful_call("get", Some("axios")));
        assert!(is_meaningful_call("post", Some("axios")));
        assert!(is_meaningful_call("put", Some("ky")));
        assert!(is_meaningful_call("delete", Some("fetch")));
    }

    #[test]
    fn test_meaningful_call_keeps_database_operations() {
        // Database and I/O operations
        assert!(is_meaningful_call("insert", None));
        assert!(is_meaningful_call("select", None));
        assert!(is_meaningful_call("update", None));
        assert!(is_meaningful_call("delete", None));
        assert!(is_meaningful_call("query", None));
        assert!(is_meaningful_call("execute", None));
    }

    #[test]
    fn test_meaningful_call_filters_promise_methods() {
        // Promise chain methods should be filtered (noise)
        assert!(!is_meaningful_call("then", None));
        assert!(!is_meaningful_call("catch", None));
        assert!(!is_meaningful_call("finally", None));
    }

    #[test]
    fn test_meaningful_call_filters_math_methods() {
        // Math methods on Math object should be filtered
        assert!(!is_meaningful_call("floor", Some("Math")));
        assert!(!is_meaningful_call("ceil", Some("Math")));
        assert!(!is_meaningful_call("round", Some("Math")));
        assert!(!is_meaningful_call("abs", Some("Math")));
    }

    #[test]
    fn test_meaningful_call_filters_object_methods() {
        // Object methods should be filtered
        assert!(!is_meaningful_call("keys", Some("Object")));
        assert!(!is_meaningful_call("values", Some("Object")));
        assert!(!is_meaningful_call("entries", Some("Object")));
    }

    #[test]
    fn test_meaningful_call_filters_array_methods() {
        // Array methods should be filtered
        assert!(!is_meaningful_call("map", Some("items")));
        assert!(!is_meaningful_call("filter", Some("data")));
        assert!(!is_meaningful_call("reduce", Some("arr")));
        assert!(!is_meaningful_call("forEach", Some("list")));
    }

    #[test]
    fn test_meaningful_call_keeps_state_setters() {
        // State setters (setX) should be kept
        assert!(is_meaningful_call("setOpen", None));
        assert!(is_meaningful_call("setLoading", None));
        assert!(is_meaningful_call("setData", None));
    }

    #[test]
    fn test_meaningful_call_keeps_special_objects() {
        // Calls on special objects like db, Response, process should be kept
        assert!(is_meaningful_call("query", Some("db")));
        assert!(is_meaningful_call("json", Some("Response")));
        assert!(is_meaningful_call("exit", Some("process")));
    }

    // ========================================================================
    // detect_framework() tests
    // ========================================================================

    #[test]
    fn test_detect_framework_rust_binary() {
        let summaries = vec![
            SemanticSummary {
                file: "Cargo.toml".to_string(),
                language: "toml".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/main.rs".to_string(),
                language: "rust".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Rust"), "Should detect Rust: {}", fw);
        assert!(fw.contains("binary"), "Should detect binary type: {}", fw);
    }

    #[test]
    fn test_detect_framework_rust_library() {
        let summaries = vec![
            SemanticSummary {
                file: "Cargo.toml".to_string(),
                language: "toml".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/lib.rs".to_string(),
                language: "rust".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Rust"), "Should detect Rust: {}", fw);
        assert!(fw.contains("library"), "Should detect library type: {}", fw);
    }

    #[test]
    fn test_detect_framework_rust_bin_and_lib() {
        let summaries = vec![
            SemanticSummary {
                file: "Cargo.toml".to_string(),
                language: "toml".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/main.rs".to_string(),
                language: "rust".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/lib.rs".to_string(),
                language: "rust".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("bin+lib"), "Should detect bin+lib: {}", fw);
    }

    #[test]
    fn test_detect_framework_nextjs_app_router() {
        let summaries = vec![
            SemanticSummary {
                file: "next.config.js".to_string(),
                language: "javascript".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "app/layout.tsx".to_string(),
                language: "tsx".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Next.js"), "Should detect Next.js: {}", fw);
        assert!(
            fw.contains("App Router"),
            "Should detect App Router: {}",
            fw
        );
    }

    #[test]
    fn test_detect_framework_nextjs_pages_router() {
        // Note: detection requires /pages/ in path (with leading slash from project root)
        let summaries = vec![
            SemanticSummary {
                file: "src/pages/index.tsx".to_string(),
                language: "tsx".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/pages/about.tsx".to_string(),
                language: "tsx".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Next.js"), "Should detect Next.js: {}", fw);
        assert!(
            fw.contains("Pages Router"),
            "Should detect Pages Router: {}",
            fw
        );
    }

    #[test]
    fn test_detect_framework_react_components() {
        let summaries = vec![
            SemanticSummary {
                file: "src/App.tsx".to_string(),
                language: "tsx".to_string(),
                symbol_kind: Some(SymbolKind::Component),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/Header.tsx".to_string(),
                language: "tsx".to_string(),
                symbol_kind: Some(SymbolKind::Component),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/Footer.tsx".to_string(),
                language: "tsx".to_string(),
                symbol_kind: Some(SymbolKind::Component),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("React"), "Should detect React: {}", fw);
    }

    #[test]
    fn test_detect_framework_express() {
        let summaries = vec![SemanticSummary {
            file: "src/server.ts".to_string(),
            language: "typescript".to_string(),
            added_dependencies: vec!["express".to_string()],
            ..Default::default()
        }];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Express"), "Should detect Express: {}", fw);
    }

    #[test]
    fn test_detect_framework_go() {
        let summaries = vec![
            SemanticSummary {
                file: "go.mod".to_string(),
                language: "go".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "main.go".to_string(),
                language: "go".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Go"), "Should detect Go: {}", fw);
    }

    #[test]
    fn test_detect_framework_python() {
        let summaries = vec![
            SemanticSummary {
                file: "pyproject.toml".to_string(),
                language: "toml".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/main.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/utils.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/app.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        assert!(fw.contains("Python"), "Should detect Python: {}", fw);
    }

    #[test]
    fn test_detect_framework_empty() {
        let summaries: Vec<SemanticSummary> = vec![];
        let framework = detect_framework(&summaries);
        assert!(framework.is_none(), "Empty summaries should return None");
    }

    #[test]
    fn test_detect_framework_multiple() {
        // Project with both Rust and Python files
        let summaries = vec![
            SemanticSummary {
                file: "Cargo.toml".to_string(),
                language: "toml".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "src/main.rs".to_string(),
                language: "rust".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "scripts/build.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "scripts/test.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
            SemanticSummary {
                file: "scripts/deploy.py".to_string(),
                language: "python".to_string(),
                ..Default::default()
            },
        ];

        let framework = detect_framework(&summaries);
        assert!(framework.is_some());
        let fw = framework.unwrap();
        // Should detect both
        assert!(
            fw.contains("Rust"),
            "Should detect Rust in multi-lang: {}",
            fw
        );
        assert!(
            fw.contains("Python"),
            "Should detect Python in multi-lang: {}",
            fw
        );
        assert!(fw.contains("+"), "Should combine frameworks: {}", fw);
    }
}
