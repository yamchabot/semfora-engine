# Semfora Engine — Quick Start

## 1. Build

```bash
git clone https://github.com/Semfora-AI/semfora-engine.git
cd semfora-engine
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## 2. Index Your Project

```bash
cd /your/project
semfora-engine index generate .
```

Done in seconds. The index lives in `~/.cache/semfora-engine/`.

## 3. Query It

```bash
semfora-engine query overview              # architecture summary
semfora-engine search "authenticate"       # find symbols + related code
semfora-engine query callers <hash>        # what calls this symbol?
semfora-engine analyze --uncommitted       # review your staged changes
```

## 4. Set Up the MCP Server (for AI Coding Assistants)

Add to your MCP client config (Claude Desktop, Cursor, etc.):

```json
{
  "mcpServers": {
    "semfora": {
      "command": "/path/to/semfora-engine/target/release/semfora-engine",
      "args": ["serve", "--repo", "/your/project"]
    }
  }
}
```

> **Note:** There is no separate `semfora-engine-server` binary.
> The MCP server is `semfora-engine serve`.

### AI Agent System Prompt Snippet

When using Semfora via MCP, add this to your agent's system prompt so it uses semantic tools instead of raw file access:

```
You have access to the Semfora MCP server for code analysis.
Use Semfora tools instead of reading files directly:
- Use `search` to find symbols and code
- Use `get_source` to read specific functions (not the whole file)
- Use `analyze` to understand a file or directory
- Use `get_callers` before modifying any function
- Only use raw file Read for small config files or non-code assets
```

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| "No index found" | Run `semfora-engine index generate .` in your project root |
| Index is stale after changes | `semfora-engine index generate . --incremental` |
| Wrong project being indexed | Run the command from inside the git repo |
| MCP server not connecting | Check the `command` path is absolute and the binary exists |

## More

- [`docs/cli.md`](docs/cli.md) — Full command reference
- [`docs/mcp-tools-reference.md`](docs/mcp-tools-reference.md) — All 18 MCP tools
- [`docs/websocket-daemon.md`](docs/websocket-daemon.md) — Real-time daemon for multi-client setups
