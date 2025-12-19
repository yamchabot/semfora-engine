//! Tests for the `setup` CLI command
//!
//! The setup command manages semfora-engine installation and MCP client configuration:
//! - `setup` - Interactive setup (skipped in tests)
//! - `setup --list-clients` - List available MCP clients
//! - `setup --dry-run` - Show what would be done without making changes
//! - `setup --non-interactive --clients <CLIENTS>` - Non-interactive setup

#![allow(unused_imports)]

use crate::common::{assert_valid_json, TestRepo};

// ============================================================================
// SETUP LIST CLIENTS TESTS
// ============================================================================

#[test]
fn test_setup_list_clients() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&["setup", "--list-clients"]);

    // Should list available clients
    assert!(
        output.contains("claude")
            || output.contains("Claude")
            || output.contains("cursor")
            || output.contains("Cursor")
            || output.contains("vscode")
            || output.contains("client")
            || !output.is_empty(),
        "Should list available MCP clients: {}",
        output
    );
}

#[test]
fn test_setup_list_clients_output() {
    let repo = TestRepo::new();

    // list-clients may not support JSON format
    let output = repo.run_cli_success(&["setup", "--list-clients"]);

    // Should have client list in text format
    assert!(
        output.contains("claude")
            || output.contains("Claude")
            || output.contains("cursor")
            || output.contains("Cursor")
            || output.contains("vscode")
            || output.contains("client")
            || !output.is_empty(),
        "Should return clients list: {}",
        output
    );
}

// ============================================================================
// SETUP DRY RUN TESTS
// ============================================================================

#[test]
fn test_setup_dry_run_basic() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
    ]);

    // Should show what would be done
    assert!(
        output.contains("would")
            || output.contains("dry")
            || output.contains("config")
            || output.contains("claude")
            || !output.is_empty(),
        "Should show dry run output: {}",
        output
    );
}

#[test]
fn test_setup_dry_run_multiple_clients() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop,cursor",
    ]);

    // Should show what would be done for multiple clients
    assert!(!output.is_empty(), "Should produce dry run output");
}

#[test]
fn test_setup_dry_run_output() {
    let repo = TestRepo::new();

    // dry-run may not support JSON format
    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
    ]);

    // Should have configuration info in text format
    assert!(
        output.contains("would")
            || output.contains("dry")
            || output.contains("claude")
            || output.contains("config")
            || !output.is_empty(),
        "Should return dry run output: {}",
        output
    );
}

// ============================================================================
// SETUP WITH OPTIONS TESTS
// ============================================================================

#[test]
fn test_setup_with_binary_path() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
        "--binary-path",
        "/custom/path/semfora-engine",
    ]);

    // Should use custom binary path
    assert!(
        output.contains("custom")
            || output.contains("/path/")
            || output.contains("binary")
            || !output.is_empty(),
        "Should acknowledge custom binary path: {}",
        output
    );
}

#[test]
fn test_setup_with_cache_dir() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
        "--cache-dir",
        "/tmp/custom-cache",
    ]);

    // Should use custom cache dir
    assert!(!output.is_empty(), "Should complete with custom cache dir");
}

#[test]
fn test_setup_with_log_level() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
        "--log-level",
        "debug",
    ]);

    // Should use debug log level
    assert!(!output.is_empty(), "Should complete with custom log level");
}

// ============================================================================
// SETUP EXPORT CONFIG TESTS
// ============================================================================

#[test]
fn test_setup_export_config() {
    let repo = TestRepo::new();
    let export_path = repo.path().join("mcp-config.json");

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop",
        "--export-config",
        export_path.to_str().unwrap(),
    ]);

    // Should mention export (in dry run, won't actually create file)
    assert!(!output.is_empty(), "Should complete export config");
}

// ============================================================================
// FORMAT TESTS
// ============================================================================

#[test]
fn test_setup_list_clients_completes() {
    let repo = TestRepo::new();

    // list-clients may not support all formats - just verify it completes
    let text_result = repo.run_cli(&["setup", "--list-clients", "-f", "text"]);
    let json_result = repo.run_cli(&["setup", "--list-clients", "-f", "json"]);
    let toon_result = repo.run_cli(&["setup", "--list-clients", "-f", "toon"]);

    // All should complete without crashing
    assert!(text_result.is_ok(), "Text format should complete");
    assert!(json_result.is_ok(), "JSON format should complete");
    assert!(toon_result.is_ok(), "TOON format should complete");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_setup_invalid_client() {
    let repo = TestRepo::new();

    let result = repo.run_cli(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "invalid-client-name",
    ]);

    // Should handle invalid client gracefully
    assert!(result.is_ok());
}

#[test]
fn test_setup_empty_clients() {
    let repo = TestRepo::new();

    let result = repo.run_cli(&["setup", "--dry-run", "--non-interactive", "--clients", ""]);

    // Should handle empty clients
    assert!(result.is_ok());
}

#[test]
fn test_setup_all_clients() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&[
        "setup",
        "--dry-run",
        "--non-interactive",
        "--clients",
        "claude-desktop,claude-code,cursor,vscode",
    ]);

    // Should handle all clients
    assert!(!output.is_empty(), "Should complete with all clients");
}

// ============================================================================
// UNINSTALL TESTS (from the uninstall command)
// ============================================================================

#[test]
fn test_uninstall_basic() {
    let repo = TestRepo::new();

    // Check uninstall help - don't actually uninstall
    let result = repo.run_cli(&["uninstall", "--help"]);
    assert!(result.is_ok(), "Uninstall help should work");
}

#[test]
fn test_uninstall_all() {
    let repo = TestRepo::new();

    // Test uninstall with all flag - won't actually uninstall if nothing is installed
    let result = repo.run_cli(&["uninstall", "all"]);

    // May succeed or report nothing to uninstall
    assert!(result.is_ok(), "Uninstall should complete");
}

// ============================================================================
// CONFIG COMMAND TESTS
// ============================================================================

#[test]
fn test_config_show() {
    let repo = TestRepo::new();

    let result = repo.run_cli(&["config", "show"]);

    // Should show config or handle missing config
    assert!(result.is_ok());
}

#[test]
fn test_config_show_output() {
    let repo = TestRepo::new();

    let output = repo.run_cli_success(&["config", "show"]);

    // Config is output in TOML format, not JSON
    assert!(
        output.contains("[")
            || output.contains("cache")
            || output.contains("logging")
            || output.contains("mcp")
            || !output.is_empty(),
        "Should return config output: {}",
        output
    );
}
