//! Enhanced TestRepo builder for comprehensive integration testing
//!
//! Provides language-specific builders and pre-configured repo structures
//! for testing symbol extraction, visibility detection, call graphs, and more.

#![allow(clippy::map_clone)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Builder for creating test repository structures with support for all 27 languages
pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    /// Create a new empty test repository
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().expect("Failed to create temp dir"),
        }
    }

    /// Get the path to the test repository root
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Get path as string
    pub fn path_str(&self) -> String {
        self.dir.path().to_string_lossy().to_string()
    }

    /// Add a source file with the given content
    pub fn add_file(&self, relative_path: &str, content: &str) -> &Self {
        let full_path = self.dir.path().join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        fs::write(&full_path, content).expect("Failed to write file");
        self
    }

    /// Run semfora-engine CLI command and return output
    pub fn run_cli(&self, args: &[&str]) -> std::io::Result<Output> {
        let binary =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/semfora-engine");

        Command::new(&binary)
            .current_dir(self.path())
            .args(args)
            .output()
    }

    /// Run CLI and expect success, return stdout
    pub fn run_cli_success(&self, args: &[&str]) -> String {
        let output = self.run_cli(args).expect("Failed to run CLI");
        assert!(
            output.status.success(),
            "CLI command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// Run CLI and expect failure, return (stdout, stderr)
    pub fn run_cli_failure(&self, args: &[&str]) -> (String, String) {
        let output = self.run_cli(args).expect("Failed to run CLI");
        assert!(
            !output.status.success(),
            "CLI command {:?} should have failed",
            args
        );
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    /// Run index generate on this repo
    pub fn generate_index(&self) -> std::io::Result<Output> {
        self.run_cli(&["index", "generate"])
    }

    /// Initialize as a git repository
    pub fn init_git(&self) -> &Self {
        Command::new("git")
            .current_dir(self.path())
            .args(["init"])
            .output()
            .expect("Failed to init git");
        Command::new("git")
            .current_dir(self.path())
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("Failed to configure git");
        Command::new("git")
            .current_dir(self.path())
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("Failed to configure git");
        self
    }

    /// Create a git commit with all files
    pub fn commit(&self, message: &str) -> &Self {
        Command::new("git")
            .current_dir(self.path())
            .args(["add", "-A"])
            .output()
            .expect("Failed to git add");
        Command::new("git")
            .current_dir(self.path())
            .args(["commit", "-m", message])
            .output()
            .expect("Failed to git commit");
        self
    }

    // ========================================================================
    // PRE-BUILT REPO STRUCTURES
    // ========================================================================

    /// Create a standard src/ layout with multiple modules
    pub fn with_standard_src_layout(&self) -> &Self {
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
    pub fn with_deep_nesting(&self) -> &Self {
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
        .add_file(
            "src/Program.cs",
            "public class Program { public static void Main() {} }",
        )
    }

    /// Create a monorepo structure
    pub fn with_monorepo_layout(&self) -> &Self {
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
    pub fn with_conflict_structure(&self) -> &Self {
        self.add_ts_function("src/game/player.ts", "gamePlayer", "")
            .add_ts_function("src/map/player.ts", "mapPlayer", "")
            .add_ts_function("src/ui/player.ts", "uiPlayer", "")
    }

    /// Create a structure with duplicate functions
    pub fn with_duplicates(&self) -> &Self {
        let validate_body = "return email.includes('@') && email.length > 5;";
        self.add_ts_function("src/auth/validate.ts", "validateEmail", validate_body)
            .add_ts_function("src/users/validate.ts", "validateEmail", validate_body)
            .add_ts_function("src/orders/validate.ts", "validateEmail", validate_body)
    }

    /// Create a multi-language project
    pub fn with_multilang(&self) -> &Self {
        self.add_ts_function("src/frontend/app.ts", "initApp", "")
            .add_rs_function("src/backend/main.rs", "main", "println!(\"Hello\");")
            .add_py_function("scripts/deploy.py", "deploy", "print('deploying')")
            .add_go_function("tools/cli/main.go", "main", "Run", "fmt.Println(\"CLI\")")
    }

    /// Create a repo with complex call graph
    pub fn with_complex_callgraph(&self) -> &Self {
        self.add_file(
            "src/service.ts",
            r#"
export function fetchUsers() {
    return apiClient.get('/users');
}

export function processUsers() {
    const users = fetchUsers();
    return users.map(formatUser);
}

export function formatUser(user: any) {
    return { ...user, name: user.name.toUpperCase() };
}

export function main() {
    const processed = processUsers();
    saveToCache(processed);
}

function saveToCache(data: any) {
    cache.set('users', data);
}
"#,
        )
    }

    /// Create a repo with security issues (for CVE testing)
    pub fn with_security_issues(&self) -> &Self {
        self.add_file(
            "src/vulnerable.ts",
            r#"
// SQL injection vulnerability
export function getUser(id: string) {
    return db.query(`SELECT * FROM users WHERE id = '${id}'`);
}

// Command injection
export function runCommand(cmd: string) {
    return exec(cmd);
}

// Path traversal
export function readFile(filename: string) {
    return fs.readFileSync('/data/' + filename);
}
"#,
        )
    }

    // ========================================================================
    // TYPESCRIPT / JAVASCRIPT FAMILY
    // ========================================================================

    /// Add a TypeScript file with a simple function
    pub fn add_ts_function(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"export function {}() {{
    {}
}}
"#,
            fn_name, body
        );
        self.add_file(relative_path, &content)
    }

    /// Add a TypeScript module with interface, class, and function
    pub fn add_ts_module(&self, relative_path: &str, module_name: &str) -> &Self {
        let content = format!(
            r#"// {module_name} module

export interface I{module_name}Config {{
    enabled: boolean;
    timeout: number;
}}

export class {module_name}Service {{
    private config: I{module_name}Config;

    constructor(config: I{module_name}Config) {{
        this.config = config;
    }}

    public async process(): Promise<void> {{
        if (this.config.enabled) {{
            await this.doWork();
        }}
    }}

    private async doWork(): Promise<void> {{
        console.log('Working...');
    }}
}}

export function create{module_name}(config: I{module_name}Config): {module_name}Service {{
    return new {module_name}Service(config);
}}

// Private function (not exported)
function internalHelper(): void {{
    console.log('internal');
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a React functional component with hooks
    pub fn add_react_component(&self, relative_path: &str, name: &str, hooks: &[&str]) -> &Self {
        let hook_imports: Vec<&str> = hooks.iter().map(|h| *h).collect();
        let hook_import_str = if hook_imports.is_empty() {
            "React".to_string()
        } else {
            format!("React, {{ {} }}", hook_imports.join(", "))
        };

        let hook_usage: String = hooks
            .iter()
            .map(|h| match *h {
                "useState" => "    const [count, setCount] = useState(0);".to_string(),
                "useEffect" => "    useEffect(() => { console.log('mounted'); return () => console.log('unmounted'); }, []);".to_string(),
                "useMemo" => "    const doubled = useMemo(() => count * 2, [count]);".to_string(),
                "useCallback" => "    const handleClick = useCallback(() => setCount(c => c + 1), []);".to_string(),
                "useRef" => "    const inputRef = useRef<HTMLInputElement>(null);".to_string(),
                "useContext" => "    const theme = useContext(ThemeContext);".to_string(),
                "useReducer" => "    const [state, dispatch] = useReducer(reducer, initialState);".to_string(),
                _ => format!("    // {} hook usage", h),
            })
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            r#"import {hook_import_str} from 'react';

interface {name}Props {{
    title: string;
    onAction?: () => void;
}}

export const {name}: React.FC<{name}Props> = ({{ title, onAction }}) => {{
{hook_usage}

    return (
        <div className="{name_lower}">
            <h1>{{title}}</h1>
            <button onClick={{onAction}}>Action</button>
        </div>
    );
}};

export default {name};
"#,
            name_lower = name.to_lowercase()
        );
        self.add_file(relative_path, &content)
    }

    /// Add a Vue Single File Component
    pub fn add_vue_component(&self, relative_path: &str, name: &str) -> &Self {
        let content = format!(
            r#"<template>
    <div class="{name_lower}">
        <h1>{{ title }}</h1>
        <button @click="handleClick">{{ buttonText }}</button>
        <p>Count: {{ count }}</p>
    </div>
</template>

<script setup lang="ts">
import {{ ref, computed, onMounted }} from 'vue';

interface Props {{
    title: string;
    initialCount?: number;
}}

const props = withDefaults(defineProps<Props>(), {{
    initialCount: 0
}});

const emit = defineEmits<{{
    (e: 'update', value: number): void;
    (e: 'click'): void;
}}>()

const count = ref(props.initialCount);
const buttonText = computed(() => `Clicked ${{count.value}} times`);

function handleClick() {{
    count.value++;
    emit('update', count.value);
    emit('click');
}}

onMounted(() => {{
    console.log('{name} mounted');
}});
</script>

<style scoped>
.{name_lower} {{
    padding: 1rem;
}}
</style>
"#,
            name_lower = name.to_lowercase()
        );
        self.add_file(relative_path, &content)
    }

    /// Add JavaScript file with CommonJS exports
    pub fn add_js_commonjs(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"function {fn_name}() {{
    {body}
}}

module.exports = {{ {fn_name} }};
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add JavaScript file with ES modules
    pub fn add_js_esm(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"export function {fn_name}() {{
    {body}
}}

export default {fn_name};
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add JSX component
    pub fn add_jsx_component(&self, relative_path: &str, name: &str) -> &Self {
        let content = format!(
            r#"import React from 'react';

export function {name}({{ children }}) {{
    return (
        <div className="{name_lower}">
            {{children}}
        </div>
    );
}}
"#,
            name_lower = name.to_lowercase()
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // RUST
    // ========================================================================

    /// Add a Rust file with a simple function
    pub fn add_rs_function(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"pub fn {fn_name}() {{
    {body}
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a Rust module with struct, trait, and impl
    pub fn add_rs_module(&self, relative_path: &str, name: &str) -> &Self {
        let content = format!(
            r#"//! {name} module

/// Configuration for {name}
pub struct {name}Config {{
    pub enabled: bool,
    pub timeout: u64,
}}

/// Trait for {name} operations
pub trait {name}Trait {{
    fn process(&self) -> Result<(), Error>;
    fn validate(&self) -> bool;
}}

/// Main {name} service
pub struct {name}Service {{
    config: {name}Config,
}}

impl {name}Service {{
    pub fn new(config: {name}Config) -> Self {{
        Self {{ config }}
    }}

    pub fn run(&self) -> Result<(), Error> {{
        if self.config.enabled {{
            self.do_work()
        }} else {{
            Ok(())
        }}
    }}

    fn do_work(&self) -> Result<(), Error> {{
        println!("Working...");
        Ok(())
    }}
}}

impl {name}Trait for {name}Service {{
    fn process(&self) -> Result<(), Error> {{
        self.run()
    }}

    fn validate(&self) -> bool {{
        self.config.timeout > 0
    }}
}}

// Private helper (not pub)
fn internal_helper() {{
    println!("internal");
}}

#[derive(Debug)]
pub struct Error;
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // PYTHON
    // ========================================================================

    /// Add a Python file with a simple function
    pub fn add_py_function(&self, relative_path: &str, fn_name: &str, body: &str) -> &Self {
        let content = format!(
            r#"def {fn_name}():
    {body}
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a Python module with dataclass and type hints
    pub fn add_py_module(&self, relative_path: &str, name: &str) -> &Self {
        let content = format!(
            r#"\"\"\"
{name} module
\"\"\"
from dataclasses import dataclass
from typing import Optional, List
import asyncio


@dataclass
class {name}Config:
    \"\"\"Configuration for {name}\"\"\"
    enabled: bool = True
    timeout: int = 30


class {name}Service:
    \"\"\"Main {name} service class\"\"\"

    def __init__(self, config: {name}Config):
        self._config = config
        self._cache: dict = {{}}

    async def process(self) -> None:
        \"\"\"Process data asynchronously\"\"\"
        if self._config.enabled:
            await self._do_work()

    async def _do_work(self) -> None:
        \"\"\"Internal work method (private)\"\"\"
        await asyncio.sleep(0.1)
        print("Working...")

    def validate(self) -> bool:
        \"\"\"Validate configuration\"\"\"
        return self._config.timeout > 0


def create_{name_lower}(config: Optional[{name}Config] = None) -> {name}Service:
    \"\"\"Factory function to create {name}Service\"\"\"
    if config is None:
        config = {name}Config()
    return {name}Service(config)


def _internal_helper() -> None:
    \"\"\"Private helper function (underscore prefix)\"\"\"
    print("internal")
"#,
            name_lower = name.to_lowercase()
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // GO
    // ========================================================================

    /// Add a Go file with exported and unexported symbols
    pub fn add_go_function(
        &self,
        relative_path: &str,
        pkg: &str,
        exported_fn: &str,
        body: &str,
    ) -> &Self {
        let content = format!(
            r#"package {pkg}

import "fmt"

// {exported_fn} is an exported function (uppercase)
func {exported_fn}() {{
    {body}
    helper()
}}

// helper is unexported (lowercase)
func helper() {{
    fmt.Println("helper")
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a Go module with struct, interface, and methods
    pub fn add_go_module(&self, relative_path: &str, pkg: &str, name: &str) -> &Self {
        let name_lower = name.to_lowercase();
        let content = format!(
            r#"package {pkg}

import (
    "context"
    "fmt"
)

// {name}Config holds configuration (exported - uppercase)
type {name}Config struct {{
    Enabled bool
    Timeout int
}}

// {name}Service is the main service (exported)
type {name}Service struct {{
    config {name}Config
    cache  map[string]interface{{}}
}}

// {name}er interface for {name} operations (exported)
type {name}er interface {{
    Process(ctx context.Context) error
    Validate() bool
}}

// New{name} creates a new {name}Service (exported factory)
func New{name}(config {name}Config) *{name}Service {{
    return &{name}Service{{
        config: config,
        cache:  make(map[string]interface{{}}),
    }}
}}

// Process implements {name}er (exported method)
func (s *{name}Service) Process(ctx context.Context) error {{
    if s.config.Enabled {{
        return s.doWork(ctx)
    }}
    return nil
}}

// doWork is a private method (unexported - lowercase)
func (s *{name}Service) doWork(ctx context.Context) error {{
    fmt.Println("Working...")
    return nil
}}

// Validate implements {name}er
func (s *{name}Service) Validate() bool {{
    return s.config.Timeout > 0
}}

// {name_lower}Helper is unexported (lowercase)
func {name_lower}Helper() {{
    fmt.Println("internal helper")
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // JAVA
    // ========================================================================

    /// Add a Java class with annotations and visibility modifiers
    pub fn add_java_class(&self, relative_path: &str, pkg: &str, name: &str) -> &Self {
        let content = format!(
            r#"package {pkg};

import java.util.concurrent.CompletableFuture;

/**
 * {name} service class
 */
public class {name}Service {{

    private final {name}Config config;

    public {name}Service({name}Config config) {{
        this.config = config;
    }}

    /**
     * Process data asynchronously
     */
    public CompletableFuture<Void> processAsync() {{
        return CompletableFuture.runAsync(() -> {{
            if (config.isEnabled()) {{
                doWork();
            }}
        }});
    }}

    /**
     * Synchronous processing
     */
    public void process() {{
        if (config.isEnabled()) {{
            doWork();
        }}
    }}

    private void doWork() {{
        System.out.println("Working...");
    }}

    // Package-private method
    void internalHelper() {{
        System.out.println("internal");
    }}

    protected void protectedMethod() {{
        System.out.println("protected");
    }}
}}

/**
 * Configuration class
 */
class {name}Config {{
    private boolean enabled = true;
    private int timeout = 30;

    public boolean isEnabled() {{
        return enabled;
    }}

    public void setEnabled(boolean enabled) {{
        this.enabled = enabled;
    }}

    public int getTimeout() {{
        return timeout;
    }}
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // KOTLIN
    // ========================================================================

    /// Add a Kotlin file with data class, sealed class, and coroutines
    pub fn add_kotlin_class(&self, relative_path: &str, pkg: &str, name: &str) -> &Self {
        let content = format!(
            r#"package {pkg}

import kotlinx.coroutines.*

/**
 * Configuration data class (public by default in Kotlin)
 */
data class {name}Config(
    val enabled: Boolean = true,
    val timeout: Int = 30
)

/**
 * Sealed class for results
 */
sealed class {name}Result {{
    data class Success(val data: String) : {name}Result()
    data class Error(val message: String) : {name}Result()
    object Loading : {name}Result()
}}

/**
 * Main service class
 */
class {name}Service(private val config: {name}Config) {{

    /**
     * Suspend function for async processing
     */
    suspend fun processAsync(): {name}Result {{
        return if (config.enabled) {{
            doWork()
        }} else {{
            {name}Result.Error("Disabled")
        }}
    }}

    private suspend fun doWork(): {name}Result {{
        delay(100)
        return {name}Result.Success("Done")
    }}

    internal fun internalHelper() {{
        println("internal")
    }}

    private fun privateHelper() {{
        println("private")
    }}
}}

/**
 * Factory function (top-level, public by default)
 */
fun create{name}(config: {name}Config = {name}Config()): {name}Service {{
    return {name}Service(config)
}}

// Private top-level function
private fun helperFunction() {{
    println("helper")
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // C#
    // ========================================================================

    /// Add a C# file with record, interface, and async methods
    pub fn add_csharp_class(&self, relative_path: &str, namespace: &str, name: &str) -> &Self {
        let content = format!(
            r#"namespace {namespace};

/// <summary>
/// Configuration record
/// </summary>
public record {name}Config(bool Enabled = true, int Timeout = 30);

/// <summary>
/// Service interface
/// </summary>
public interface I{name}Service
{{
    Task ProcessAsync(CancellationToken ct = default);
    bool Validate();
}}

/// <summary>
/// Main service implementation
/// </summary>
public class {name}Service : I{name}Service
{{
    private readonly {name}Config _config;

    public {name}Service({name}Config config)
    {{
        _config = config;
    }}

    public async Task ProcessAsync(CancellationToken ct = default)
    {{
        if (_config.Enabled)
        {{
            await DoWorkAsync(ct);
        }}
    }}

    public bool Validate() => _config.Timeout > 0;

    private async Task DoWorkAsync(CancellationToken ct)
    {{
        await Task.Delay(100, ct);
        Console.WriteLine("Working...");
    }}

    internal void InternalHelper()
    {{
        Console.WriteLine("internal");
    }}

    protected virtual void ProtectedMethod()
    {{
        Console.WriteLine("protected");
    }}
}}

/// <summary>
/// Internal helper class
/// </summary>
internal class {name}Helper
{{
    public static void Help() => Console.WriteLine("help");
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // C / C++
    // ========================================================================

    /// Add a C file with extern and static functions
    pub fn add_c_module(&self, relative_path: &str, name: &str) -> &Self {
        let name_upper = name.to_uppercase();
        let content = format!(
            r#"#ifndef {name_upper}_H
#define {name_upper}_H

#include <stdio.h>
#include <stdlib.h>

// Public struct (in header)
typedef struct {{
    int enabled;
    int timeout;
}} {name}Config;

// Public function declarations (extern)
extern int {name}_init({name}Config* config);
extern int {name}_process(void);
extern void {name}_cleanup(void);

#endif // {name_upper}_H

// Implementation
static int internal_helper(void);
static {name}Config* g_config = NULL;

int {name}_init({name}Config* config) {{
    g_config = config;
    return 0;
}}

int {name}_process(void) {{
    if (g_config && g_config->enabled) {{
        return internal_helper();
    }}
    return -1;
}}

void {name}_cleanup(void) {{
    g_config = NULL;
}}

// Static (private) function
static int internal_helper(void) {{
    printf("Working...\\n");
    return 0;
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a C++ file with class, namespace, and templates
    pub fn add_cpp_module(&self, relative_path: &str, namespace: &str, name: &str) -> &Self {
        let content = format!(
            r#"#pragma once

#include <iostream>
#include <memory>
#include <string>

namespace {namespace} {{

struct {name}Config {{
    bool enabled = true;
    int timeout = 30;
}};

class {name}Service {{
public:
    explicit {name}Service(const {name}Config& config) : config_(config) {{}}

    void process() {{
        if (config_.enabled) {{
            doWork();
        }}
    }}

    bool validate() const {{
        return config_.timeout > 0;
    }}

private:
    void doWork() {{
        std::cout << "Working..." << std::endl;
    }}

    {name}Config config_;
}};

// Template function
template<typename T>
std::unique_ptr<T> create{name}(const {name}Config& config) {{
    return std::make_unique<T>(config);
}}

// Free function
inline void helper() {{
    std::cout << "helper" << std::endl;
}}

}} // namespace {namespace}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // BASH / SHELL
    // ========================================================================

    /// Add a Bash script with functions
    pub fn add_bash_script(&self, relative_path: &str, name: &str) -> &Self {
        let content = format!(
            r#"#!/bin/bash
# {name} script

set -euo pipefail

# Configuration
CONFIG_ENABLED=true
CONFIG_TIMEOUT=30

# Main function
{name}_main() {{
    echo "Starting {name}..."
    if [ "$CONFIG_ENABLED" = true ]; then
        {name}_process
    fi
}}

# Process function
{name}_process() {{
    echo "Processing..."
    _internal_helper
}}

# Internal helper (underscore prefix convention)
_internal_helper() {{
    echo "Internal helper"
}}

# Cleanup trap
cleanup() {{
    echo "Cleaning up..."
}}
trap cleanup EXIT

# Run if executed directly
if [[ "${{BASH_SOURCE[0]}}" == "${{0}}" ]]; then
    {name}_main "$@"
fi
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // TERRAFORM / HCL
    // ========================================================================

    /// Add a Terraform module
    pub fn add_terraform_module(
        &self,
        relative_path: &str,
        resource_type: &str,
        name: &str,
    ) -> &Self {
        let content = format!(
            r#"# {name} Terraform module

variable "{name}_enabled" {{
  description = "Enable {name}"
  type        = bool
  default     = true
}}

variable "{name}_config" {{
  description = "Configuration for {name}"
  type = object({{
    timeout = number
    retries = number
  }})
  default = {{
    timeout = 30
    retries = 3
  }}
}}

locals {{
  {name}_tags = {{
    Name        = "{name}"
    Environment = var.environment
  }}
}}

resource "{resource_type}" "{name}" {{
  count = var.{name}_enabled ? 1 : 0

  name = "{name}"

  tags = local.{name}_tags
}}

output "{name}_id" {{
  description = "ID of the {name} resource"
  value       = var.{name}_enabled ? {resource_type}.{name}[0].id : null
}}

output "{name}_arn" {{
  description = "ARN of the {name} resource"
  value       = var.{name}_enabled ? {resource_type}.{name}[0].arn : null
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // DOCKERFILE
    // ========================================================================

    /// Add a Dockerfile with multi-stage build
    pub fn add_dockerfile(&self, relative_path: &str, base_image: &str) -> &Self {
        let content = format!(
            r#"# Build stage
FROM {base_image} AS builder

WORKDIR /app

COPY package*.json ./
RUN npm ci --only=production

COPY . .
RUN npm run build

# Production stage
FROM {base_image}-slim AS production

WORKDIR /app

ENV NODE_ENV=production
ENV PORT=3000

COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules

EXPOSE $PORT

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:$PORT/health || exit 1

USER node

CMD ["node", "dist/index.js"]
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // GRADLE
    // ========================================================================

    /// Add a Gradle build file
    pub fn add_gradle_build(&self, relative_path: &str, group: &str, artifact: &str) -> &Self {
        let content = format!(
            r#"plugins {{
    id 'java'
    id 'application'
}}

group = '{group}'
version = '1.0.0'

repositories {{
    mavenCentral()
}}

dependencies {{
    implementation 'com.google.guava:guava:31.1-jre'
    testImplementation 'junit:junit:4.13.2'
}}

application {{
    mainClass = '{group}.{artifact}.Main'
}}

tasks.named('test') {{
    useJUnit()
}}

task customTask {{
    description = 'A custom task'
    doLast {{
        println 'Running custom task'
    }}
}}
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // CONFIG FILES (JSON, YAML, TOML, XML)
    // ========================================================================

    /// Add a JSON config file
    pub fn add_json_config(&self, relative_path: &str) -> &Self {
        let content = r#"{
    "name": "test-project",
    "version": "1.0.0",
    "config": {
        "enabled": true,
        "timeout": 30,
        "features": ["a", "b", "c"]
    },
    "dependencies": {
        "lodash": "^4.17.21"
    }
}
"#;
        self.add_file(relative_path, content)
    }

    /// Add a YAML config file
    pub fn add_yaml_config(&self, relative_path: &str) -> &Self {
        let content = r#"name: test-project
version: "1.0.0"

config:
  enabled: true
  timeout: 30
  features:
    - feature_a
    - feature_b
    - feature_c

services:
  api:
    port: 3000
    host: localhost
  database:
    host: db.example.com
    port: 5432
"#;
        self.add_file(relative_path, content)
    }

    /// Add a TOML config file
    pub fn add_toml_config(&self, relative_path: &str) -> &Self {
        let content = r#"[package]
name = "test-project"
version = "1.0.0"
edition = "2021"

[config]
enabled = true
timeout = 30
features = ["a", "b", "c"]

[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
"#;
        self.add_file(relative_path, content)
    }

    /// Add an XML config file
    pub fn add_xml_config(&self, relative_path: &str) -> &Self {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <name>test-project</name>
    <version>1.0.0</version>

    <config>
        <enabled>true</enabled>
        <timeout>30</timeout>
        <features>
            <feature>a</feature>
            <feature>b</feature>
            <feature>c</feature>
        </features>
    </config>

    <dependencies>
        <dependency>
            <groupId>com.example</groupId>
            <artifactId>library</artifactId>
            <version>1.0.0</version>
        </dependency>
    </dependencies>
</project>
"#;
        self.add_file(relative_path, content)
    }

    // ========================================================================
    // MARKUP FILES (HTML, CSS, SCSS, Markdown)
    // ========================================================================

    /// Add an HTML page
    pub fn add_html_page(&self, relative_path: &str, title: &str) -> &Self {
        let content = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <link rel="stylesheet" href="styles.css">
</head>
<body>
    <header>
        <nav>
            <a href="/">Home</a>
            <a href="/about">About</a>
        </nav>
    </header>

    <main>
        <h1>{title}</h1>
        <p>Welcome to the page.</p>
    </main>

    <footer>
        <p>&copy; 2024 Example</p>
    </footer>

    <script src="app.js"></script>
</body>
</html>
"#
        );
        self.add_file(relative_path, &content)
    }

    /// Add a CSS file
    pub fn add_css_styles(&self, relative_path: &str) -> &Self {
        let content = r#"/* Main styles */
:root {
    --primary-color: #007bff;
    --secondary-color: #6c757d;
}

body {
    font-family: system-ui, sans-serif;
    line-height: 1.6;
    margin: 0;
    padding: 0;
}

.container {
    max-width: 1200px;
    margin: 0 auto;
    padding: 1rem;
}

.button {
    background-color: var(--primary-color);
    color: white;
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
}

.button:hover {
    opacity: 0.9;
}

@media (max-width: 768px) {
    .container {
        padding: 0.5rem;
    }
}
"#;
        self.add_file(relative_path, content)
    }

    /// Add an SCSS file
    pub fn add_scss_styles(&self, relative_path: &str) -> &Self {
        let content = r#"// Variables
$primary-color: #007bff;
$secondary-color: #6c757d;
$breakpoint-mobile: 768px;

// Mixins
@mixin flex-center {
    display: flex;
    justify-content: center;
    align-items: center;
}

@mixin responsive($breakpoint) {
    @media (max-width: $breakpoint) {
        @content;
    }
}

// Base styles
body {
    font-family: system-ui, sans-serif;
    line-height: 1.6;
}

// Components
.button {
    background-color: $primary-color;
    color: white;
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 4px;

    &:hover {
        opacity: 0.9;
    }

    &--secondary {
        background-color: $secondary-color;
    }
}

.container {
    max-width: 1200px;
    margin: 0 auto;

    @include responsive($breakpoint-mobile) {
        padding: 0.5rem;
    }
}
"#;
        self.add_file(relative_path, content)
    }

    /// Add a Markdown file
    pub fn add_markdown(&self, relative_path: &str, title: &str) -> &Self {
        let content = format!(
            r#"# {title}

## Overview

This is a sample markdown document for testing.

## Features

- Feature 1
- Feature 2
- Feature 3

## Code Example

```typescript
function example(): void {{
    console.log('Hello, World!');
}}
```

## Table

| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| A        | B        | C        |
| D        | E        | F        |

## Links

- [Example Link](https://example.com)
- [Another Link](https://example.org)

---

*Last updated: 2024*
"#
        );
        self.add_file(relative_path, &content)
    }

    // ========================================================================
    // EDGE CASE HELPERS
    // ========================================================================

    /// Add an empty file
    pub fn add_empty_file(&self, relative_path: &str) -> &Self {
        self.add_file(relative_path, "")
    }

    /// Add a file with only whitespace
    pub fn add_whitespace_only(&self, relative_path: &str) -> &Self {
        self.add_file(relative_path, "   \n\n\t  \n   ")
    }

    /// Add a file with only comments
    pub fn add_comments_only_ts(&self, relative_path: &str) -> &Self {
        self.add_file(
            relative_path,
            r#"// This is a comment
/* This is a
   multi-line comment */
// Another comment
"#,
        )
    }

    /// Add a file with syntax errors
    pub fn add_syntax_error_ts(&self, relative_path: &str) -> &Self {
        self.add_file(
            relative_path,
            r#"export function broken( { this is not valid typescript
const x =
"#,
        )
    }

    /// Add a file with Unicode identifiers
    pub fn add_unicode_ts(&self, relative_path: &str) -> &Self {
        self.add_file(
            relative_path,
            r#"// Unicode identifiers
export const 你好 = "hello";
export const приветМир = "hello world";
export const Übermensch = "superman";
export const emoji🎉 = "party";

export function 処理する() {
    return 你好;
}
"#,
        )
    }

    /// Add a very long file
    pub fn add_very_long_file_ts(&self, relative_path: &str, lines: usize) -> &Self {
        let mut content = String::new();
        for i in 0..lines {
            content.push_str(&format!(
                "export function func{}(): number {{ return {}; }}\n",
                i, i
            ));
        }
        self.add_file(relative_path, &content)
    }

    /// Add a file with very long lines
    pub fn add_long_lines_ts(&self, relative_path: &str, line_length: usize) -> &Self {
        let long_string = "a".repeat(line_length);
        let content = format!(
            r#"export const longString = "{}";
export const anotherLong = "{}";
"#,
            long_string, long_string
        );
        self.add_file(relative_path, &content)
    }

    /// Add a deeply nested file
    pub fn add_deeply_nested_ts(&self, relative_path: &str, depth: usize) -> &Self {
        let mut content = String::from("export function deepNest() {\n");
        for _ in 0..depth {
            content.push_str("    if (true) {\n");
        }
        content.push_str("        console.log('deep');\n");
        for _ in 0..depth {
            content.push_str("    }\n");
        }
        content.push_str("}\n");
        self.add_file(relative_path, &content)
    }
}

impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}
