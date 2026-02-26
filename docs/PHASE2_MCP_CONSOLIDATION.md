# Phase 2: MCP Tool Consolidation

## Goal
Make MCP tools call into the CLI command handlers (in `src/commands/`) instead of having duplicate implementations. This will:
1. Reduce code duplication
2. Ensure CLI and MCP behave identically
3. Reduce `src/mcp_server/mod.rs` from 2,859 lines to ~1,800 lines

## Current State
- CLI restructure complete on `man_rewrite` branch (commit `712c38b`)
- CLI handlers are in `src/commands/*.rs`
- MCP server is in `src/mcp_server/mod.rs` (2,859 lines, 35+ tools)

## Approach
MCP tools should:
1. Parse MCP request into CLI args struct
2. Call the CLI handler function from `src/commands/`
3. Format the result for MCP response

## Key Consolidations (35 tools → ~18)

### Search Tools → 1 unified `search` tool
Current MCP tools:
- `search_symbols` (line 961)
- `semantic_search` (line 2662)
- `raw_search` (line 1050)
- `search_and_get_symbols` (line 2138)

CLI handler: `src/commands/search.rs` - `run_search()`
- Already implements unified hybrid search
- Mode parameter: `symbols`, `semantic`, `raw`, or hybrid (default)

### Validation Tools → 1 unified `validate` tool
Current MCP tools:
- `validate_symbol` (line 2482)
- `validate_file_symbols` (line 2540)
- `validate_module_symbols` (line 2599)

CLI handler: `src/commands/validate.rs` - `run_validate()`
- Auto-detects scope (symbol hash, file, module, or directory)

### Index Tools → 1 unified `index` tool
Current MCP tools:
- `generate_index` (line 614)
- `check_index` (line 1223)

CLI handler: `src/commands/index.rs` - `run_index()`

### Test Tools → 1 unified `test` tool
Current MCP tools:
- `run_tests` (line 1310)
- `detect_tests` (line 1367)

CLI handler: `src/commands/test.rs` - `run_test()`

### Security Tools → 1 unified `security` tool
Current MCP tools:
- `cve_scan` (line 1642)
- `update_security_patterns` (line 1756)
- `get_security_pattern_stats` (line 1801)

CLI handler: `src/commands/security.rs` - `run_security()`

### Server Status Tools → 1 unified `server_status` tool
Current MCP tools:
- `check_server_mode` (line 1455)
- `get_layer_status` (line 1424)

### Duplicate Tools → 1 unified `find_duplicates` tool
Current MCP tools:
- `find_duplicates` (line 1492)
- `check_duplicates` (line 1594)

Already part of validate in CLI.

### Analysis Tools → 1 unified `analyze` tool
Current MCP tools:
- `analyze_file` (line 271)
- `analyze_directory` (line 313)
- `get_module` (line 548)

CLI handler: `src/commands/analyze.rs` - `run_analyze()`

### Query Tools (keep separate but share code)
These stay mostly as-is but should call into query.rs:
- `get_context` - keep
- `get_repo_overview` + `list_modules` → `get_overview`
- `get_symbol` + `get_symbols` - keep (batch support)
- `get_symbol_source` → `get_source`
- `get_file_symbols` + `list_symbols` → `get_file`
- `get_callers` - keep
- `get_call_graph` + `export_call_graph_sqlite` → `get_callgraph`

## Implementation Steps

1. **Create shared types** in `src/commands/mod.rs`:
   - `CommandContext` already exists
   - Add `CommandResult` enum for structured output

2. **Modify CLI handlers** to return structured data:
   - Currently return `Result<String>`
   - Could return `Result<CommandOutput>` that can be formatted as TOON/JSON

3. **Update MCP handlers** to call CLI handlers:
   ```rust
   // Example: search tool
   async fn search(&self, params: SearchRequest) -> Result<CallToolResult, McpError> {
       let args = SearchArgs::from(params);
       let ctx = CommandContext { format: OutputFormat::Toon, verbose: false, progress: false };
       let output = run_search(&args, &ctx)?;
       Ok(CallToolResult::success(vec![Content::text(output)]))
   }
   ```

4. **Remove duplicate tool implementations** from mcp_server/mod.rs

## Files to Modify
- `src/mcp_server/mod.rs` - Consolidate 35+ tools to ~18
- `src/mcp_server/types.rs` - Update request types
- `src/commands/mod.rs` - Add conversion traits if needed

## Testing
After consolidation:
1. Run `cargo test`
2. Test MCP server with Claude Code
3. Verify all tools work identically to CLI
