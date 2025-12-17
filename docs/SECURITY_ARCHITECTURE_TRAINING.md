# Semfora Security Architecture Training Guide


## Introduction

Welcome to the Semfora security architecture training. By the end of this session, you'll understand how our security vulnerability detection system works, from the moment a CVE is published to when it's detected in someone's code.

We have three main applications that work together: semfora-cve, semfora-engine, and semfora-ci. Think of them as a pipeline. semfora-cve is the data gatherer and pattern factory. semfora-engine is the scanner that actually finds vulnerabilities. And semfora-ci is the quality gate that runs in continuous integration pipelines.

Let's walk through how they connect.

---

## The Big Picture

Imagine you're building a system that can detect Log4Shell, or any known vulnerability, in code automatically. You need three things.

First, you need to know what vulnerabilities exist. That means pulling data from vulnerability databases like NIST's National Vulnerability Database.

Second, you need to understand what vulnerable code actually looks like. Not just the description, but the actual code patterns that make something vulnerable.

Third, you need to efficiently scan code and match it against those patterns.

That's exactly what our system does. Let me explain each part.

---

## How Advisory Data Flows In

Let's start with semfora-cve. This application is responsible for pulling in vulnerability advisories from multiple sources.

The primary source right now is NIST's National Vulnerability Database, which we call NVD. This is the authoritative source for CVE data. We use their REST API to fetch vulnerability records.

We also have connectors for GitHub Security Advisories, which uses GraphQL. That one is still being tested. There's MITRE, which maintains the official CVE list in a Git repository. CISA KEV, which tracks actively exploited vulnerabilities. And several regional databases like ENISA for Europe and JVN for Japan.

All of these sources implement a common interface called SourceFetcher. It has two main methods. One for incremental fetches, where you say "give me everything since yesterday." And one for full fetches, where you pull the entire history.

Each source also has a priority number. NVD has priority zero, meaning it's the most authoritative. If the same CVE appears in multiple sources, we use the highest priority data.

When you run "semfora-cve ingest now", the system iterates through all available sources, fetches new advisories, and converts them into a common format called RawAdvisory.

A RawAdvisory contains the CVE ID, a description, the CVSS severity score, a list of CWE identifiers which describe the type of weakness, affected packages, and reference URLs.

But here's the thing. This raw data isn't enough to detect the vulnerability in code. We need to transform it.

---

## The Magic of Claude Normalization

This is where things get interesting. We use Claude, specifically Claude Sonnet 4.5, to analyze each advisory and generate something special.

When Claude receives a raw advisory, we give it a detailed system prompt. The prompt asks Claude to extract structured information, things like which programming languages are affected, what CWE categories apply, and what the attack vector looks like.

But the critical part, and I want you to really understand this, is that Claude generates actual vulnerable code.

Not pseudocode. Not a description. Actual, syntactically valid code that demonstrates the vulnerability.

For example, if the CVE is about SQL injection in Python, Claude will generate a function like "get user" that takes a user ID, builds a query using string formatting, and executes it. The code includes a comment marking the vulnerable line.

Why does this matter? Because this generated code becomes the template for our fingerprints. We can analyze this code the same way we analyze any other code, and extract a unique signature.

If Claude isn't available, we fall back to heuristic rules. We can infer the language from the package ecosystem. NPM packages mean JavaScript. PyPI means Python. Maven means Java. And we can infer severity from the CVSS score. But without Claude, we can't generate the vulnerable code examples, which limits our pattern quality.

The output of normalization is a NormalizedCve object. It contains all the extracted metadata plus the vulnerable code and a confidence score between zero and one.

---

## Pattern Fingerprinting

Now we need to turn that normalized data into something we can efficiently match against.

This is where pattern fingerprinting comes in. A fingerprint is a compact numeric signature that represents the behavioral characteristics of code.

We use three types of fingerprints, all stored as 64-bit integers.

The first is the call fingerprint. This captures the sequence of function calls in the code. For SQL injection, you'd see patterns like "cursor.execute" or "db.query" with string concatenation.

The second is the control flow fingerprint. This captures the branching and looping structure. Things like if statements, try-catch blocks, and loops. We encode these as patterns like "ITT" meaning if, then try, then try.

The third is the state fingerprint. This captures what the code reads and writes. Database connections, file handles, user input.

We have two ways to generate these fingerprints.

The preferred method uses semfora-engine itself to analyze the code Claude generated. The engine extracts the semantic call graph, control flow, and state operations. This gives us the most accurate fingerprints.

The fallback method uses a simpler hash-based approach called FNV-1a. It parses the code looking for patterns and hashes them. It's less accurate but works without the engine binary.

Additionally, some high-profile vulnerabilities have manually curated patterns. Log4Shell and the React Server Components vulnerability have hand-crafted patterns maintained in dedicated files.

The result of fingerprinting is a CVEPattern object. It contains the CVE ID, CWE identifiers, the three 64-bit fingerprints, severity level, affected languages, and information about where the pattern came from.

---

## Compilation and Distribution

Once we have patterns, we need to package them for distribution. This is what the compiler does.

When you run "semfora-cve compile", the system loads all patterns from the SQLite database, merges in the manual patterns, creates a PatternDatabase object, and serializes it using bincode.

The output is a binary file, typically named "security patterns dot bin". We also generate a JSON sidecar file with metadata: the version number, SHA256 hash, pattern count, and generation timestamp.

The version number follows a date-based format: year, month, day, dot, sequence number. So the first release on January 15th, 2025 would be "2025 01 15 dot 1".

Now here's where distribution gets interesting.

For connected environments, semfora-engine can fetch pattern updates at runtime. It checks an environment variable called SEMFORA_PATTERN_URL, or falls back to a default pattern server. The update is atomic: we download the new database, validate it, and swap it in without restarting.

For air-gapped environments, you have two options.

Option one: embed the patterns at build time. You set an environment variable pointing to the binary file, build semfora-engine with a special feature flag, and the patterns get baked into the executable using Rust's include bytes macro. The result is a single binary that needs no external files.

Option two: runtime file loading. You copy the binary file to the air-gapped system and use an MCP tool to load it. This is useful when you want to update patterns without rebuilding the engine.

---

## How semfora-engine Uses the Patterns

Now let's talk about the scanning side.

When semfora-engine starts, it lazily loads the pattern database. Either from embedded bytes or from a previous runtime update. The database is cached in memory using a read-write lock.

The MCP server exposes a tool called "cve scan". When invoked, it performs a two-pass algorithm.

Pass one is the coarse filter. For each function in the codebase, we compute the Hamming distance between its fingerprints and each pattern's fingerprints. Hamming distance is just counting how many bits differ between two 64-bit numbers. It's extremely fast, just an XOR operation. If the distance is below a threshold, the pair moves to pass two.

Pass two is the fine filter. Here we compute Jaccard similarity, which measures the overlap between the actual sets of calls, control flow patterns, and state operations. This is more expensive but more accurate.

Functions that pass both filters become CVE matches. Each match includes the CVE ID, matched CWE categories, severity, similarity score between zero and one, the function that matched, and remediation guidance.

The results can be filtered by severity, by CWE category, or by module. They're sorted with critical vulnerabilities first.

---

## The CI Integration Story

Semfora-ci is designed to run in continuous integration pipelines. It analyzes pull requests and enforces quality gates.

Currently, semfora-ci wraps the semfora-engine CLI. It calls the engine with flags like "diff" to analyze changes between branches, "file symbols" to validate individual files, and "find duplicates" to detect code duplication.

The analyzer parses the engine's output and feeds it to a rule evaluator. Rules check things like complexity thresholds, nesting depth, risk levels, and duplicate code.

The reporter formats results for different outputs: plain text, JSON, or GitHub-flavored annotations for step summaries.

Here's an important gap in the current implementation. Semfora-ci doesn't yet integrate CVE scanning. The capability exists in semfora-engine, but semfora-ci doesn't call it. This would be a valuable addition: failing a build if new code matches known vulnerable patterns.

---

## Air-Gapped Deployment Strategy

Let me walk you through how all of this works for air-gapped clients.

In a connected environment, you'd run semfora-cve on a schedule, maybe daily. It fetches from NVD, normalizes with Claude, generates patterns, compiles the binary, and uploads to a pattern server. semfora-engine instances fetch updates automatically.

In an air-gapped environment, the workflow is different.

You maintain a connected build server that runs the ingestion pipeline. Weekly or monthly, you review the pattern quality, tag a release, and generate distribution artifacts. These include the binary pattern file, the metadata JSON, and pre-built semfora-engine executables for Linux, Mac, and Windows, all with patterns embedded.

You publish these to an internal mirror, GitHub releases, or an S3 bucket. Air-gapped clients pull them through whatever approved transfer mechanism they use, often called sneakernet.

The client runs the embedded binary or uses the MCP tool to load patterns from a local file. Either way, they get vulnerability scanning without any internet connectivity.

---

## Current Status and What's Next

Let me summarize what's working today and what's in progress.

The NVD source is fully working. You can fetch CVEs from NIST right now.

Claude normalization is working. Given raw advisories, Claude generates structured data and vulnerable code examples.

Pattern generation and compilation are working. You can create distributable binary artifacts.

The pattern loading and CVE scanning in semfora-engine are working. The MCP tools exist and function correctly.

What's not complete yet?

The GitHub Security Advisories source is implemented but needs testing. It requires a GitHub token and hasn't been validated end-to-end.

Several other sources like ENISA, JVN, and vendor advisories exist in code but haven't been thoroughly tested.

And semfora-ci is missing CVE scan integration. It calls the engine for other analysis but doesn't leverage vulnerability detection yet.

---

## Key Takeaways

Let me leave you with the essential points to remember.

One: Data flows from vulnerability databases through normalization to fingerprints to a compiled binary. Each step transforms raw data into something more useful for scanning.

Two: Claude's role is critical. It generates the vulnerable code examples that make high-quality fingerprints possible. Without Claude, we fall back to less precise heuristics.

Three: The two-pass algorithm makes scanning fast. Hamming distance for quick filtering, Jaccard similarity for accurate matching.

Four: Distribution is flexible. Embed at build time for air-gapped deployments, or update at runtime for connected environments.

Five: The system is designed to be extensible. Adding a new vulnerability source means implementing the SourceFetcher trait. The rest of the pipeline handles it automatically.

That's the Semfora security architecture. The code lives in three repositories: semfora-cve for ingestion and compilation, semfora-engine for scanning, and semfora-ci for quality gates. Now you understand how they connect.

---

## Appendix: File Locations Reference

For your reference, here are the key files mentioned in this training.

In semfora-cve:
- Sources are in src/sources, with individual files for NVD, GHSA, MITRE, and others
- AI normalization is in src/ai, including normalizer.rs and prompts.rs
- Pattern generation is in src/patterns
- The compiler is in src/compiler
- The CLI entry point is src/main.rs

In semfora-engine:
- Security module is in src/security
- Pattern loading is in src/security/patterns/embedded.rs
- CVE scanning MCP tools are in src/mcp_server/mod.rs

In semfora-ci:
- The analyzer that wraps the engine is in src/analyzer.rs
- Rule evaluation is in src/rules.rs
- The main CLI is in src/main.rs

