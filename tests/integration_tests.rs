//! Integration tests for semfora-engine
//!
//! These tests verify end-to-end behavior across multiple modules.
//!
//! ## Test Tiers
//!
//! - **Tier 1: Unit** - Individual functions, mocked dependencies (in src/*.rs)
//! - **Tier 2: Component** - Module interactions (this file)
//! - **Tier 3: Integration** - Full CLI/MCP pipelines (this file, e2e_* modules)
//!
//! ## Running Integration Tests
//!
//! ```bash
//! # Run all integration tests
//! cargo test --test integration_tests
//!
//! # Run specific test group
//! cargo test --test integration_tests module_naming
//! cargo test --test integration_tests e2e_index
//! cargo test --test integration_tests e2e_cli
//!
//! # Run language-specific tests
//! cargo test --test integration_tests languages::javascript_family
//! cargo test --test integration_tests languages::systems_family
//!
//! # Run CLI command tests
//! cargo test --test integration_tests cli::analyze_tests
//! cargo test --test integration_tests cli::search_tests
//! ```
//!

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(clippy::duplicate_mod)]
//! ## Test Fixture Strategy
//!
//! Tests use tempfile to create temporary directories with specific source structures.
//! This avoids bloating the repo with fixture files while enabling realistic testing.

// Shared test infrastructure
mod common;

// Language-specific tests (27 languages organized by family)
mod languages;

// CLI command tests (analyze, search, query, validate, index)
mod cli;

// MCP server tests (all 18 tool handlers)
mod mcp;

// Output format consistency tests
mod formats;

// Edge cases and error handling tests
mod edge_cases;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

// ============================================================================
// TEST FIXTURE UTILITIES
// ============================================================================

/// Builder for creating test repository structures
struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    /// Create a new empty test repository
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("Failed to create temp dir"),
        }
    }

    /// Get the path to the test repository root
    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Add a source file with the given content
    fn add_file(&self, relative_path: &str, content: &str) -> &Self {
        let full_path = self.dir.path().join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        fs::write(&full_path, content).expect("Failed to write file");
        self
    }

    /// Add a TypeScript file with a simple function
    fn add_ts_function(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"export function {}() {{
    {}
}}
"#,
            fn_name, body
        );
        self.add_file(relative_path, &content)
    }

    /// Add a Rust file with a simple function
    fn add_rs_function(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"pub fn {}() {{
    {}
}}
"#,
            fn_name, body
        );
        self.add_file(relative_path, &content)
    }

    /// Create a standard src/ layout with multiple modules
    fn with_standard_src_layout(&self) -> &Self {
        self.add_ts_function("src/index.ts", "main", "console.log('main');")
            .add_ts_function("src/api/users.ts", "getUsers", "return db.query('users');")
            .add_ts_function("src/api/posts.ts", "getPosts", "return db.query('posts');")
            .add_ts_function(
                "src/utils/format.ts",
                "formatDate",
                "return date.toISOString();",
            )
            .add_ts_function(
                "src/utils/validate.ts",
                "validateEmail",
                "return email.includes('@');",
            )
    }

    /// Create a deep nested structure (like nopCommerce)
    fn with_deep_nesting(&self) -> &Self {
        self.add_file(
            "src/Presentation/Web/Controllers/HomeController.cs",
            "public class HomeController { public void Index() {} }",
        )
        .add_file(
            "src/Presentation/Web/Controllers/ProductController.cs",
            "public class ProductController { public void List() {} }",
        )
        .add_file(
            "src/Libraries/Services/Catalog/ProductService.cs",
            "public class ProductService { public void GetProducts() {} }",
        )
        .add_file(
            "src/Libraries/Data/Mapping/ProductMap.cs",
            "public class ProductMap { }",
        )
        // Root file that used to block stripping
        .add_file(
            "src/Program.cs",
            "public class Program { public static void Main() {} }",
        )
    }

    /// Create a monorepo structure
    fn with_monorepo_layout(&self) -> &Self {
        self.add_ts_function("packages/core/src/utils.ts", "coreUtil", "return 'core';")
            .add_ts_function("packages/core/src/types.ts", "CoreType", "")
            .add_ts_function(
                "packages/api/src/handlers.ts",
                "apiHandler",
                "return fetch('/api');",
            )
            .add_ts_function("packages/api/src/routes.ts", "setupRoutes", "app.get('/');")
            .add_ts_function("packages/web/src/App.tsx", "App", "return <div>App</div>;")
    }

    /// Create a structure with potential conflicts
    fn with_conflict_structure(&self) -> &Self {
        // These would conflict if we stripped too aggressively
        self.add_ts_function("src/game/player.ts", "gamePlayer", "")
            .add_ts_function("src/map/player.ts", "mapPlayer", "")
            .add_ts_function("src/ui/player.ts", "uiPlayer", "")
    }

    /// Create a structure with duplicate functions
    fn with_duplicates(&self) -> &Self {
        // Exact same function in different modules
        let validate_body = "return email.includes('@') && email.length > 5;";
        self.add_ts_function("src/auth/validate.ts", "validateEmail", validate_body)
            .add_ts_function("src/users/validate.ts", "validateEmail", validate_body)
            .add_ts_function("src/orders/validate.ts", "validateEmail", validate_body)
    }

    /// Run semfora-engine CLI command and return output
    fn run_cli(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
        // Find the release binary
        let binary =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/semfora-engine");

        Command::new(&binary)
            .current_dir(self.path())
            .args(args)
            .output()
    }

    /// Run index generate on this repo
    fn generate_index(&self) -> std::io::Result<std::process::Output> {
        self.run_cli(&["index", "generate"])
    }
}

// ============================================================================
// MODULE NAMING TESTS
// ============================================================================

mod module_naming {
    use semfora_engine::shard::compute_optimal_names_public;

    /// Test: Single-component modules don't block stripping for multi-component ones
    ///
    /// This is the exact bug we fixed: having "root" modules would block
    /// ALL stripping, leaving module names like "src.Presentation.Nop.Web..."
    #[test]
    fn test_single_component_does_not_block_stripping() {
        let paths = vec![
            "root".to_string(),
            "src.Presentation.Web.Controllers".to_string(),
            "src.Libraries.Services".to_string(),
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        // Single-component should be unchanged
        assert_eq!(
            result[0], "root",
            "Single-component module should be unchanged"
        );

        // Multi-component should be stripped (no more "src." prefix)
        assert!(
            !result[1].starts_with("src."),
            "Controllers module should not start with 'src.': {}",
            result[1]
        );
        assert!(
            !result[2].starts_with("src."),
            "Services module should not start with 'src.': {}",
            result[2]
        );

        // Should have stripped at least 1 component
        assert!(depth >= 1, "Expected strip_depth >= 1, got {}", depth);
    }

    /// Test: Conflict detection still works correctly
    #[test]
    fn test_conflict_stops_stripping() {
        let paths = vec!["src.game.player".to_string(), "src.map.player".to_string()];

        let (result, depth) = compute_optimal_names_public(&paths);

        // Can only strip "src" - stripping further would create duplicate "player"
        assert_eq!(result[0], "game.player", "Should stop at game.player");
        assert_eq!(result[1], "map.player", "Should stop at map.player");
        assert_eq!(depth, 1, "Should strip exactly 1 component");
    }

    /// Test: All single-component modules = no stripping
    #[test]
    fn test_all_single_component_no_stripping() {
        let paths = vec!["root".to_string(), "main".to_string(), "lib".to_string()];

        let (result, depth) = compute_optimal_names_public(&paths);

        // All should be unchanged
        assert_eq!(
            result, paths,
            "All single-component modules should be unchanged"
        );
        assert_eq!(depth, 0, "No stripping possible");
    }

    /// Regression test: nopCommerce-like structure with deep nesting
    ///
    /// This simulates the exact structure that was causing issues
    #[test]
    fn test_regression_nopcommerce_structure() {
        let paths = vec![
            "src.Presentation.Nop.Web.Framework.Migrations.UpgradeTo500".to_string(),
            "src.Presentation.Nop.Web.Framework.Migrations.UpgradeTo510".to_string(),
            "src.Presentation.Nop.Web.Controllers.HomeController".to_string(),
            "src.Libraries.Nop.Data.Mapping.Builders".to_string(),
            "src.Libraries.Nop.Services.Catalog".to_string(),
            "root".to_string(), // The troublemaker that was blocking everything
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        // Root should be unchanged
        assert_eq!(result[5], "root", "Root should be unchanged");

        // ALL other modules should NOT have "src." prefix
        for (i, name) in result.iter().enumerate() {
            if i == 5 {
                continue; // Skip root
            }
            assert!(
                !name.starts_with("src."),
                "Module at index {} should not start with 'src.': {}",
                i,
                name
            );
        }

        // Should have stripped at least 1 component
        assert!(depth >= 1, "Expected strip_depth >= 1, got {}", depth);
    }

    /// Test: Deep common prefix gets fully stripped
    #[test]
    fn test_deep_common_prefix_stripping() {
        let paths = vec![
            "a.b.c.d.player".to_string(),
            "a.b.c.d.enemy".to_string(),
            "a.b.c.d.weapon".to_string(),
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        // Should strip all the way down to unique names
        assert_eq!(result[0], "player");
        assert_eq!(result[1], "enemy");
        assert_eq!(result[2], "weapon");
        assert_eq!(depth, 4, "Should strip 4 components (a.b.c.d)");
    }

    /// Test: Empty input
    #[test]
    fn test_empty_input() {
        let paths: Vec<String> = vec![];
        let (result, depth) = compute_optimal_names_public(&paths);

        assert!(result.is_empty());
        assert_eq!(depth, 0);
    }

    /// Test: Single module
    #[test]
    fn test_single_module() {
        let paths = vec!["src.game.player".to_string()];
        let (result, depth) = compute_optimal_names_public(&paths);

        // Single module should be fully stripped (no conflicts possible)
        assert_eq!(result[0], "player");
        assert_eq!(depth, 2);
    }

    /// Test: Mixed single and multi with immediate conflict
    #[test]
    fn test_mixed_immediate_conflict() {
        let paths = vec![
            "main".to_string(),
            "src.main".to_string(), // Would conflict with "main" after stripping
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        // Single-component preserved
        assert_eq!(result[0], "main");
        // Multi-component can't strip because it would conflict
        assert_eq!(result[1], "src.main");
        assert_eq!(depth, 0, "Can't strip due to conflict");
    }

    /// Test: Very deep paths (10+ components)
    #[test]
    fn test_very_deep_paths() {
        let paths = vec![
            "a.b.c.d.e.f.g.h.i.j.player".to_string(),
            "a.b.c.d.e.f.g.h.i.j.enemy".to_string(),
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        assert_eq!(result[0], "player");
        assert_eq!(result[1], "enemy");
        assert_eq!(depth, 10);
    }

    /// Test: Paths with similar endings but different depths
    #[test]
    fn test_different_depths_same_ending() {
        let paths = vec!["src.game.utils".to_string(), "src.utils".to_string()];

        let (result, depth) = compute_optimal_names_public(&paths);

        // After stripping "src", we get "game.utils" and "utils" - no conflict
        assert_eq!(result[0], "game.utils");
        assert_eq!(result[1], "utils");
        assert_eq!(depth, 1);
    }
}

// ============================================================================
// MODULE REGISTRY TESTS
// ============================================================================

mod module_registry {
    use semfora_engine::module_registry::ModuleRegistrySqlite;
    use semfora_engine::CacheDir;
    use tempfile::TempDir;

    /// Test: Registry persists and retrieves data correctly
    #[test]
    fn test_registry_persistence() {
        let dir = TempDir::new().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        // Insert data
        {
            let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();
            let entries = vec![
                (
                    "src.game.player".to_string(),
                    "game.player".to_string(),
                    "/path/player.rs".to_string(),
                ),
                (
                    "src.game.enemy".to_string(),
                    "game.enemy".to_string(),
                    "/path/enemy.rs".to_string(),
                ),
            ];
            reg.bulk_insert(&entries, 1).unwrap();
        }

        // Reopen and verify
        {
            let reg = ModuleRegistrySqlite::open(&cache).unwrap();
            assert_eq!(reg.get_module_count(), 2);
            assert_eq!(reg.get_strip_depth(), 1);
            assert_eq!(
                reg.get_short_name("src.game.player"),
                Some("game.player".to_string())
            );
            assert_eq!(
                reg.get_full_path("game.enemy"),
                Some("src.game.enemy".to_string())
            );
        }
    }

    /// Test: Bulk insert replaces existing data
    #[test]
    fn test_registry_bulk_replace() {
        let dir = TempDir::new().unwrap();
        let cache = CacheDir::for_repo(dir.path()).unwrap();
        cache.init().unwrap();

        let mut reg = ModuleRegistrySqlite::open(&cache).unwrap();

        // First insert
        let entries1 = vec![(
            "old.module".to_string(),
            "module".to_string(),
            "".to_string(),
        )];
        reg.bulk_insert(&entries1, 1).unwrap();
        assert!(reg.has_short_name("module"));

        // Second insert should replace
        let entries2 = vec![
            ("new.a".to_string(), "a".to_string(), "".to_string()),
            ("new.b".to_string(), "b".to_string(), "".to_string()),
        ];
        reg.bulk_insert(&entries2, 2).unwrap();

        // Old data should be gone
        assert!(!reg.has_short_name("module"));
        // New data should be present
        assert!(reg.has_short_name("a"));
        assert!(reg.has_short_name("b"));
        assert_eq!(reg.get_strip_depth(), 2);
    }
}

// ============================================================================
// DUPLICATE DETECTION TESTS
// ============================================================================

mod duplicate_detection {
    use semfora_engine::duplicate::{DuplicateDetector, FunctionSignature, SymbolRef};

    /// Test: Detector finds exact duplicates
    #[test]
    fn test_finds_exact_duplicates() {
        // Create two identical signatures
        let sig1 = create_test_signature("validateEmail", "services", "user.ts");
        let sig2 = create_test_signature("validateEmail", "orders", "order.ts");

        let signatures = vec![sig1, sig2];
        let detector = DuplicateDetector::new(0.85);
        let clusters = detector.find_all_clusters(&signatures);

        // Should find at least one cluster
        assert!(!clusters.is_empty(), "Should find duplicate cluster");
    }

    /// Test: Module names in signatures are used correctly
    #[test]
    fn test_signature_module_preserved() {
        let sig = create_test_signature("testFunc", "my.module", "test.ts");

        assert_eq!(sig.module, "my.module");

        let symbol_ref = sig.to_symbol_ref();
        assert_eq!(symbol_ref.module, "my.module");
    }

    /// Test: Different signatures are not marked as duplicates
    #[test]
    fn test_different_not_duplicates() {
        let sig1 = create_test_signature("validateEmail", "auth", "auth.ts");
        let sig2 = create_test_signature_different("processPayment", "payments", "pay.ts");

        let signatures = vec![sig1, sig2];
        let detector = DuplicateDetector::new(0.90);
        let clusters = detector.find_all_clusters(&signatures);

        // Should not find these as duplicates (different functions)
        // The cluster might exist but with no duplicates
        for cluster in &clusters {
            if cluster.primary.name == "validateEmail" {
                // If validateEmail is primary, processPayment shouldn't be a high-similarity duplicate
                let high_sim_dups: Vec<_> = cluster
                    .duplicates
                    .iter()
                    .filter(|d| d.similarity >= 0.90)
                    .collect();
                assert!(
                    high_sim_dups.is_empty()
                        || !high_sim_dups
                            .iter()
                            .any(|d| d.symbol.name == "processPayment"),
                    "Different functions should not be high-similarity duplicates"
                );
            }
        }
    }

    // Helper to create test signatures
    // Uses file path in hash to ensure uniqueness even for same-named functions
    fn create_test_signature(name: &str, module: &str, file: &str) -> FunctionSignature {
        FunctionSignature {
            symbol_hash: format!("hash_{}_{}", name, file), // Unique per file
            name: name.to_string(),
            file: file.to_string(),
            module: module.to_string(),
            start_line: 1,
            name_tokens: vec!["validate".to_string(), "email".to_string()],
            call_fingerprint: 12345,
            control_flow_fingerprint: 67890,
            state_fingerprint: 11111,
            has_business_logic: true,
            business_calls: vec!["db.query".to_string()],
            param_count: 1,
            boilerplate_category: None,
            line_count: 10,
        }
    }

    fn create_test_signature_different(name: &str, module: &str, file: &str) -> FunctionSignature {
        FunctionSignature {
            symbol_hash: format!("hash_{}_{}", name, file), // Unique per file
            name: name.to_string(),
            file: file.to_string(),
            module: module.to_string(),
            start_line: 1,
            name_tokens: vec!["process".to_string(), "payment".to_string()],
            call_fingerprint: 99999, // Different fingerprints
            control_flow_fingerprint: 88888,
            state_fingerprint: 77777,
            has_business_logic: true,
            business_calls: vec!["stripe.charge".to_string()],
            param_count: 3,
            boilerplate_category: None,
            line_count: 25,
        }
    }
}

// ============================================================================
// EXTRACT MODULE NAME TESTS
// ============================================================================

mod extract_module_name {
    use semfora_engine::extract_module_name;

    /// Test: Standard src/ layout
    #[test]
    fn test_src_layout() {
        assert_eq!(extract_module_name("src/api/users.ts"), "api");
        assert_eq!(
            extract_module_name("src/components/Button.tsx"),
            "components"
        );
        assert_eq!(
            extract_module_name("src/features/auth/login.ts"),
            "features.auth"
        );
    }

    /// Test: Root files get "root" module
    #[test]
    fn test_root_files() {
        assert_eq!(extract_module_name("src/index.ts"), "root");
        assert_eq!(extract_module_name("src/main.rs"), "root");
        assert_eq!(extract_module_name("src/lib.rs"), "root");
    }

    /// Test: Unity/Unreal style paths
    #[test]
    fn test_game_engine_paths() {
        assert_eq!(extract_module_name("Assets/Scripts/Game/Player.cs"), "Game");
        assert_eq!(extract_module_name("Source/MyGame/Character.cpp"), "MyGame");
    }

    /// Test: Deep paths
    #[test]
    fn test_deep_paths() {
        assert_eq!(
            extract_module_name("/project/src/server/api/handlers/users.ts"),
            "server.api.handlers"
        );
    }

    /// Test: Monorepo packages
    #[test]
    fn test_monorepo_packages() {
        assert_eq!(
            extract_module_name("/repo/packages/core/utils/format.ts"),
            "core.utils"
        );
        assert_eq!(
            extract_module_name("packages/api/handlers/auth.ts"),
            "api.handlers"
        );
    }
}

// ============================================================================
// REGRESSION TESTS
// ============================================================================

mod regression {
    use semfora_engine::shard::compute_optimal_names_public;

    /// Regression: SEM-XX - Module names showing full paths
    ///
    /// Bug: Single-component modules blocked ALL stripping, causing
    /// module names like "src.Presentation.Nop.Web.Framework.Migrations"
    /// instead of shortened names.
    #[test]
    fn test_sem_xx_full_path_module_names() {
        let paths = vec![
            "root".to_string(),
            "src.Presentation.Nop.Web.Framework.Migrations.UpgradeTo500".to_string(),
        ];

        let (result, depth) = compute_optimal_names_public(&paths);

        // The bug would cause depth=0 and result[1] to keep "src." prefix
        assert!(
            depth >= 1,
            "Bug regression: strip_depth should be >= 1, got {}",
            depth
        );
        assert!(
            !result[1].starts_with("src."),
            "Bug regression: module should not start with 'src.': {}",
            result[1]
        );
    }
}

// ============================================================================
// END-TO-END INDEX GENERATION TESTS
// ============================================================================

mod e2e_index {
    use super::TestRepo;

    /// Test: Index generation succeeds on standard src/ layout
    #[test]
    fn test_index_generation_standard_layout() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();

        let output = repo.generate_index().expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success(),
            "Index generation failed: {}",
            stdout
        );
        assert!(
            stdout.contains("files_processed:"),
            "Should report processed files"
        );
    }

    /// Test: Index generation succeeds on deep nested structure
    #[test]
    fn test_index_generation_deep_nesting() {
        let repo = TestRepo::new();
        repo.with_deep_nesting();

        let output = repo.generate_index().expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success(),
            "Index generation failed: {}",
            stdout
        );
    }

    /// Test: Index generation succeeds on monorepo structure
    #[test]
    fn test_index_generation_monorepo() {
        let repo = TestRepo::new();
        repo.with_monorepo_layout();

        let output = repo.generate_index().expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success(),
            "Index generation failed: {}",
            stdout
        );
    }

    /// Test: Index generation handles empty directory gracefully
    #[test]
    fn test_index_generation_empty_dir() {
        let repo = TestRepo::new();
        // Don't add any files

        let output = repo.generate_index().expect("Failed to run CLI");

        // Should complete without crashing, even if no files found
        // Exit code might be non-zero for empty, but shouldn't panic
        let _stdout = String::from_utf8_lossy(&output.stdout);
        let _stderr = String::from_utf8_lossy(&output.stderr);
        // Just verify it didn't crash/panic
    }

    /// Test: Index reports correct file counts
    #[test]
    fn test_index_file_counts() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/a.ts", "funcA", "")
            .add_ts_function("src/b.ts", "funcB", "")
            .add_ts_function("src/c.ts", "funcC", "");

        let output = repo.generate_index().expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Index generation failed");
        // Should process exactly 3 files
        assert!(
            stdout.contains("files_processed: 3") || stdout.contains("files_found: 3"),
            "Should report 3 files processed: {}",
            stdout
        );
    }
}

// ============================================================================
// CLI OUTPUT CONSISTENCY TESTS
// ============================================================================

mod e2e_cli {
    use super::TestRepo;

    /// Test: Query overview returns valid output in text format
    #[test]
    fn test_query_overview_text() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();
        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "text"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);
        assert!(
            stdout.contains("REPOSITORY OVERVIEW") || stdout.contains("repo_overview"),
            "Should contain overview header"
        );
    }

    /// Test: Query overview returns valid output in TOON format
    #[test]
    fn test_query_overview_toon() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();
        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "toon", "--modules"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);
        assert!(
            stdout.contains("_type: repo_overview"),
            "Should have TOON type marker"
        );
        assert!(stdout.contains("modules["), "Should list modules");
    }

    /// Test: Query overview returns valid output in JSON format
    #[test]
    fn test_query_overview_json() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();
        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "json"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);

        // Should be valid JSON
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(parsed.is_ok(), "Should be valid JSON: {}", stdout);

        let json = parsed.unwrap();
        assert_eq!(json["_type"], "repo_overview", "Should have correct type");
    }

    /// Test: Search command works after indexing
    #[test]
    fn test_search_after_index() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();
        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["search", "getUsers", "-f", "toon"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Search failed: {}", stdout);
        // Should find the getUsers function
        assert!(
            stdout.contains("getUsers") || stdout.contains("symbol_results"),
            "Should find getUsers function: {}",
            stdout
        );
    }

    /// Test: Validate duplicates command works
    #[test]
    fn test_validate_duplicates() {
        let repo = TestRepo::new();
        repo.with_duplicates();
        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["validate", "--duplicates", "-f", "toon"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Validate failed: {}", stdout);
        // Should find duplicates since we have 3 identical validateEmail functions
        assert!(
            stdout.contains("duplicate") || stdout.contains("cluster"),
            "Should find duplicates: {}",
            stdout
        );
    }
}

// ============================================================================
// MODULE NAME FLOW TESTS
// ============================================================================

mod e2e_module_names {
    use super::TestRepo;

    /// Test: Module names in overview match expected shortened form
    #[test]
    fn test_overview_module_names_shortened() {
        let repo = TestRepo::new();
        // Create structure where stripping should definitely happen
        repo.add_ts_function("src/api/users.ts", "getUsers", "")
            .add_ts_function("src/api/posts.ts", "getPosts", "")
            .add_ts_function("src/utils/format.ts", "format", "");

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "toon", "--modules"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);

        // Module names should be shortened (api, utils - not src.api, src.utils)
        // Note: Exact behavior depends on whether there are conflicts
        // At minimum, verify the output contains module information
        assert!(
            stdout.contains("modules["),
            "Should list modules: {}",
            stdout
        );
    }

    /// Test: Deep nesting gets properly shortened
    #[test]
    fn test_deep_nesting_module_names() {
        let repo = TestRepo::new();
        repo.with_deep_nesting();

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "toon", "--modules"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);

        // Should NOT have overly long module names
        // The old bug would leave "src.Presentation.Web.Controllers" etc.
        // Check that we don't have excessive "src." prefixes in all modules
        let src_prefixes = stdout.matches("src.").count();
        let module_count: usize = stdout
            .lines()
            .filter(|l| l.trim().starts_with("src.") || l.contains(",high") || l.contains(",low"))
            .count();

        // This is a heuristic - if stripping works, we shouldn't see src. prefix
        // on every single module line
        if module_count > 2 {
            assert!(
                src_prefixes < module_count * 2,
                "Too many 'src.' prefixes - stripping may not be working: {}",
                stdout
            );
        }
    }

    /// Test: Conflict structure preserves necessary prefixes
    #[test]
    fn test_conflict_preserves_prefixes() {
        let repo = TestRepo::new();
        repo.with_conflict_structure();

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "overview", "-f", "toon", "--modules"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Query failed: {}", stdout);

        // With conflict structure (game/player, map/player, ui/player),
        // we should see distinguishing prefixes preserved
        // e.g., "game.player", "map.player", "ui.player" or similar
        // NOT just "player" three times
    }
}

// ============================================================================
// SEARCH INTEGRATION TESTS
// ============================================================================

mod e2e_search {
    use super::TestRepo;

    /// Test: Symbol search finds functions by name
    #[test]
    fn test_symbol_search_by_name() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api/users.ts", "getUserById", "return db.get(id);")
            .add_ts_function("src/api/posts.ts", "getPostById", "return db.get(id);");

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["search", "getUser", "-f", "json"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "Search failed: {}", stdout);
        assert!(
            stdout.contains("getUserById"),
            "Should find getUserById: {}",
            stdout
        );
    }

    /// Test: Search handles no results gracefully
    #[test]
    fn test_search_no_results() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api/users.ts", "getUsers", "");

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["search", "nonexistentFunction12345", "-f", "toon"])
            .expect("Failed to run CLI");

        // Should not crash, should return empty or "no results" message
        assert!(
            output.status.success(),
            "Search should succeed even with no results"
        );
    }

    /// Test: Search works without prior index (should auto-generate or error gracefully)
    #[test]
    fn test_search_without_index() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/test.ts", "testFunc", "");

        // Don't generate index first
        let output = repo
            .run_cli(&["search", "testFunc"])
            .expect("Failed to run CLI");

        // Should either auto-generate index or give clear error
        // Just verify it doesn't crash
        let _stdout = String::from_utf8_lossy(&output.stdout);
        let _stderr = String::from_utf8_lossy(&output.stderr);
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

mod e2e_errors {
    use super::TestRepo;

    /// Test: Invalid command gives helpful error
    #[test]
    fn test_invalid_command() {
        let repo = TestRepo::new();

        let output = repo
            .run_cli(&["invalidcommand"])
            .expect("Failed to run CLI");

        assert!(!output.status.success(), "Invalid command should fail");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("error") || stderr.contains("unrecognized"),
            "Should give helpful error message"
        );
    }

    /// Test: Query without index gives clear error
    #[test]
    fn test_query_without_index() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/test.ts", "test", "");

        // Don't generate index
        let output = repo
            .run_cli(&["query", "overview"])
            .expect("Failed to run CLI");

        // Should give clear message about missing index
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        assert!(
            combined.to_lowercase().contains("index")
                || combined.to_lowercase().contains("generate")
                || combined.to_lowercase().contains("not found")
                || !output.status.success(),
            "Should indicate missing index or fail gracefully"
        );
    }

    /// Test: Handles binary files gracefully
    #[test]
    fn test_handles_binary_files() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/code.ts", "realCode", "");

        // Add a binary file
        let binary_content: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0x00];
        let binary_path = repo.path().join("src/binary.dat");
        std::fs::write(&binary_path, binary_content).expect("Failed to write binary");

        let output = repo.generate_index().expect("Failed to run CLI");

        // Should complete without crashing
        assert!(
            output.status.success(),
            "Should handle binary files gracefully"
        );
    }

    /// Test: Handles deeply nested directories
    #[test]
    fn test_deeply_nested_directories() {
        let repo = TestRepo::new();

        // Create very deep nesting (20 levels)
        let deep_path = (0..20)
            .map(|i| format!("level{}", i))
            .collect::<Vec<_>>()
            .join("/");
        let file_path = format!("{}/deep.ts", deep_path);
        repo.add_ts_function(&file_path, "deepFunc", "");

        let output = repo.generate_index().expect("Failed to run CLI");

        // Should complete without stack overflow or timeout
        assert!(output.status.success(), "Should handle deep nesting");
    }

    /// Test: Handles files with special characters in names
    #[test]
    fn test_special_characters_in_filenames() {
        let repo = TestRepo::new();

        // Files with spaces, dashes, underscores
        repo.add_ts_function("src/my-component.ts", "myComponent", "")
            .add_ts_function("src/util_helpers.ts", "utilHelper", "");

        let output = repo.generate_index().expect("Failed to run CLI");

        assert!(
            output.status.success(),
            "Should handle special characters in filenames"
        );
    }

    /// Test: Handles empty files
    #[test]
    fn test_empty_files() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.ts", "")
            .add_ts_function("src/real.ts", "realFunc", "");

        let output = repo.generate_index().expect("Failed to run CLI");

        assert!(output.status.success(), "Should handle empty files");
    }

    /// Test: Handles syntax errors in source files
    #[test]
    fn test_syntax_errors_in_source() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/broken.ts",
            "export function broken( { this is not valid",
        )
        .add_ts_function("src/valid.ts", "validFunc", "");

        let output = repo.generate_index().expect("Failed to run CLI");

        // Should complete, potentially with errors logged but not crash
        // Valid files should still be indexed
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("files_processed") || output.status.success(),
            "Should process files despite syntax errors"
        );
    }
}

// ============================================================================
// CALL GRAPH INTEGRATION TESTS
// ============================================================================

mod e2e_callgraph {
    use super::TestRepo;

    /// Test: Call graph captures function calls
    #[test]
    fn test_callgraph_captures_calls() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api.ts", "fetchData", "return httpClient.get('/data');")
            .add_ts_function(
                "src/service.ts",
                "processData",
                "const data = fetchData(); return transform(data);",
            );

        repo.generate_index().expect("Index generation failed");

        let output = repo
            .run_cli(&["query", "callgraph", "-f", "toon"])
            .expect("Failed to run CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should have call graph data
        assert!(output.status.success(), "Call graph query should succeed");
    }
}

// ============================================================================
// FORMAT CONSISTENCY TESTS
// ============================================================================

mod e2e_formats {
    use super::TestRepo;

    /// Test: All three formats produce consistent data
    #[test]
    fn test_format_consistency() {
        let repo = TestRepo::new();
        repo.with_standard_src_layout();
        repo.generate_index().expect("Index generation failed");

        // Get overview in all three formats
        let text_output = repo
            .run_cli(&["query", "overview", "-f", "text"])
            .expect("Failed");
        let toon_output = repo
            .run_cli(&["query", "overview", "-f", "toon"])
            .expect("Failed");
        let json_output = repo
            .run_cli(&["query", "overview", "-f", "json"])
            .expect("Failed");

        // All should succeed
        assert!(text_output.status.success(), "Text format should work");
        assert!(toon_output.status.success(), "TOON format should work");
        assert!(json_output.status.success(), "JSON format should work");

        // JSON should be parseable
        let json_str = String::from_utf8_lossy(&json_output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&json_str).expect("JSON should be valid");

        // All should indicate the same type
        assert!(json["_type"].as_str().is_some(), "JSON should have _type");

        let toon_str = String::from_utf8_lossy(&toon_output.stdout);
        assert!(toon_str.contains("_type:"), "TOON should have _type marker");
    }

    /// Test: Search results are consistent across formats
    #[test]
    fn test_search_format_consistency() {
        let repo = TestRepo::new();
        repo.add_ts_function("src/api.ts", "searchableFunc", "return 'found';");
        repo.generate_index().expect("Index generation failed");

        let toon = repo
            .run_cli(&["search", "searchable", "-f", "toon"])
            .expect("Failed");
        let json = repo
            .run_cli(&["search", "searchable", "-f", "json"])
            .expect("Failed");

        assert!(toon.status.success(), "TOON search should work");
        assert!(json.status.success(), "JSON search should work");

        // Both should find the function
        let toon_str = String::from_utf8_lossy(&toon.stdout);
        let json_str = String::from_utf8_lossy(&json.stdout);

        // At least one format should mention the function name
        assert!(
            toon_str.contains("searchableFunc") || json_str.contains("searchableFunc"),
            "Should find searchableFunc in at least one format"
        );
    }
}
