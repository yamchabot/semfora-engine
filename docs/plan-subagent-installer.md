# Plan: Subagent Workflow Installation

## Overview

Add the ability to install Semfora workflow agents (audit, search, review, impact, quality) during the `semfora setup` process. Users can opt-in to install agents at global or project level for any supported editor.

## Current State

The installer already supports:
- 6 editor integrations: Claude Desktop, Claude Code, Cursor, VS Code, OpenAI Codex, Custom
- Global and project-level MCP config for most editors
- Interactive multi-select UI with `dialoguer`
- Atomic file operations with backups

## Design Goals

1. **Opt-in by default** - Agents are NOT installed unless explicitly selected
2. **Sub-checkbox UI** - Each editor shows agent options only when selected
3. **Scope selection** - Global (`~/.claude/agents/`) or Project (`.claude/agents/`)
4. **Platform adaptation** - Convert agent templates to each platform's format
5. **Idempotent** - Safe to run multiple times, updates existing agents
6. **Prefix convention** - Use `semfora-` prefix to avoid conflicts with user agents

## Agent Support by Platform

| Platform | Agent Support | Global Path | Project Path | Format |
|----------|---------------|-------------|--------------|--------|
| Claude Code | Native | `~/.claude/agents/*.md` | `.claude/agents/*.md` | Markdown + YAML frontmatter |
| Cursor | Rules/Notepads | `~/.cursor/rules/` | `.cursor/rules/` | Markdown |
| VS Code | Continue.dev | `~/.continue/` | `.continue/` | JSON config |
| Claude Desktop | Via MCP | N/A (MCP only) | N/A | N/A |
| OpenAI Codex | AGENTS.md | `~/.codex/` | `.codex/` or `AGENTS.md` | Markdown sections |

## Implementation Plan

### Phase 1: Core Infrastructure

#### 1.1 Agent Template Storage

Create embedded templates that can be converted to each platform's format.

```
src/installer/agents/
├── mod.rs              # Agent installer logic
├── templates/          # Source templates (Claude Code format)
│   ├── semfora-audit.md
│   ├── semfora-search.md
│   ├── semfora-review.md
│   ├── semfora-impact.md
│   └── semfora-quality.md
└── converters/         # Platform-specific converters
    ├── mod.rs
    ├── claude_code.rs  # Pass-through (native format)
    ├── cursor.rs       # Convert to Cursor rules
    ├── continue_dev.rs # Convert to Continue.dev JSON
    └── codex.rs        # Convert to AGENTS.md sections
```

**Embed templates at compile time:**
```rust
// src/installer/agents/templates/mod.rs
pub const SEMFORA_AUDIT: &str = include_str!("semfora-audit.md");
pub const SEMFORA_SEARCH: &str = include_str!("semfora-search.md");
pub const SEMFORA_REVIEW: &str = include_str!("semfora-review.md");
pub const SEMFORA_IMPACT: &str = include_str!("semfora-impact.md");
pub const SEMFORA_QUALITY: &str = include_str!("semfora-quality.md");
```

#### 1.2 Agent Scope Enum

```rust
// src/installer/agents/mod.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentScope {
    Global,   // ~/.claude/agents/
    Project,  // ./.claude/agents/
    Both,     // Install to both locations
}

#[derive(Debug, Clone)]
pub struct AgentInstallConfig {
    pub enabled: bool,
    pub scope: AgentScope,
}
```

#### 1.3 Platform Agent Trait

```rust
pub trait AgentInstaller {
    /// Check if this platform supports agents
    fn supports_agents(&self) -> bool;

    /// Get the global agents directory for this platform
    fn global_agents_dir(&self) -> Option<PathBuf>;

    /// Get the project agents directory for this platform
    fn project_agents_dir(&self) -> Option<PathBuf>;

    /// Convert a Claude Code agent template to this platform's format
    fn convert_agent(&self, template: &str, name: &str) -> Result<String>;

    /// Install agents to the specified scope
    fn install_agents(&self, scope: AgentScope) -> Result<Vec<PathBuf>>;
}
```

### Phase 2: Platform Converters

#### 2.1 Claude Code (Native - No Conversion)

```rust
// src/installer/agents/converters/claude_code.rs
impl AgentConverter for ClaudeCodeConverter {
    fn convert(&self, template: &str, _name: &str) -> Result<String> {
        // Native format - pass through unchanged
        Ok(template.to_string())
    }

    fn file_extension(&self) -> &str {
        "md"
    }
}
```

**Paths:**
- Global: `~/.claude/agents/semfora-{name}.md`
- Project: `.claude/agents/semfora-{name}.md`

#### 2.2 Cursor

Cursor uses `.cursorrules` or `.cursor/rules/` for custom instructions.

```rust
// src/installer/agents/converters/cursor.rs
impl AgentConverter for CursorConverter {
    fn convert(&self, template: &str, name: &str) -> Result<String> {
        // Extract just the prompt content (remove YAML frontmatter)
        // Wrap in Cursor-specific format
        let content = extract_prompt_body(template)?;

        Ok(format!(r#"# Semfora {name} Agent

When the user asks for "{}" related tasks, follow this workflow:

{content}

## Tool Access
Use the semfora-engine MCP tools available in your context.
"#, name_to_trigger(name)))
    }
}
```

**Paths:**
- Global: `~/.cursor/rules/semfora-{name}.md`
- Project: `.cursor/rules/semfora-{name}.md`

#### 2.3 Continue.dev (VS Code)

Continue.dev uses JSON configuration with custom commands.

```rust
// src/installer/agents/converters/continue_dev.rs
impl AgentConverter for ContinueDevConverter {
    fn convert(&self, template: &str, name: &str) -> Result<String> {
        // Convert to Continue.dev custom command format
        let prompt = extract_prompt_body(template)?;

        let command = json!({
            "name": format!("semfora-{}", name),
            "description": extract_description(template)?,
            "prompt": prompt
        });

        Ok(serde_json::to_string_pretty(&command)?)
    }
}
```

**Integration approach:**
- Read existing `~/.continue/config.json`
- Add/update `customCommands` array
- Write back atomically

#### 2.4 OpenAI Codex

Codex uses an `AGENTS.md` file with markdown sections.

```rust
// src/installer/agents/converters/codex.rs
impl AgentConverter for CodexConverter {
    fn convert(&self, template: &str, name: &str) -> Result<String> {
        let prompt = extract_prompt_body(template)?;

        Ok(format!(r#"## Semfora {name}

{prompt}
"#))
    }

    fn install_mode(&self) -> InstallMode {
        // Codex appends to a single AGENTS.md file
        InstallMode::AppendToFile("AGENTS.md".to_string())
    }
}
```

**Paths:**
- Global: `~/.codex/AGENTS.md` (append sections)
- Project: `AGENTS.md` or `.codex/AGENTS.md`

#### 2.5 Claude Desktop

Claude Desktop doesn't have native agent support - it relies on MCP tools.

```rust
impl AgentConverter for ClaudeDesktopConverter {
    fn supports_agents(&self) -> bool {
        false  // No native agent support
    }
}
```

**Alternative approach:** Could create a "semfora-agents" MCP server that exposes agent prompts as resources. Deferred for future implementation.

### Phase 3: UI/UX Changes

#### 3.1 Updated Wizard Flow

```
┌─────────────────────────────────────────────────────────────┐
│  Select AI tools to configure:                              │
│                                                             │
│  [x] Claude Code                                            │
│      └─ [ ] Install Semfora workflow agents                 │
│           ○ Global (~/.claude/agents/)                      │
│           ○ Project (.claude/agents/)                       │
│           ○ Both                                            │
│                                                             │
│  [x] Cursor                                                 │
│      └─ [ ] Install Semfora workflow agents                 │
│           ○ Global (~/.cursor/rules/)                       │
│           ○ Project (.cursor/rules/)                        │
│                                                             │
│  [ ] VS Code (Continue.dev)                                 │
│      └─ [ ] Install Semfora workflow agents                 │
│                                                             │
│  [x] Claude Desktop                                         │
│      (Agents not supported - uses MCP tools directly)       │
│                                                             │
│  [ ] OpenAI Codex                                           │
│      └─ [ ] Install Semfora workflow agents                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2 Implementation in wizard.rs

```rust
// After client selection, show agent options for each selected client
fn prompt_agent_options(selected_clients: &[ClientType]) -> Result<HashMap<ClientType, AgentInstallConfig>> {
    let mut configs = HashMap::new();

    for client in selected_clients {
        if !client.supports_agents() {
            println!("  {} (Agents not supported - uses MCP tools directly)",
                     client.display_name().dimmed());
            continue;
        }

        let install_agents = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("  └─ Install Semfora workflow agents for {}?", client.display_name()))
            .default(false)  // Default OFF
            .interact()?;

        if install_agents {
            let scope = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("     Scope")
                .items(&[
                    format!("Global ({})", client.global_agents_path().display()),
                    format!("Project ({})", client.project_agents_path().display()),
                    "Both".to_string(),
                ])
                .default(0)
                .interact()?;

            configs.insert(*client, AgentInstallConfig {
                enabled: true,
                scope: match scope {
                    0 => AgentScope::Global,
                    1 => AgentScope::Project,
                    _ => AgentScope::Both,
                },
            });
        }
    }

    Ok(configs)
}
```

#### 3.3 Non-Interactive CLI Flags

```rust
// In SetupArgs
#[derive(Parser)]
pub struct SetupArgs {
    // ... existing args ...

    /// Install Semfora workflow agents for selected clients
    #[arg(long, default_value = "false")]
    pub with_agents: bool,

    /// Agent installation scope (global, project, both)
    #[arg(long, default_value = "global")]
    pub agents_scope: AgentScope,

    /// Only install agents (skip MCP configuration)
    #[arg(long)]
    pub agents_only: bool,
}
```

**Example usage:**
```bash
# Install with agents globally
semfora setup --non-interactive --clients claude-code,cursor --with-agents

# Install agents to project only
semfora setup --non-interactive --clients claude-code --with-agents --agents-scope project

# Just install agents (MCP already configured)
semfora setup --agents-only --clients claude-code --agents-scope both
```

### Phase 4: Installation Logic

#### 4.1 Main Installation Flow

```rust
// src/installer/agents/mod.rs

pub fn install_agents_for_client(
    client: &dyn McpClient,
    scope: AgentScope,
) -> Result<InstallResult> {
    let converter = get_converter_for_client(client)?;
    let templates = get_all_templates();
    let mut installed = Vec::new();

    for (name, template) in templates {
        let converted = converter.convert(template, name)?;

        match scope {
            AgentScope::Global => {
                let path = install_to_global(client, name, &converted)?;
                installed.push(path);
            }
            AgentScope::Project => {
                let path = install_to_project(client, name, &converted)?;
                installed.push(path);
            }
            AgentScope::Both => {
                installed.push(install_to_global(client, name, &converted)?);
                installed.push(install_to_project(client, name, &converted)?);
            }
        }
    }

    Ok(InstallResult { installed_paths: installed })
}

fn install_to_global(client: &dyn McpClient, name: &str, content: &str) -> Result<PathBuf> {
    let dir = client.global_agents_dir()?;
    fs::create_dir_all(&dir)?;

    let filename = format!("semfora-{}.{}", name, client.agent_file_extension());
    let path = dir.join(&filename);

    // Atomic write with backup
    if path.exists() {
        let backup = path.with_extension("md.backup");
        fs::copy(&path, &backup)?;
    }

    atomic_write(&path, content)?;
    Ok(path)
}
```

#### 4.2 Update Existing Agents

```rust
fn should_update_agent(existing: &Path, new_content: &str) -> Result<bool> {
    let existing_content = fs::read_to_string(existing)?;

    // Extract version from frontmatter if present
    let existing_version = extract_version(&existing_content);
    let new_version = extract_version(new_content);

    match (existing_version, new_version) {
        (Some(ev), Some(nv)) => Ok(nv > ev),
        _ => {
            // No version info - compare content hash
            Ok(hash(&existing_content) != hash(new_content))
        }
    }
}
```

### Phase 5: Agent Templates

#### 5.1 Template Format (Claude Code Native)

Templates are stored in Claude Code's native format and converted for other platforms.

```markdown
---
name: semfora-audit
description: Full codebase audit using semfora-engine...
model: sonnet
version: 1.0.0
---

You are a codebase audit specialist...

## Step 0: Load All Tools First
...
```

#### 5.2 Version Tracking

Add `version` field to frontmatter for update detection:

```yaml
---
name: semfora-audit
version: 1.0.0
---
```

#### 5.3 All 5 Agent Templates

Copy the current agents from `adk-playground/.claude/agents/`:
- `semfora-audit.md` - Full codebase audit
- `semfora-search.md` - Fast semantic search
- `semfora-review.md` - PR/diff review
- `semfora-impact.md` - Refactoring impact analysis
- `semfora-quality.md` - Code quality metrics

### Phase 6: Summary & Confirmation UI

#### 6.1 Installation Summary

```
┌─────────────────────────────────────────────────────────────┐
│  Installation Summary                                       │
│                                                             │
│  MCP Server Configuration:                                  │
│    ✓ Claude Code    ~/.claude/mcp.json                      │
│    ✓ Cursor         ~/.cursor/mcp.json                      │
│                                                             │
│  Semfora Workflow Agents:                                   │
│    ✓ Claude Code (Global)                                   │
│      → ~/.claude/agents/semfora-audit.md                    │
│      → ~/.claude/agents/semfora-search.md                   │
│      → ~/.claude/agents/semfora-review.md                   │
│      → ~/.claude/agents/semfora-impact.md                   │
│      → ~/.claude/agents/semfora-quality.md                  │
│                                                             │
│    ✓ Cursor (Project)                                       │
│      → .cursor/rules/semfora-audit.md                       │
│      → .cursor/rules/semfora-search.md                      │
│      → .cursor/rules/semfora-review.md                      │
│      → .cursor/rules/semfora-impact.md                      │
│      → .cursor/rules/semfora-quality.md                     │
│                                                             │
│  Proceed with installation? [Y/n]                           │
└─────────────────────────────────────────────────────────────┘
```

#### 6.2 Post-Installation Instructions

```
┌─────────────────────────────────────────────────────────────┐
│  ✓ Installation Complete!                                   │
│                                                             │
│  Semfora workflow agents installed:                         │
│                                                             │
│  Claude Code Usage:                                         │
│    • Type "/agents" to see available agents                 │
│    • Use "semfora-audit" for codebase audits                │
│    • Use "semfora-search" for semantic code search          │
│    • Use "semfora-review" for PR reviews                    │
│    • Use "semfora-impact" for refactoring analysis          │
│    • Use "semfora-quality" for code quality checks          │
│                                                             │
│  Cursor Usage:                                              │
│    • Agents available as custom rules                       │
│    • Reference @semfora-audit in chat                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Phase 7: Uninstall Support

#### 7.1 Update Uninstall Menu

```
What would you like to remove?
  1. Remove MCP configs only
  2. Remove Semfora workflow agents only
  3. Remove engine cache
  4. Remove everything (configs + agents + cache)
```

#### 7.2 Agent Removal Logic

```rust
pub fn uninstall_agents(client: &dyn McpClient, scope: AgentScope) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    let prefix = "semfora-";

    let dirs = match scope {
        AgentScope::Global => vec![client.global_agents_dir()?],
        AgentScope::Project => vec![client.project_agents_dir()?],
        AgentScope::Both => vec![
            client.global_agents_dir()?,
            client.project_agents_dir()?,
        ],
    };

    for dir in dirs {
        if !dir.exists() { continue; }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with(prefix) {
                fs::remove_file(entry.path())?;
                removed.push(entry.path());
            }
        }
    }

    Ok(removed)
}
```

## File Structure (Final)

```
src/installer/
├── mod.rs
├── wizard.rs           # Updated with agent UI
├── config.rs
├── platform/
├── clients/
│   ├── mod.rs          # Add supports_agents() to trait
│   ├── claude_code.rs  # Add agent paths
│   ├── cursor.rs       # Add agent paths
│   ├── vscode.rs       # Add Continue.dev integration
│   ├── claude_desktop.rs
│   ├── openai_codex.rs # Add AGENTS.md support
│   └── custom.rs
└── agents/             # NEW
    ├── mod.rs          # Core installation logic
    ├── templates/      # Embedded templates
    │   ├── mod.rs
    │   ├── semfora-audit.md
    │   ├── semfora-search.md
    │   ├── semfora-review.md
    │   ├── semfora-impact.md
    │   └── semfora-quality.md
    └── converters/     # Platform-specific converters
        ├── mod.rs
        ├── claude_code.rs
        ├── cursor.rs
        ├── continue_dev.rs
        └── codex.rs
```

## Implementation Order

1. **Core infrastructure** (agents/mod.rs, templates)
2. **Claude Code support** (native, no conversion)
3. **Wizard UI updates** (sub-checkboxes, scope selection)
4. **CLI flags** (--with-agents, --agents-scope)
5. **Cursor support** (converter)
6. **Continue.dev support** (converter)
7. **Codex support** (converter)
8. **Uninstall support**
9. **Testing & documentation**

## Testing Strategy

1. **Unit tests** for each converter
2. **Integration tests** for installation/uninstall
3. **Manual testing** on each platform:
   - macOS (Intel + ARM)
   - Linux (x86_64)
   - Windows
4. **Verify agent functionality** in each editor after installation

## Future Enhancements

1. **Agent updates** - `semfora agents update` command
2. **Custom agents** - Allow users to add their own agent templates
3. **Agent marketplace** - Fetch community agents from URL
4. **Claude Desktop agents** - Implement as MCP resource server
5. **Windsurf support** - Add when format is documented
