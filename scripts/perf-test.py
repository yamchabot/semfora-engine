#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Semfora Engine Performance Test Suite

Parallel performance testing with standardized JSON output.
Output format is compatible with Google Benchmark JSON for tooling interoperability.

Usage (with UV - recommended):
    uv run scripts/perf-test.py                    # Run all tests
    uv run scripts/perf-test.py --quick            # Quick smoke test
    uv run scripts/perf-test.py --self             # Benchmark semfora-engine itself
    uv run scripts/perf-test.py --indexing-only    # Just indexing benchmarks
    uv run scripts/perf-test.py --queries-only     # Just query benchmarks
    uv run scripts/perf-test.py --validation-only  # Just validation benchmarks
    uv run scripts/perf-test.py --report           # Generate HTML report
    uv run scripts/perf-test.py --compare          # Compare against previous run

Alternative (direct execution if UV is in PATH):
    ./scripts/perf-test.py --quick
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path
from typing import Optional
import statistics

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR = Path(__file__).parent.resolve()
PROJECT_DIR = SCRIPT_DIR.parent
HISTORY_DIR = PROJECT_DIR / "target" / "perf-history"
RESULTS_DIR = PROJECT_DIR / "target" / "perf-results"
REPOS_DIR = Path(os.environ.get("SEMFORA_TEST_REPOS", "/home/kadajett/Dev/semfora-test-repos/repos"))

ENGINE_BIN = PROJECT_DIR / "target" / "release" / "semfora-engine"
DAEMON_BIN = PROJECT_DIR / "target" / "release" / "semfora-daemon"

# Repo categories by expected size
SMALL_REPOS = ["nestjs-starter", "react-realworld", "angular-realworld", "sample-hugo"]
MEDIUM_REPOS = ["express-examples", "fastify-examples", "koa-examples", "zod", "routing-controllers"]
LARGE_REPOS = ["next.js", "typescript-eslint", "babel", "puppeteer", "playwright", "nextjs-examples"]

# Query patterns for benchmarking
SEARCH_PATTERNS = ["function", "export", "handler", "error", "async", "render", "parse", "interface"]

# ============================================================================
# Data Structures (Google Benchmark compatible)
# ============================================================================

@dataclass
class BenchmarkResult:
    """Single benchmark result - Google Benchmark compatible"""
    name: str
    real_time: float  # seconds
    cpu_time: float = 0.0  # seconds (optional)
    iterations: int = 1
    time_unit: str = "s"
    # Extended fields
    items_per_second: float = 0.0
    bytes_per_second: float = 0.0
    memory_peak_mb: float = 0.0
    error: Optional[str] = None
    metadata: dict = field(default_factory=dict)

@dataclass
class BenchmarkContext:
    """System context - Google Benchmark compatible"""
    date: str
    host_name: str
    executable: str
    num_cpus: int
    mhz_per_cpu: int = 0
    cpu_scaling_enabled: bool = False
    caches: list = field(default_factory=list)
    # Extended fields
    os: str = ""
    os_version: str = ""
    memory_gb: float = 0.0
    git_commit: str = ""
    rust_version: str = ""
    engine_version: str = ""

@dataclass
class BenchmarkReport:
    """Full report - Google Benchmark compatible structure"""
    context: BenchmarkContext
    benchmarks: list  # List of BenchmarkResult

    def to_dict(self):
        return {
            "context": asdict(self.context),
            "benchmarks": [asdict(b) for b in self.benchmarks]
        }

# ============================================================================
# Utilities
# ============================================================================

def get_system_context() -> BenchmarkContext:
    """Gather system information"""
    git_commit = ""
    try:
        git_commit = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=PROJECT_DIR, stderr=subprocess.DEVNULL
        ).decode().strip()
    except Exception:
        pass  # Git info is optional context

    rust_version = ""
    try:
        rust_version = subprocess.check_output(
            ["rustc", "--version"], stderr=subprocess.DEVNULL
        ).decode().strip()
    except Exception:
        pass  # Rust version is optional context

    engine_version = ""
    try:
        result = subprocess.run(
            [str(ENGINE_BIN), "--version"],
            capture_output=True, text=True, timeout=5
        )
        if result.returncode == 0:
            engine_version = result.stdout.strip()
    except Exception:
        pass  # Engine version is optional context

    memory_gb = 0.0
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemTotal:"):
                    memory_gb = int(line.split()[1]) / 1024 / 1024
                    break
    except Exception:
        pass  # Memory info unavailable on some platforms

    return BenchmarkContext(
        date=datetime.now().isoformat(),
        host_name=platform.node(),
        executable=str(ENGINE_BIN),
        num_cpus=os.cpu_count() or 1,
        os=platform.system(),
        os_version=platform.release(),
        memory_gb=round(memory_gb, 1),
        git_commit=git_commit,
        rust_version=rust_version,
        engine_version=engine_version,
    )

def find_repos(category: str = "all") -> list[tuple[str, Path]]:
    """Find available test repos"""
    if not REPOS_DIR.exists():
        return []

    if category == "small":
        names = SMALL_REPOS
    elif category == "medium":
        names = MEDIUM_REPOS
    elif category == "large":
        names = LARGE_REPOS
    else:
        names = SMALL_REPOS + MEDIUM_REPOS + LARGE_REPOS

    repos = []
    for name in names:
        path = REPOS_DIR / name
        if path.exists() and path.is_dir():
            repos.append((name, path))

    # Also add any other repos found
    if category == "all":
        known = set(SMALL_REPOS + MEDIUM_REPOS + LARGE_REPOS)
        for entry in REPOS_DIR.iterdir():
            if entry.is_dir() and entry.name not in known:
                repos.append((entry.name, entry))

    return repos

def count_source_files(path: Path) -> int:
    """Count source files in a directory"""
    extensions = {'.ts', '.tsx', '.js', '.jsx', '.rs', '.py', '.go', '.java', '.c', '.cpp', '.h', '.hpp'}
    count = 0
    try:
        for root, dirs, files in os.walk(path):
            # Skip common non-source directories
            dirs[:] = [d for d in dirs if d not in {'node_modules', '.git', 'target', '__pycache__', 'dist', 'build'}]
            for f in files:
                if Path(f).suffix.lower() in extensions:
                    count += 1
    except Exception:
        pass  # Return partial count on error
    return count

def get_index_size(path: Path) -> int:
    """Get the size of the .semfora_index directory in bytes"""
    index_path = path / ".semfora_index"
    if not index_path.exists():
        return 0
    total = 0
    try:
        for f in index_path.rglob("*"):
            if f.is_file():
                total += f.stat().st_size
    except Exception:
        pass  # Return partial size on error
    return total

def clear_cache(repo_path: Path) -> bool:
    """Clear cache for a repo using new CLI (operates on current dir)"""
    try:
        subprocess.run(
            [str(ENGINE_BIN), "cache", "clear"],
            capture_output=True, timeout=30, cwd=str(repo_path)
        )
        return True
    except Exception:
        return False

def run_with_timing(cmd: list[str], timeout: int = 300, cwd: Path = None) -> tuple[float, bool, str]:
    """Run command and return (duration_seconds, success, output)

    Args:
        cmd: Command to run
        timeout: Timeout in seconds
        cwd: Working directory to run command in (most commands operate on current dir)
    """
    start = time.perf_counter()
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(cwd) if cwd else None)
        duration = time.perf_counter() - start
        return duration, result.returncode == 0, result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return timeout, False, "Timeout"
    except Exception as e:
        return time.perf_counter() - start, False, str(e)

def get_memory_usage_mb() -> float:
    """Get current process memory usage in MB"""
    try:
        with open("/proc/self/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1]) / 1024
    except Exception:
        pass  # /proc unavailable on some platforms
    return 0.0

def print_progress(msg: str, color: str = "blue"):
    """Print colored progress message"""
    colors = {"blue": "\033[94m", "green": "\033[92m", "yellow": "\033[93m", "red": "\033[91m", "cyan": "\033[96m"}
    reset = "\033[0m"
    print(f"{colors.get(color, '')}{msg}{reset}")

# ============================================================================
# Benchmark Functions - Updated for new CLI
# ============================================================================

def benchmark_index_repo(args: tuple[str, Path, bool]) -> BenchmarkResult:
    """Index a single repo and measure performance using new CLI"""
    name, path, clear_first = args

    if clear_first:
        clear_cache(path)

    file_count = count_source_files(path)

    # Use new CLI: index generate <path>
    cmd = [str(ENGINE_BIN), "index", "generate", str(path), "--format", "toon"]
    duration, success, output = run_with_timing(cmd)

    # Get index size after generation
    index_size = get_index_size(path)

    return BenchmarkResult(
        name=f"indexing/{name}",
        real_time=duration,
        iterations=1,
        items_per_second=file_count / duration if duration > 0 else 0,
        bytes_per_second=index_size / duration if duration > 0 else 0,
        error=None if success else output[:200],
        metadata={
            "files": file_count,
            "repo": name,
            "index_size_kb": round(index_size / 1024, 1),
        }
    )

def benchmark_search(args: tuple[str, Path, str, str, int]) -> BenchmarkResult:
    """Run search query benchmark using new CLI"""
    repo_name, repo_path, pattern, mode, iterations = args

    times = []
    result_counts = []

    for _ in range(iterations):
        # Use new CLI: search <pattern> [--symbols|--related] (operates on current dir)
        # mode should be "symbols" or "semantic"
        mode_flag = "--symbols" if mode == "symbols" else "--related"
        cmd = [str(ENGINE_BIN), "search", pattern, mode_flag, "--limit", "20", "--format", "json"]
        duration, success, output = run_with_timing(cmd, timeout=30, cwd=repo_path)
        if success:
            times.append(duration)
            # Try to extract result count from JSON output
            try:
                data = json.loads(output)
                if isinstance(data, list):
                    result_counts.append(len(data))
                elif isinstance(data, dict) and "matches" in data:
                    result_counts.append(len(data["matches"]))
            except (json.JSONDecodeError, TypeError, KeyError):
                pass  # Result count is optional metadata

    if not times:
        return BenchmarkResult(
            name=f"search_{mode}/{repo_name}/{pattern}",
            real_time=0,
            error="All iterations failed"
        )

    return BenchmarkResult(
        name=f"search_{mode}/{repo_name}/{pattern}",
        real_time=statistics.mean(times),
        iterations=len(times),
        metadata={
            "pattern": pattern,
            "repo": repo_name,
            "mode": mode,
            "min": round(min(times), 4),
            "max": round(max(times), 4),
            "stddev": round(statistics.stdev(times), 4) if len(times) > 1 else 0,
            "avg_results": round(statistics.mean(result_counts), 1) if result_counts else 0,
        }
    )

def benchmark_get_overview(repo_name: str, repo_path: Path, iterations: int = 10) -> BenchmarkResult:
    """Benchmark get_overview operation using new CLI"""
    times = []
    token_counts = []

    for _ in range(iterations):
        # Use new CLI: query overview (operates on current dir)
        cmd = [str(ENGINE_BIN), "query", "overview", "--format", "toon"]
        duration, success, output = run_with_timing(cmd, timeout=30, cwd=repo_path)
        if success:
            times.append(duration)
            # Estimate token count (4 chars per token approximation)
            token_counts.append(len(output) // 4)

    if not times:
        return BenchmarkResult(name=f"overview/{repo_name}", real_time=0, error="Failed")

    return BenchmarkResult(
        name=f"overview/{repo_name}",
        real_time=statistics.mean(times),
        iterations=len(times),
        metadata={
            "min": round(min(times), 4),
            "max": round(max(times), 4),
            "avg_tokens": round(statistics.mean(token_counts), 0) if token_counts else 0,
        }
    )

def benchmark_get_call_graph(repo_name: str, repo_path: Path, iterations: int = 10) -> BenchmarkResult:
    """Benchmark call graph retrieval using new CLI"""
    times = []
    edge_counts = []

    for _ in range(iterations):
        # Use new CLI: query callgraph --stats-only (operates on current dir)
        cmd = [str(ENGINE_BIN), "query", "callgraph", "--format", "json", "--stats-only"]
        duration, success, output = run_with_timing(cmd, timeout=60, cwd=repo_path)
        if success:
            times.append(duration)
            try:
                data = json.loads(output)
                if "edge_count" in data:
                    edge_counts.append(data["edge_count"])
                elif "edges" in data:
                    edge_counts.append(len(data["edges"]))
            except (json.JSONDecodeError, TypeError, KeyError):
                pass  # Edge count is optional metadata

    if not times:
        return BenchmarkResult(name=f"call_graph/{repo_name}", real_time=0, error="Failed")

    return BenchmarkResult(
        name=f"call_graph/{repo_name}",
        real_time=statistics.mean(times),
        iterations=len(times),
        metadata={
            "min": round(min(times), 4),
            "max": round(max(times), 4),
            "avg_edges": round(statistics.mean(edge_counts), 0) if edge_counts else 0,
        }
    )

def benchmark_validation(repo_name: str, repo_path: Path, iterations: int = 5) -> list[BenchmarkResult]:
    """Benchmark validation operations (duplicates, complexity)"""
    results = []

    # Duplicate detection - use --duplicates flag (operates on current dir)
    times = []
    for _ in range(iterations):
        cmd = [str(ENGINE_BIN), "validate", "--duplicates", "--format", "json", "--limit", "50"]
        duration, success, output = run_with_timing(cmd, timeout=120, cwd=repo_path)
        if success:
            times.append(duration)

    if times:
        results.append(BenchmarkResult(
            name=f"validate_duplicates/{repo_name}",
            real_time=statistics.mean(times),
            iterations=len(times),
            metadata={"min": round(min(times), 4), "max": round(max(times), 4)}
        ))

    # Complexity validation - just validate without TARGET (operates on current dir)
    times = []
    for _ in range(iterations):
        cmd = [str(ENGINE_BIN), "validate", "--format", "json", "--limit", "100"]
        duration, success, output = run_with_timing(cmd, timeout=120, cwd=repo_path)
        if success:
            times.append(duration)

    if times:
        results.append(BenchmarkResult(
            name=f"validate_complexity/{repo_name}",
            real_time=statistics.mean(times),
            iterations=len(times),
            metadata={"min": round(min(times), 4), "max": round(max(times), 4)}
        ))

    return results

def benchmark_diff_analysis(repo_name: str, repo_path: Path, iterations: int = 5) -> BenchmarkResult:
    """Benchmark diff analysis operation"""
    times = []

    for _ in range(iterations):
        # Analyze diff against HEAD~1 (if available) - operates on current dir
        cmd = [str(ENGINE_BIN), "analyze", "--diff", "HEAD~1", "--format", "json"]
        duration, success, output = run_with_timing(cmd, timeout=60, cwd=repo_path)
        if success:
            times.append(duration)

    if not times:
        return BenchmarkResult(name=f"diff_analysis/{repo_name}", real_time=0, error="Failed or no commits")

    return BenchmarkResult(
        name=f"diff_analysis/{repo_name}",
        real_time=statistics.mean(times),
        iterations=len(times),
        metadata={"min": round(min(times), 4), "max": round(max(times), 4)}
    )

def benchmark_cache_performance(repo_name: str, repo_path: Path) -> list[BenchmarkResult]:
    """Benchmark cold vs warm cache performance"""
    results = []

    # Cold cache (clear first)
    clear_cache(repo_path)
    cmd = [str(ENGINE_BIN), "query", "overview", "--format", "toon"]
    cold_time, cold_success, _ = run_with_timing(cmd, timeout=60, cwd=repo_path)

    if cold_success:
        results.append(BenchmarkResult(
            name=f"cache_cold/{repo_name}",
            real_time=cold_time,
            iterations=1,
            metadata={"type": "cold_start"}
        ))

    # Warm cache (index should exist now)
    warm_times = []
    for _ in range(5):
        duration, success, _ = run_with_timing(cmd, timeout=30, cwd=repo_path)
        if success:
            warm_times.append(duration)

    if warm_times:
        results.append(BenchmarkResult(
            name=f"cache_warm/{repo_name}",
            real_time=statistics.mean(warm_times),
            iterations=len(warm_times),
            metadata={
                "type": "warm_cache",
                "speedup": round(cold_time / statistics.mean(warm_times), 2) if cold_time > 0 else 0,
            }
        ))

    return results

def benchmark_query_module(repo_name: str, repo_path: Path, iterations: int = 10) -> BenchmarkResult:
    """Benchmark module query operation"""
    times = []

    # First get module names from overview (operates on current dir)
    cmd = [str(ENGINE_BIN), "query", "overview", "--format", "json"]
    _, success, output = run_with_timing(cmd, timeout=30, cwd=repo_path)

    module_name = "src"  # default
    if success:
        try:
            data = json.loads(output)
            if "modules" in data and data["modules"]:
                module_name = data["modules"][0].get("name", "src")
        except (json.JSONDecodeError, TypeError, KeyError):
            pass  # Use default module name on parse error

    for _ in range(iterations):
        # query module <NAME> (operates on current dir)
        cmd = [str(ENGINE_BIN), "query", "module", module_name, "--format", "toon"]
        duration, success, _ = run_with_timing(cmd, timeout=30, cwd=repo_path)
        if success:
            times.append(duration)

    if not times:
        return BenchmarkResult(name=f"query_module/{repo_name}", real_time=0, error="Failed")

    return BenchmarkResult(
        name=f"query_module/{repo_name}",
        real_time=statistics.mean(times),
        iterations=len(times),
        metadata={"module": module_name, "min": round(min(times), 4), "max": round(max(times), 4)}
    )

# ============================================================================
# Test Suites
# ============================================================================

def run_indexing_benchmarks(repos: list[tuple[str, Path]], parallel: bool = True) -> list[BenchmarkResult]:
    """Run indexing benchmarks on repos"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  INDEXING BENCHMARKS")
    print_progress(f"  Repos: {len(repos)}, Parallel: {parallel}")
    print_progress(f"{'='*60}\n")

    # Phase 1: Sequential baseline
    print_progress("Phase 1: Sequential indexing (baseline)...", "yellow")
    seq_start = time.perf_counter()

    for name, path in repos:
        result = benchmark_index_repo((name, path, True))
        results.append(result)
        status = "✓" if not result.error else "✗"
        index_size = result.metadata.get('index_size_kb')
        size_str = f", {index_size:.0f}KB" if index_size is not None else ""
        print(f"  {status} {name}: {result.real_time:.2f}s ({result.items_per_second:.0f} files/s{size_str})")

    seq_total = time.perf_counter() - seq_start
    results.append(BenchmarkResult(
        name="indexing/sequential_total",
        real_time=seq_total,
        metadata={"repos": len(repos)}
    ))

    # Phase 2: Parallel indexing
    if parallel and len(repos) > 1:
        print_progress("\nPhase 2: Parallel indexing...", "yellow")

        # Clear all caches first
        for _, path in repos:
            clear_cache(path)

        par_start = time.perf_counter()

        with ProcessPoolExecutor(max_workers=min(len(repos), os.cpu_count() or 4)) as executor:
            futures = {
                executor.submit(benchmark_index_repo, (name, path, False)): name
                for name, path in repos
            }

            for future in as_completed(futures):
                name = futures[future]
                try:
                    result = future.result()
                    result.name = f"indexing_parallel/{result.metadata.get('repo', name)}"
                    results.append(result)
                    status = "✓" if not result.error else "✗"
                    print(f"  {status} {name}: {result.real_time:.2f}s")
                except Exception as e:
                    print(f"  ✗ {name}: {e}")

        par_total = time.perf_counter() - par_start
        speedup = seq_total / par_total if par_total > 0 else 1.0

        results.append(BenchmarkResult(
            name="indexing/parallel_total",
            real_time=par_total,
            metadata={"repos": len(repos), "speedup": round(speedup, 2)}
        ))

        print_progress(f"\n  Sequential: {seq_total:.2f}s", "green")
        print_progress(f"  Parallel:   {par_total:.2f}s", "green")
        print_progress(f"  Speedup:    {speedup:.2f}x", "green")

    return results

def run_query_benchmarks(repos: list[tuple[str, Path]], iterations: int = 5) -> list[BenchmarkResult]:
    """Run query benchmarks"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  QUERY BENCHMARKS")
    print_progress(f"  Repos: {len(repos)}, Iterations: {iterations}")
    print_progress(f"{'='*60}\n")

    # Ensure repos are indexed first
    print_progress("Ensuring indexes exist...", "yellow")
    for name, path in repos:
        cmd = [str(ENGINE_BIN), "index", "generate", str(path), "--format", "toon"]
        subprocess.run(cmd, capture_output=True)

    # Build list of all search benchmarks to run in parallel
    search_tasks = []
    for name, path in repos:
        for pattern in SEARCH_PATTERNS[:4]:  # Use subset for speed
            # Test both symbol and semantic search modes
            search_tasks.append((name, path, pattern, "symbols", iterations))
            search_tasks.append((name, path, pattern, "semantic", iterations))

    print_progress(f"Running {len(search_tasks)} search benchmarks in parallel...", "yellow")

    with ThreadPoolExecutor(max_workers=8) as executor:
        futures = [executor.submit(benchmark_search, task) for task in search_tasks]

        for future in as_completed(futures):
            try:
                result = future.result()
                results.append(result)
                if result.error:
                    print(f"  ✗ {result.name}: {result.error}")
                else:
                    print(f"  ✓ {result.name}: {result.real_time*1000:.1f}ms")
            except Exception as e:
                print(f"  ✗ Error: {e}")

    # Overview, call graph, and module benchmarks
    print_progress("\nRunning overview/callgraph/module benchmarks...", "yellow")
    for name, path in repos[:3]:  # Limit to 3 repos
        result = benchmark_get_overview(name, path, iterations)
        results.append(result)
        tokens = result.metadata.get('avg_tokens', 0)
        print(f"  ✓ overview/{name}: {result.real_time*1000:.1f}ms (~{tokens:.0f} tokens)")

        result = benchmark_get_call_graph(name, path, iterations)
        results.append(result)
        edges = result.metadata.get('avg_edges', 0)
        print(f"  ✓ call_graph/{name}: {result.real_time*1000:.1f}ms (~{edges:.0f} edges)")

        result = benchmark_query_module(name, path, iterations)
        results.append(result)
        print(f"  ✓ query_module/{name}: {result.real_time*1000:.1f}ms")

    return results

def run_validation_benchmarks(repos: list[tuple[str, Path]], iterations: int = 3) -> list[BenchmarkResult]:
    """Run validation benchmarks (duplicates, complexity)"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  VALIDATION BENCHMARKS")
    print_progress(f"  Repos: {len(repos)}, Iterations: {iterations}")
    print_progress(f"{'='*60}\n")

    for name, path in repos[:3]:  # Limit to 3 repos
        print_progress(f"Validating {name}...", "yellow")

        validation_results = benchmark_validation(name, path, iterations)
        for result in validation_results:
            results.append(result)
            if result.error:
                print(f"  ✗ {result.name}: {result.error}")
            else:
                print(f"  ✓ {result.name}: {result.real_time:.2f}s")

        # Diff analysis
        result = benchmark_diff_analysis(name, path, iterations)
        results.append(result)
        if result.error:
            print(f"  ✗ {result.name}: {result.error}")
        else:
            print(f"  ✓ {result.name}: {result.real_time*1000:.1f}ms")

    return results

def run_cache_benchmarks(repos: list[tuple[str, Path]]) -> list[BenchmarkResult]:
    """Run cache performance benchmarks"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  CACHE PERFORMANCE BENCHMARKS")
    print_progress(f"  Repos: {len(repos)}")
    print_progress(f"{'='*60}\n")

    for name, path in repos[:3]:  # Limit to 3 repos
        print_progress(f"Testing cache for {name}...", "yellow")

        cache_results = benchmark_cache_performance(name, path)
        for result in cache_results:
            results.append(result)
            speedup = result.metadata.get('speedup', '')
            speedup_str = f" ({speedup}x speedup)" if speedup else ""
            print(f"  ✓ {result.name}: {result.real_time:.3f}s{speedup_str}")

    return results

def run_stress_test(repos: list[tuple[str, Path]], num_queries: int = 100) -> list[BenchmarkResult]:
    """Run concurrent query stress test"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  STRESS TEST")
    print_progress(f"  Repos: {len(repos)}, Queries: {num_queries}")
    print_progress(f"{'='*60}\n")

    # Ensure indexes exist
    for _, path in repos:
        subprocess.run([str(ENGINE_BIN), "index", "generate", str(path), "--format", "toon"], capture_output=True)

    print_progress(f"Running {num_queries} concurrent queries...", "yellow")

    import random

    def random_query(_):
        name, path = random.choice(repos)
        pattern = random.choice(SEARCH_PATTERNS)
        flag = random.choice(["--symbols", "--related"])
        cmd = [str(ENGINE_BIN), "search", pattern, flag, "--limit", "20"]
        start = time.perf_counter()
        try:
            subprocess.run(cmd, capture_output=True, timeout=30, cwd=path)
            return time.perf_counter() - start, True
        except Exception:
            return time.perf_counter() - start, False

    start_time = time.perf_counter()

    with ThreadPoolExecutor(max_workers=16) as executor:
        query_results = list(executor.map(random_query, range(num_queries)))

    total_time = time.perf_counter() - start_time

    successful = [t for t, ok in query_results if ok]
    failed = num_queries - len(successful)

    qps = num_queries / total_time if total_time > 0 else 0
    avg_latency = statistics.mean(successful) if successful else 0

    print_progress(f"\n  Total time:    {total_time:.2f}s", "green")
    print_progress(f"  Queries/sec:   {qps:.1f}", "green")
    print_progress(f"  Avg latency:   {avg_latency*1000:.1f}ms", "green")
    print_progress(f"  Success rate:  {len(successful)}/{num_queries}", "green")

    results.append(BenchmarkResult(
        name="stress/concurrent_queries",
        real_time=total_time,
        iterations=num_queries,
        items_per_second=qps,
        metadata={
            "avg_latency_ms": round(avg_latency * 1000, 2),
            "min_latency_ms": round(min(successful) * 1000, 2) if successful else 0,
            "max_latency_ms": round(max(successful) * 1000, 2) if successful else 0,
            "p95_latency_ms": round(sorted(successful)[int(len(successful) * 0.95)] * 1000, 2) if len(successful) >= 1 else 0,
            "success_count": len(successful),
            "failure_count": failed
        }
    ))

    return results

def run_self_benchmark() -> list[BenchmarkResult]:
    """Benchmark semfora-engine on its own codebase"""
    results = []

    print_progress(f"\n{'='*60}")
    print_progress("  SELF-BENCHMARK (semfora-engine codebase)")
    print_progress(f"{'='*60}\n")

    repo_name = "semfora-engine"
    repo_path = PROJECT_DIR

    # Clear cache first
    clear_cache(repo_path)

    # Index benchmark
    print_progress("Indexing...", "yellow")
    result = benchmark_index_repo((repo_name, repo_path, True))
    results.append(result)
    print(f"  ✓ Index: {result.real_time:.2f}s ({result.items_per_second:.0f} files/s, {result.metadata.get('index_size_kb', 0):.0f}KB)")

    # Overview
    print_progress("Overview query...", "yellow")
    result = benchmark_get_overview(repo_name, repo_path, 10)
    results.append(result)
    print(f"  ✓ Overview: {result.real_time*1000:.1f}ms (~{result.metadata.get('avg_tokens', 0):.0f} tokens)")

    # Call graph
    print_progress("Call graph query...", "yellow")
    result = benchmark_get_call_graph(repo_name, repo_path, 10)
    results.append(result)
    print(f"  ✓ Call graph: {result.real_time*1000:.1f}ms (~{result.metadata.get('avg_edges', 0):.0f} edges)")

    # Search benchmarks
    print_progress("Search benchmarks...", "yellow")
    for pattern in ["function", "struct", "impl", "async"]:
        for mode in ["symbols", "semantic"]:
            result = benchmark_search((repo_name, repo_path, pattern, mode, 10))
            results.append(result)
            print(f"  ✓ search_{mode}/{pattern}: {result.real_time*1000:.1f}ms")

    # Validation
    print_progress("Validation benchmarks...", "yellow")
    validation_results = benchmark_validation(repo_name, repo_path, 3)
    for result in validation_results:
        results.append(result)
        print(f"  ✓ {result.name}: {result.real_time:.2f}s")

    # Cache performance
    print_progress("Cache performance...", "yellow")
    cache_results = benchmark_cache_performance(repo_name, repo_path)
    for result in cache_results:
        results.append(result)
        speedup = result.metadata.get('speedup', '')
        speedup_str = f" ({speedup}x speedup)" if speedup else ""
        print(f"  ✓ {result.name}: {result.real_time:.3f}s{speedup_str}")

    return results

# ============================================================================
# Report Generation & Comparison
# ============================================================================

def compare_reports(current: BenchmarkReport, baseline: BenchmarkReport) -> str:
    """Compare current results against baseline"""
    output = []
    output.append(f"\n{'='*60}")
    output.append("  COMPARISON vs BASELINE")
    output.append(f"  Current:  {current.context.date[:19]} ({current.context.git_commit})")
    output.append(f"  Baseline: {baseline.context.date[:19]} ({baseline.context.git_commit})")
    output.append(f"{'='*60}\n")

    # Build lookup for baseline
    baseline_lookup = {b.name: b for b in baseline.benchmarks}

    regressions = []
    improvements = []
    unchanged = []

    for curr in current.benchmarks:
        if curr.name not in baseline_lookup:
            continue

        base = baseline_lookup[curr.name]
        if base.real_time == 0 or curr.real_time == 0:
            continue

        change_pct = ((curr.real_time - base.real_time) / base.real_time) * 100
        abs_change = abs(curr.real_time - base.real_time)

        # Require both >10% change AND >10ms absolute change to avoid false positives
        # on very fast operations where tiny differences appear as large percentages
        if change_pct > 10 and abs_change > 0.01:  # More than 10% slower AND >10ms
            regressions.append((curr.name, base.real_time, curr.real_time, change_pct))
        elif change_pct < -10 and abs_change > 0.01:  # More than 10% faster AND >10ms
            improvements.append((curr.name, base.real_time, curr.real_time, change_pct))
        else:
            unchanged.append((curr.name, base.real_time, curr.real_time, change_pct))

    if regressions:
        output.append("REGRESSIONS (>10% slower):")
        for name, base_t, curr_t, pct in sorted(regressions, key=lambda x: x[3], reverse=True):
            output.append(f"  ❌ {name}: {base_t:.3f}s → {curr_t:.3f}s (+{pct:.1f}%)")
        output.append("")

    if improvements:
        output.append("IMPROVEMENTS (>10% faster):")
        for name, base_t, curr_t, pct in sorted(improvements, key=lambda x: x[3]):
            output.append(f"  ✅ {name}: {base_t:.3f}s → {curr_t:.3f}s ({pct:.1f}%)")
        output.append("")

    output.append(f"Summary: {len(regressions)} regressions, {len(improvements)} improvements, {len(unchanged)} unchanged")

    return "\n".join(output)

def generate_html_report(report: BenchmarkReport, output_path: Path, comparison_text: str = ""):
    """Generate HTML report with charts"""

    # Group benchmarks by category
    categories = {}
    for b in report.benchmarks:
        cat = b.name.split("/")[0]
        if cat not in categories:
            categories[cat] = []
        categories[cat].append(b)

    html = f"""<!DOCTYPE html>
<html>
<head>
    <title>Semfora Performance Report - {report.context.date[:10]}</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 40px; background: #f5f5f5; }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        h1 {{ color: #333; }}
        .card {{ background: white; border-radius: 8px; padding: 20px; margin: 20px 0; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .context {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 10px; }}
        .context-item {{ background: #f8f9fa; padding: 10px; border-radius: 4px; }}
        .context-label {{ font-size: 12px; color: #666; }}
        .context-value {{ font-weight: bold; color: #333; }}
        table {{ width: 100%; border-collapse: collapse; }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f8f9fa; font-weight: 600; }}
        .chart-container {{ height: 300px; margin: 20px 0; }}
        .success {{ color: #28a745; }}
        .error {{ color: #dc3545; }}
        .metric {{ font-size: 24px; font-weight: bold; color: #007bff; }}
        pre {{ background: #f8f9fa; padding: 15px; border-radius: 4px; overflow-x: auto; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Semfora Performance Report</h1>

        <div class="card">
            <h2>System Context</h2>
            <div class="context">
                <div class="context-item">
                    <div class="context-label">Date</div>
                    <div class="context-value">{report.context.date[:19]}</div>
                </div>
                <div class="context-item">
                    <div class="context-label">Host</div>
                    <div class="context-value">{report.context.host_name}</div>
                </div>
                <div class="context-item">
                    <div class="context-label">CPUs</div>
                    <div class="context-value">{report.context.num_cpus}</div>
                </div>
                <div class="context-item">
                    <div class="context-label">Memory</div>
                    <div class="context-value">{report.context.memory_gb} GB</div>
                </div>
                <div class="context-item">
                    <div class="context-label">Git Commit</div>
                    <div class="context-value">{report.context.git_commit or 'N/A'}</div>
                </div>
                <div class="context-item">
                    <div class="context-label">Rust</div>
                    <div class="context-value">{report.context.rust_version.split()[1] if report.context.rust_version else 'N/A'}</div>
                </div>
                <div class="context-item">
                    <div class="context-label">Engine</div>
                    <div class="context-value">{report.context.engine_version or 'N/A'}</div>
                </div>
            </div>
        </div>
"""

    if comparison_text:
        html += f"""
        <div class="card">
            <h2>Comparison vs Baseline</h2>
            <pre>{comparison_text}</pre>
        </div>
"""

    # Add sections for each category
    for cat, benchmarks in categories.items():
        html += f"""
        <div class="card">
            <h2>{cat.replace('_', ' ').title()} Benchmarks</h2>
            <table>
                <tr>
                    <th>Name</th>
                    <th>Time</th>
                    <th>Iterations</th>
                    <th>Throughput</th>
                    <th>Status</th>
                </tr>
"""
        for b in benchmarks:
            time_str = f"{b.real_time:.3f}s" if b.real_time >= 1 else f"{b.real_time*1000:.1f}ms"
            throughput = f"{b.items_per_second:.1f}/s" if b.items_per_second > 0 else "-"
            status = '<span class="error">✗</span>' if b.error else '<span class="success">✓</span>'

            html += f"""
                <tr>
                    <td>{b.name}</td>
                    <td>{time_str}</td>
                    <td>{b.iterations}</td>
                    <td>{throughput}</td>
                    <td>{status}</td>
                </tr>
"""
        html += """
            </table>
        </div>
"""

    html += """
    </div>
</body>
</html>
"""

    output_path.write_text(html)
    print_progress(f"\nHTML report: {output_path}", "green")

# ============================================================================
# Main
# ============================================================================

def build_release():
    """Build release binaries"""
    print_progress("Building release binaries...", "yellow")
    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=PROJECT_DIR,
        capture_output=True
    )
    if result.returncode != 0:
        print_progress("Build failed!", "red")
        print(result.stderr.decode())
        sys.exit(1)
    print_progress("Build complete.", "green")

def load_baseline() -> Optional[BenchmarkReport]:
    """Load the most recent benchmark result as baseline"""
    json_files = sorted(HISTORY_DIR.glob("perf_*.json"), reverse=True)
    if not json_files:
        return None

    try:
        with open(json_files[0]) as f:
            data = json.load(f)
        context = BenchmarkContext(**data["context"])
        benchmarks = [BenchmarkResult(**b) for b in data["benchmarks"]]
        return BenchmarkReport(context=context, benchmarks=benchmarks)
    except Exception:
        return None  # Baseline is optional, ignore corrupt files

def main():
    parser = argparse.ArgumentParser(description="Semfora Performance Test Suite")
    parser.add_argument("--quick", action="store_true", help="Quick smoke test (small repos only)")
    parser.add_argument("--self", action="store_true", help="Benchmark semfora-engine codebase itself")
    parser.add_argument("--indexing-only", action="store_true", help="Run only indexing benchmarks")
    parser.add_argument("--queries-only", action="store_true", help="Run only query benchmarks")
    parser.add_argument("--validation-only", action="store_true", help="Run only validation benchmarks")
    parser.add_argument("--cache-only", action="store_true", help="Run only cache performance benchmarks")
    parser.add_argument("--stress-only", action="store_true", help="Run only stress test")
    parser.add_argument("--no-build", action="store_true", help="Skip cargo build")
    parser.add_argument("--report", action="store_true", help="Generate HTML report from latest results")
    parser.add_argument("--compare", action="store_true", help="Compare against previous run")
    parser.add_argument("--output", type=str, help="Output JSON file path")
    args = parser.parse_args()

    # Setup directories
    HISTORY_DIR.mkdir(parents=True, exist_ok=True)
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")

    if args.report:
        # Find latest results and generate report
        json_files = sorted(HISTORY_DIR.glob("perf_*.json"), reverse=True)
        if not json_files:
            json_files = sorted(RESULTS_DIR.glob("perf_*.json"), reverse=True)
        if not json_files:
            print_progress("No results found to generate report from", "red")
            sys.exit(1)

        with open(json_files[0]) as f:
            data = json.load(f)

        context = BenchmarkContext(**data["context"])
        benchmarks = [BenchmarkResult(**b) for b in data["benchmarks"]]
        report = BenchmarkReport(context=context, benchmarks=benchmarks)

        html_path = RESULTS_DIR / f"report_{timestamp}.html"
        generate_html_report(report, html_path)
        return

    # Build
    if not args.no_build:
        build_release()

    if not ENGINE_BIN.exists():
        print_progress(f"Engine binary not found: {ENGINE_BIN}", "red")
        sys.exit(1)

    # Load baseline for comparison if requested
    baseline = load_baseline() if args.compare else None

    # Find repos or use self-benchmark
    if args.self:
        repos = []  # Will use self-benchmark
    elif args.quick:
        repos = find_repos("small")[:3]
    else:
        repos = find_repos("all")

    if not repos and not args.self:
        print_progress(f"No test repos found in {REPOS_DIR}", "yellow")
        print_progress("Falling back to self-benchmark mode...", "yellow")
        args.self = True

    print_progress(f"\n{'='*60}")
    print_progress("  SEMFORA PERFORMANCE TEST SUITE")
    print_progress(f"  Timestamp: {timestamp}")
    if args.self:
        print_progress("  Mode: Self-benchmark (semfora-engine codebase)")
    else:
        print_progress(f"  Repos: {len(repos)}")
    print_progress(f"{'='*60}")

    # Gather context
    context = get_system_context()
    all_results = []

    # Run benchmarks
    if args.self:
        all_results.extend(run_self_benchmark())
    elif args.indexing_only:
        all_results.extend(run_indexing_benchmarks(repos))
    elif args.queries_only:
        all_results.extend(run_query_benchmarks(repos[:5]))
    elif args.validation_only:
        all_results.extend(run_validation_benchmarks(repos[:3]))
    elif args.cache_only:
        all_results.extend(run_cache_benchmarks(repos[:3]))
    elif args.stress_only:
        all_results.extend(run_stress_test(repos[:5]))
    else:
        # Run all
        all_results.extend(run_indexing_benchmarks(repos))
        all_results.extend(run_query_benchmarks(repos[:5]))
        all_results.extend(run_validation_benchmarks(repos[:3]))
        all_results.extend(run_cache_benchmarks(repos[:3]))
        all_results.extend(run_stress_test(repos[:5]))

    # Create report
    report = BenchmarkReport(context=context, benchmarks=all_results)

    # Compare with baseline
    comparison_text = ""
    if baseline:
        comparison_text = compare_reports(report, baseline)
        print(comparison_text)

    # Save JSON to history
    history_path = HISTORY_DIR / f"perf_{timestamp}.json"
    with open(history_path, "w") as f:
        json.dump(report.to_dict(), f, indent=2)

    # Also save to custom output if specified
    if args.output:
        output_path = Path(args.output)
        with open(output_path, "w") as f:
            json.dump(report.to_dict(), f, indent=2)

    print_progress(f"\n{'='*60}")
    print_progress("  COMPLETE")
    print_progress(f"  JSON: {history_path}")
    print_progress(f"{'='*60}")

    # Generate HTML report
    html_path = RESULTS_DIR / f"report_{timestamp}.html"
    generate_html_report(report, html_path, comparison_text)

if __name__ == "__main__":
    main()
