//! LSP Comparison Benchmarks
//!
//! Compares semfora-engine's semantic analysis against Language Server Protocol
//! implementations to measure:
//! - Latency: Time to extract symbols, find references
//! - Information richness: What semantic data is returned
//! - Token efficiency: Response size for AI context windows
//!
//! Run with: cargo bench --bench lsp_comparison
//!
//! Prerequisites:
//! - typescript-language-server: npm install -g typescript-language-server typescript
//! - rust-analyzer: rustup component add rust-analyzer

#![allow(unused_imports)]
#![allow(dead_code)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use lsp_types::{
    DocumentSymbolParams, InitializeParams, InitializeResult, PartialResultParams,
    TextDocumentIdentifier, Uri, WorkDoneProgressParams, WorkspaceSymbolParams,
};
use semfora_engine::cache::CacheDir;
use semfora_engine::lang::Lang;
use semfora_engine::schema::SemanticSummary;
use semfora_engine::socket_server::{index_directory, IndexOptions};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Parse and extract semantic summary from a file
fn analyze_file(file_path: &Path) -> Option<SemanticSummary> {
    let lang = Lang::from_path(file_path).ok()?;
    let source = std::fs::read_to_string(file_path).ok()?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;

    let tree = parser.parse(&source, None)?;
    semfora_engine::extract::extract(file_path, &source, &tree, lang).ok()
}

/// Simple LSP client for benchmarking
struct LspClient {
    process: Child,
    request_id: AtomicU64,
}

/// JSON-RPC request envelope
#[derive(Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: T,
}

/// JSON-RPC response envelope
#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    #[serde(default)]
    id: Option<serde_json::Value>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Read a single LSP message from the reader
fn read_lsp_message<R: BufRead>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut content_length = 0usize;

    // Read headers
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if line.to_lowercase().starts_with("content-length:") {
            content_length = line
                .split(':')
                .nth(1)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
        }
    }

    if content_length == 0 {
        return Err("No content-length header".to_string());
    }

    // Read body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(body)
}

impl LspClient {
    /// Spawn a new LSP server process
    fn spawn(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let process = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(Self {
            process,
            request_id: AtomicU64::new(1),
        })
    }

    /// Send an LSP request and wait for response
    fn request<P, R>(&mut self, method: &'static str, params: P) -> Result<R, String>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        let body = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        // Write request
        let stdin = self.process.stdin.as_mut().ok_or("No stdin")?;
        stdin
            .write_all(message.as_bytes())
            .map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;

        // Read responses, skipping notifications until we get our response
        let stdout = self.process.stdout.as_mut().ok_or("No stdout")?;
        let mut reader = BufReader::new(stdout);

        loop {
            let body = read_lsp_message(&mut reader)?;

            // Try to parse as a response with our id
            let json: serde_json::Value =
                serde_json::from_slice(&body).map_err(|e| e.to_string())?;

            // Check if this is a response to our request (has matching id)
            if let Some(response_id) = json.get("id") {
                if response_id.as_u64() == Some(id) {
                    // This is our response
                    let response: JsonRpcResponse<R> =
                        serde_json::from_value(json).map_err(|e| e.to_string())?;

                    if let Some(error) = response.error {
                        return Err(format!("LSP error {}: {}", error.code, error.message));
                    }

                    return response.result.ok_or_else(|| "No result".to_string());
                }
            }
            // Otherwise it's a notification or different response, skip it
        }
    }

    /// Initialize the LSP server with a workspace
    #[allow(deprecated)]
    fn initialize(&mut self, root_uri: &str) -> Result<InitializeResult, String> {
        let params = InitializeParams {
            root_uri: Some(Uri::from_str(root_uri).map_err(|e| format!("{:?}", e))?),
            ..Default::default()
        };
        self.request("initialize", params)
    }

    /// Send initialized notification
    fn initialized(&mut self) -> Result<(), String> {
        let stdin = self.process.stdin.as_mut().ok_or("No stdin")?;
        let body = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        stdin
            .write_all(message.as_bytes())
            .map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get document symbols (equivalent to semfora's analyze_file)
    fn document_symbols(&mut self, uri: &str) -> Result<Vec<lsp_types::DocumentSymbol>, String> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: Uri::from_str(uri).map_err(|e| format!("{:?}", e))?,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        // LSP can return either DocumentSymbol[] or SymbolInformation[]
        // For simplicity, we'll try DocumentSymbol first
        self.request::<_, Vec<lsp_types::DocumentSymbol>>("textDocument/documentSymbol", params)
    }

    /// Search workspace symbols (equivalent to semfora's search_symbols)
    fn workspace_symbols(
        &mut self,
        query: &str,
    ) -> Result<Vec<lsp_types::SymbolInformation>, String> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        self.request("workspace/symbol", params)
    }

    /// Shutdown the server gracefully
    fn shutdown(&mut self) -> Result<(), String> {
        let _ = self.request::<(), ()>("shutdown", ());
        let stdin = self.process.stdin.as_mut().ok_or("No stdin")?;
        let body = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = stdin.write_all(message.as_bytes());
        let _ = stdin.flush();
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
        let _ = self.process.kill();
    }
}

/// Benchmark results for comparison
#[derive(Debug, Default)]
struct BenchmarkResults {
    /// Average latency in milliseconds
    latency_ms: f64,
    /// Response size in bytes
    response_bytes: usize,
    /// Number of symbols/items returned
    item_count: usize,
    /// Semantic fields available
    semantic_fields: Vec<String>,
}

/// Get path to test repos directory
fn test_repos_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("/home/kadajett/Dev/semfora-test-repos/repos"),
        PathBuf::from("../semfora-test-repos/repos"),
        std::env::var("SEMFORA_TEST_REPOS")
            .map(PathBuf::from)
            .unwrap_or_default(),
    ];

    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    panic!("Test repos directory not found. Set SEMFORA_TEST_REPOS env var.");
}

/// Check if typescript-language-server is available
fn check_tsserver() -> bool {
    Command::new("typescript-language-server")
        .arg("--version")
        .output()
        .is_ok()
}

/// Set up a pre-indexed repo for semfora benchmarks
fn setup_semfora_index(repo_name: &str) -> Option<(CacheDir, PathBuf)> {
    let repos_dir = test_repos_dir();
    let repo_path = repos_dir.join(repo_name);

    if !repo_path.exists() {
        return None;
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let cache = CacheDir {
        root: temp_dir.path().to_path_buf(),
        repo_root: repo_path.clone(),
        repo_hash: format!("bench_{}", repo_name),
    };
    cache.init().unwrap();

    let options = IndexOptions::default();
    let _ = index_directory(&repo_path, cache.clone(), &options);

    std::mem::forget(temp_dir);
    Some((cache, repo_path))
}

/// Repos to benchmark (need to have TypeScript/JavaScript files)
const BENCH_REPOS: &[&str] = &["zod", "express-examples", "react-realworld"];

/// Compare document symbol extraction latency
fn bench_document_symbols_latency(c: &mut Criterion) {
    if !check_tsserver() {
        eprintln!("Skipping LSP benchmarks: typescript-language-server not installed");
        eprintln!("Install with: npm install -g typescript-language-server typescript");
        return;
    }

    let mut group = c.benchmark_group("document_symbols_latency");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(30));

    for repo_name in BENCH_REPOS {
        let repos_dir = test_repos_dir();
        let repo_path = repos_dir.join(repo_name);

        if !repo_path.exists() {
            eprintln!("Skipping {}: not found", repo_name);
            continue;
        }

        // Find a TypeScript/JavaScript file to analyze
        let test_file = find_ts_file(&repo_path);
        if test_file.is_none() {
            eprintln!("Skipping {}: no TS/JS files found", repo_name);
            continue;
        }
        let test_file = test_file.unwrap();
        let file_uri = format!("file://{}", test_file.display());

        // Benchmark LSP (typescript-language-server)
        group.bench_with_input(
            BenchmarkId::new(format!("{}/lsp", repo_name), repo_name),
            &file_uri,
            |b, file_uri| {
                // Initialize LSP server once
                let mut client =
                    LspClient::spawn("typescript-language-server", &["--stdio"]).unwrap();
                let root_uri = format!("file://{}", repo_path.display());
                client.initialize(&root_uri).unwrap();
                client.initialized().unwrap();

                // Wait for server to be ready
                std::thread::sleep(Duration::from_millis(500));

                b.iter(|| {
                    let _ = client.document_symbols(black_box(file_uri));
                });
            },
        );

        // Benchmark semfora
        let Some((_cache, _)) = setup_semfora_index(repo_name) else {
            continue;
        };

        group.bench_with_input(
            BenchmarkId::new(format!("{}/semfora", repo_name), repo_name),
            &test_file,
            |b, file_path| {
                b.iter(|| {
                    let _ = analyze_file(black_box(file_path));
                });
            },
        );
    }

    group.finish();
}

/// Compare workspace symbol search latency
fn bench_symbol_search_latency(c: &mut Criterion) {
    if !check_tsserver() {
        return;
    }

    let mut group = c.benchmark_group("symbol_search_latency");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(20));

    let search_queries = ["function", "handler", "error", "parse"];

    for repo_name in BENCH_REPOS {
        let repos_dir = test_repos_dir();
        let repo_path = repos_dir.join(repo_name);

        if !repo_path.exists() {
            continue;
        }

        // Benchmark LSP workspace/symbol
        for query in search_queries {
            let root_uri = format!("file://{}", repo_path.display());

            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}/lsp", repo_name, query), query),
                &root_uri,
                |b, root_uri| {
                    let mut client =
                        LspClient::spawn("typescript-language-server", &["--stdio"]).unwrap();
                    client.initialize(root_uri).unwrap();
                    client.initialized().unwrap();
                    std::thread::sleep(Duration::from_millis(500));

                    b.iter(|| {
                        let _ = client.workspace_symbols(black_box(query));
                    });
                },
            );
        }

        // Benchmark semfora search_symbols
        let Some((cache, _)) = setup_semfora_index(repo_name) else {
            continue;
        };

        for query in search_queries {
            let cache_clone = cache.clone();
            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}/semfora", repo_name, query), query),
                query,
                |b, query| {
                    b.iter(|| {
                        let _ = cache_clone.search_symbols(black_box(query), None, None, None, 20);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Compare information richness between LSP and semfora
fn bench_information_richness(c: &mut Criterion) {
    if !check_tsserver() {
        return;
    }

    let mut group = c.benchmark_group("information_richness");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    for repo_name in BENCH_REPOS.iter().take(1) {
        let repos_dir = test_repos_dir();
        let repo_path = repos_dir.join(repo_name);

        if !repo_path.exists() {
            continue;
        }

        let test_file = find_ts_file(&repo_path);
        if test_file.is_none() {
            continue;
        }
        let test_file = test_file.unwrap();

        // Get LSP response and measure
        let mut client = LspClient::spawn("typescript-language-server", &["--stdio"]).unwrap();
        let root_uri = format!("file://{}", repo_path.display());
        client.initialize(&root_uri).unwrap();
        client.initialized().unwrap();
        std::thread::sleep(Duration::from_millis(1000));

        let file_uri = format!("file://{}", test_file.display());
        let lsp_result = client.document_symbols(&file_uri);

        // Get semfora response
        let semfora_result = analyze_file(&test_file);

        // Print comparison (this isn't a timing benchmark, just information)
        group.bench_function(BenchmarkId::new(*repo_name, "compare"), |b| {
            b.iter(|| {
                // LSP fields available
                let lsp_fields = [
                    "name",
                    "kind",
                    "range",
                    "selectionRange",
                    "children",
                    "deprecated",
                    "detail",
                ];

                // Semfora fields available
                let semfora_fields = vec![
                    "name",
                    "kind",
                    "start_line",
                    "end_line",
                    "is_exported",
                    "return_type",
                    "arguments",
                    "calls",
                    "state_changes",
                    "control_flow",
                    "behavioral_risk",
                    "complexity",
                    "cognitive_complexity",
                    "dependencies",
                ];

                (lsp_fields.len(), semfora_fields.len())
            });
        });

        if let Ok(symbols) = lsp_result {
            eprintln!("\n=== {} Information Richness Comparison ===", repo_name);
            eprintln!("File: {}", test_file.display());
            eprintln!("\nLSP (typescript-language-server):");
            eprintln!("  Symbols found: {}", symbols.len());
            eprintln!("  Fields: name, kind, range, selectionRange, children, deprecated, detail");

            if let Some(summary) = semfora_result {
                eprintln!("\nSemfora:");
                eprintln!("  Symbols found: {}", summary.symbols.len());
                eprintln!("  Fields: name, kind, lines, is_exported, return_type, arguments,");
                eprintln!("          calls, state_changes, control_flow, behavioral_risk,");
                eprintln!("          complexity, cognitive_complexity, dependencies");
                eprintln!("\n  Additional data:");
                eprintln!(
                    "    - Dependencies: {} items",
                    summary.added_dependencies.len()
                );
                eprintln!("    - Calls: {} items", summary.calls.len());
                eprintln!("    - State changes: {} items", summary.state_changes.len());
            }
        }
    }

    group.finish();
}

/// Compare token efficiency (response size)
fn bench_token_efficiency(c: &mut Criterion) {
    if !check_tsserver() {
        return;
    }

    let mut group = c.benchmark_group("token_efficiency");
    group.sample_size(10);

    for repo_name in BENCH_REPOS.iter().take(1) {
        let repos_dir = test_repos_dir();
        let repo_path = repos_dir.join(repo_name);

        if !repo_path.exists() {
            continue;
        }

        let test_file = find_ts_file(&repo_path);
        if test_file.is_none() {
            continue;
        }
        let test_file = test_file.unwrap();

        // Get LSP response size
        let mut client = LspClient::spawn("typescript-language-server", &["--stdio"]).unwrap();
        let root_uri = format!("file://{}", repo_path.display());
        client.initialize(&root_uri).unwrap();
        client.initialized().unwrap();
        std::thread::sleep(Duration::from_millis(1000));

        let file_uri = format!("file://{}", test_file.display());

        group.bench_function(BenchmarkId::new(*repo_name, "measure_sizes"), |b| {
            b.iter(|| {
                // Get LSP JSON size
                let lsp_result = client.document_symbols(&file_uri);
                let lsp_json = serde_json::to_string(&lsp_result.ok()).unwrap_or_default();
                let lsp_bytes = lsp_json.len();

                // Get semfora JSON size
                let semfora_result = analyze_file(&test_file);
                let semfora_json = serde_json::to_string(&semfora_result).unwrap_or_default();
                let semfora_json_bytes = semfora_json.len();

                // Get semfora TOON size
                let toon_output = if let Some(ref summary) = semfora_result {
                    semfora_engine::toon::encode_toon(summary)
                } else {
                    String::new()
                };
                let toon_bytes = toon_output.len();

                (lsp_bytes, semfora_json_bytes, toon_bytes)
            });
        });

        // Print comparison
        let lsp_result = client.document_symbols(&file_uri);
        let lsp_json = serde_json::to_string(&lsp_result.ok()).unwrap_or_default();

        let semfora_result = analyze_file(&test_file);
        let semfora_json = serde_json::to_string(&semfora_result).unwrap_or_default();
        let toon_output = if let Some(ref summary) = semfora_result {
            semfora_engine::toon::encode_toon(summary)
        } else {
            String::new()
        };

        eprintln!("\n=== {} Token Efficiency Comparison ===", repo_name);
        eprintln!("File: {}", test_file.display());
        eprintln!("\nResponse sizes (bytes):");
        eprintln!("  LSP JSON:      {:>8} bytes", lsp_json.len());
        eprintln!("  Semfora JSON:  {:>8} bytes", semfora_json.len());
        eprintln!("  Semfora TOON:  {:>8} bytes", toon_output.len());
        eprintln!("\nToken efficiency (approx 4 chars/token):");
        eprintln!("  LSP tokens:      ~{}", lsp_json.len() / 4);
        eprintln!("  Semfora tokens:  ~{}", semfora_json.len() / 4);
        eprintln!("  TOON tokens:     ~{}", toon_output.len() / 4);

        if !lsp_json.is_empty() && !toon_output.is_empty() {
            let savings = 100.0 * (1.0 - (toon_output.len() as f64 / lsp_json.len() as f64));
            eprintln!("\nTOON vs LSP savings: {:.1}%", savings);
        }
    }

    group.finish();
}

/// Find a TypeScript or JavaScript file in a directory
fn find_ts_file(dir: &PathBuf) -> Option<PathBuf> {
    fn walk_dir(dir: &PathBuf, depth: usize) -> Option<PathBuf> {
        if depth > 5 {
            return None;
        }

        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str());
                if matches!(ext, Some("ts") | Some("tsx") | Some("js") | Some("jsx")) {
                    // Skip test files and node_modules
                    let path_str = path.to_string_lossy();
                    if !path_str.contains("node_modules")
                        && !path_str.contains(".test.")
                        && !path_str.contains(".spec.")
                    {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str());
                if !matches!(name, Some("node_modules") | Some(".git") | Some("dist")) {
                    if let Some(found) = walk_dir(&path, depth + 1) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }

    walk_dir(dir, 0)
}

criterion_group!(
    benches,
    bench_document_symbols_latency,
    bench_symbol_search_latency,
    bench_information_richness,
    bench_token_efficiency,
);
criterion_main!(benches);
