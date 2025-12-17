# Semfora Security Architecture Audit

## System Architecture Graph

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                           SEMFORA SECURITY ECOSYSTEM                                 │
└─────────────────────────────────────────────────────────────────────────────────────┘

                         ┌─────────────────────────────────────┐
                         │       ADVISORY SOURCES              │
                         │  (semfora-cve/src/sources/)         │
                         └─────────────────────────────────────┘
                                        │
    ┌───────────┬───────────┬───────────┼───────────┬───────────┬───────────┐
    ▼           ▼           ▼           ▼           ▼           ▼           ▼
┌───────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐
│  NVD  │  │  GHSA  │  │ MITRE  │  │CISA KEV│  │ ENISA  │  │  JVN   │  │ Vendor │
│(NIST) │  │(GitHub)│  │(CVE.org│  │(Active │  │ (EU)   │  │(Japan) │  │(RedHat,│
│ ✅    │  │ 🚧    │  │  Git)  │  │Exploit)│  │        │  │        │  │  MSRC) │
└───┬───┘  └───┬────┘  └───┬────┘  └───┬────┘  └───┬────┘  └───┬────┘  └───┬────┘
    │          │           │           │           │           │           │
    └──────────┴───────────┴───────────┼───────────┴───────────┴───────────┘
                                       ▼
                         ┌─────────────────────────────────────┐
                         │       SourceFetcher Trait           │
                         │  fetch_incremental(since) → Vec<RawAdvisory>
                         │  fetch_full() → Vec<RawAdvisory>    │
                         └──────────────────┬──────────────────┘
                                            ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              INGESTION PIPELINE                                      │
│                         (semfora-cve ingest --now)                                   │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                            │
                                            ▼
                         ┌─────────────────────────────────────┐
                         │          RawAdvisory                │
                         │  • cve_id, description              │
                         │  • cvss_v3_score, severity          │
                         │  • cwe_ids[], affected[]            │
                         │  • references[]                     │
                         └──────────────────┬──────────────────┘
                                            │
                    ┌───────────────────────┼───────────────────────┐
                    ▼                       ▼                       ▼
           ┌────────────────┐    ┌────────────────────┐   ┌────────────────┐
           │ AI Normalizer  │    │ Fallback Normalize │   │  StateStore    │
           │ (Claude API)   │    │ (heuristic rules)  │   │  (SQLite)      │
           │ claude-sonnet  │    │                    │   │                │
           │ -4-5-20250929  │    │ ecosystem→language │   │ sync_state     │
           └───────┬────────┘    │ severity→cvss     │   │ processed_cves │
                   │             └────────────────────┘   │ normalized_cves│
                   │                                      │ patterns       │
                   ▼                                      │ artifacts      │
    ┌──────────────────────────────┐                     └────────────────┘
    │   Claude Normalization       │
    │   (src/ai/prompts.rs)        │
    │                              │
    │  SYSTEM_PROMPT instructs:    │
    │  • Extract cwe_ids[]         │
    │  • Infer languages[]         │
    │  • Identify frameworks[]     │
    │  • Describe attack_vector    │
    │  • List vulnerable_code_patterns │
    │  • **GENERATE vulnerable_code**  │ ◄── Critical for fingerprinting
    │  • Provide remediation       │
    │  • Set confidence (0.0-1.0)  │
    └───────────────┬──────────────┘
                    ▼
           ┌────────────────────┐
           │   NormalizedCve    │
           │  • cve_id          │
           │  • cwe_ids[]       │
           │  • languages[]     │
           │  • vulnerable_code │ ◄── Actual code for analysis
           │  • vulnerable_code_patterns │
           │  • confidence      │
           └─────────┬──────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                           PATTERN GENERATION                                         │
│                    (semfora-cve/src/patterns/)                                       │
└─────────────────────────────────────────────────────────────────────────────────────┘
                     │
       ┌─────────────┼─────────────┐
       ▼             ▼             ▼
┌─────────────┐ ┌──────────────┐ ┌─────────────────┐
│ Engine      │ │ Fingerprinter│ │ Manual Patterns │
│ Fingerprint │ │ (fallback)   │ │ (curated)       │
│             │ │              │ │                 │
│ Uses actual │ │ FNV-1a hash  │ │ react_rsc.rs    │
│ semfora-    │ │ of:          │ │ log4shell.rs    │
│ engine to   │ │ • call seqs  │ │ (spring4shell)  │
│ analyze     │ │ • ctrl flow  │ │                 │
│ code        │ │ • state ops  │ │                 │
└──────┬──────┘ └──────┬───────┘ └────────┬────────┘
       │               │                  │
       └───────────────┼──────────────────┘
                       ▼
              ┌────────────────────┐
              │    CVEPattern      │
              │ • cve_id           │
              │ • cwe_ids[]        │
              │ • call_fingerprint │  ◄── 64-bit hash
              │ • control_flow_fp  │  ◄── 64-bit hash
              │ • state_fingerprint│  ◄── 64-bit hash
              │ • severity         │
              │ • languages[]      │
              │ • source           │
              └─────────┬──────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                           COMPILATION & DISTRIBUTION                                 │
│                         (semfora-cve compile -o security_patterns.bin)               │
└─────────────────────────────────────────────────────────────────────────────────────┘
                        │
                        ▼
              ┌────────────────────┐
              │  ArtifactCompiler  │
              │                    │
              │  1. Load patterns  │
              │     from SQLite    │
              │  2. Merge manual   │
              │     patterns       │
              │  3. Create         │
              │     PatternDatabase│
              │  4. Serialize      │
              │     (bincode)      │
              │  5. SHA256 hash    │
              └─────────┬──────────┘
                        │
            ┌───────────┼───────────┐
            ▼           ▼           ▼
     ┌──────────┐ ┌──────────┐ ┌────────────┐
     │ .bin     │ │ .json    │ │ SQLite     │
     │ artifact │ │ metadata │ │ artifact   │
     │          │ │          │ │ record     │
     │ version: │ │ sha256   │ │            │
     │ YYYYMMDD │ │ count    │ │ version    │
     │ .N       │ │ date     │ │ created_at │
     └────┬─────┘ └──────────┘ └────────────┘
          │
          │ ◄───── DISTRIBUTION POINT
          │
    ┌─────┴──────────────────────────────────────────┐
    │                                                │
    ▼                                                ▼
┌───────────────────────┐              ┌──────────────────────────────┐
│   EMBEDDED AT BUILD   │              │   RUNTIME UPDATE             │
│   (Air-gapped)        │              │   (Connected)                │
│                       │              │                              │
│ build.rs:             │              │ fetch_pattern_updates():     │
│ SECURITY_PATTERNS_PATH│              │ • SEMFORA_PATTERN_URL env    │
│ = "security_patterns  │              │ • Default: patterns.semfora  │
│     .bin"             │              │   .dev/security_patterns.bin │
│                       │              │ • Or local file path         │
│ include_bytes!()      │              │                              │
└───────────┬───────────┘              └──────────────┬───────────────┘
            │                                         │
            └─────────────────────┬───────────────────┘
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              SEMFORA-ENGINE                                          │
│                     (Pattern Matching at Scan Time)                                  │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
                   ┌──────────────────────────┐
                   │    PatternDatabase       │
                   │                          │
                   │  load_embedded_patterns()│
                   │  • Lazy-loaded once      │
                   │  • Cached in RwLock      │
                   │                          │
                   │  Runtime hot-swap via:   │
                   │  • update_security_      │
                   │    patterns MCP tool     │
                   └──────────────┬───────────┘
                                  │
                                  ▼
                   ┌──────────────────────────┐
                   │      cve_scan()          │
                   │   MCP Server Tool        │
                   │                          │
                   │  2-Pass Algorithm:       │
                   │  ┌────────────────────┐  │
                   │  │ Pass 1: Hamming    │  │
                   │  │ Fast 64-bit XOR    │  │
                   │  │ Filter candidates  │  │
                   │  └────────┬───────────┘  │
                   │           ▼              │
                   │  ┌────────────────────┐  │
                   │  │ Pass 2: Jaccard    │  │
                   │  │ Fine similarity    │  │
                   │  │ Confirm matches    │  │
                   │  └────────────────────┘  │
                   └──────────────┬───────────┘
                                  │
                                  ▼
                   ┌──────────────────────────┐
                   │       CVEMatch           │
                   │  • cve_id                │
                   │  • cwe_ids[]             │
                   │  • severity              │
                   │  • similarity (0.0-1.0)  │
                   │  • matched_function      │
                   │  • remediation           │
                   └──────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              SEMFORA-CI INTEGRATION                                  │
│                         (Quality Gates in CI/CD)                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘

     ┌───────────────────────────────────────────────────────────────────┐
     │                        semfora-ci                                 │
     │                                                                   │
     │  ┌─────────────────┐    ┌─────────────────┐    ┌───────────────┐ │
     │  │    Analyzer     │───▶│   semfora-      │───▶│ RuleEvaluator │ │
     │  │                 │    │   engine CLI    │    │               │ │
     │  │ • analyze_diff  │    │                 │    │ • Risk rules  │ │
     │  │ • validate_file │    │ • --diff        │    │ • Complexity  │ │
     │  │ • find_duplicates│   │ • --file-symbols│    │ • Duplicates  │ │
     │  │                 │    │ • --find-dupes  │    │ • Public API  │ │
     │  └─────────────────┘    └─────────────────┘    └───────┬───────┘ │
     │                                                        │         │
     │                                                        ▼         │
     │  ┌─────────────────────────────────────────────────────────────┐ │
     │  │                      Reporter                               │ │
     │  │  Output formats: text, json, github (step summary)          │ │
     │  │  Exit codes: 0 (pass), 1 (fail), 2 (error)                 │ │
     │  └─────────────────────────────────────────────────────────────┘ │
     └───────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                         ┌─────────────────────┐
                         │   CI/CD Pipeline    │
                         │                     │
                         │  GitHub Actions     │
                         │  GitLab CI          │
                         │  Jenkins            │
                         └─────────────────────┘
```

---

## Detailed Component Analysis

### 1. Advisory Sources (`semfora-cve/src/sources/`)

| Source | Status | API Type | Priority | Notes |
|--------|--------|----------|----------|-------|
| **NVD (NIST)** | ✅ Working | REST | 0 | `nvd.rs:16` - Uses `services.nvd.nist.gov/rest/json/cves/2.0` |
| **GHSA (GitHub)** | 🚧 In Progress | GraphQL | 0 | `ghsa.rs:15` - Requires `GITHUB_TOKEN` |
| **MITRE** | ✅ Available | Git Clone | 5 | `mitre.rs` - Clones CVE list repo |
| **CISA KEV** | ✅ Available | JSON Feed | 0 | Actively exploited vulnerabilities |
| **ENISA (EU)** | ⚠️ Untested | REST | 10 | EU vulnerability database |
| **JVN (Japan)** | ⚠️ Untested | REST | 15 | Japan-specific advisories |
| **Vendor (RedHat/MSRC)** | ⚠️ Untested | REST | 12 | Vendor-specific patches |

**Key Interface** (`sources/mod.rs:33-63`):
```rust
#[async_trait]
pub trait SourceFetcher: Send + Sync {
    fn source_id(&self) -> &'static str;
    fn source_name(&self) -> &'static str;
    async fn fetch_incremental(&self, since: DateTime<Utc>) -> Result<Vec<RawAdvisory>>;
    async fn fetch_full(&self) -> Result<Vec<RawAdvisory>>;
    fn rate_limiter(&self) -> &DefaultDirectRateLimiter;
    fn is_available(&self) -> bool;
    fn priority(&self) -> u8;
}
```

### 2. Claude AI Normalization (`semfora-cve/src/ai/`)

**Model**: `claude-sonnet-4-5-20250929` (`normalizer.rs:12`)

**Critical Output - `vulnerable_code` field** (`prompts.rs:51-68`):
Claude generates **actual syntactically valid vulnerable code examples** that can be analyzed by semfora-engine. This is the key to fingerprint generation:

```
### 6. vulnerable_code (REQUIRED - CRITICAL)
Generate a COMPLETE, SYNTACTICALLY VALID function that demonstrates the vulnerability.
Requirements:
- Must be a complete function definition
- Must be syntactically valid in the target language
- Must show realistic data flow
- Include comment marking vulnerable line with "// VULNERABLE:"
```

**Fallback Normalization** (`main.rs:293-342`):
When Claude is unavailable, heuristic rules map:
- Ecosystem → Language (npm → JavaScript, pip → Python)
- CVSS Score → Severity

### 3. Pattern Fingerprinting (`semfora-cve/src/patterns/`)

**Two fingerprinting approaches**:

1. **Engine Fingerprinting** (`engine_fingerprinter.rs:75-125`):
   - Calls `semfora-engine analyze-file` on Claude-generated code
   - Extracts semantic call graph, control flow, state operations
   - Most accurate but requires engine binary

2. **Fallback Fingerprinting** (`fingerprinter.rs`):
   - FNV-1a hash of:
     - Call sequences
     - Control flow patterns (e.g., "ITT" for if-try-try)
     - State operations
   - Works without engine but less precise

**CVEPattern Structure** (`security/mod.rs:108-163`):
```rust
pub struct CVEPattern {
    pub cve_id: String,
    pub cwe_ids: Vec<String>,
    pub pattern_id: u32,
    pub call_fingerprint: u64,      // 64-bit for fast Hamming
    pub control_flow_fingerprint: u64,
    pub state_fingerprint: u64,
    pub severity: Severity,
    pub languages: Vec<Lang>,
    // ...
}
```

### 4. Pattern Compilation & Distribution

**Compilation** (`compiler/mod.rs:59-88`):
```rust
pub fn compile(&self, patterns: Vec<CVEPattern>) -> Result<CompiledArtifact> {
    let version = self.store.next_version()?;  // YYYYMMDD.N format
    let db = PatternDatabase::new(version, patterns);
    let bytes = db.to_bytes()?;  // bincode serialization
    let sha256 = Self::compute_sha256(&bytes);
    // ...
}
```

**Output**:
- `security_patterns.bin` - Binary artifact
- `security_patterns.json` - Metadata sidecar (version, sha256, count)

### 5. Pattern Loading in semfora-engine

**Embedded Patterns** (`security/patterns/embedded.rs:14-20`):
```rust
#[cfg(feature = "embedded-patterns")]
static EMBEDDED_PATTERNS: &[u8] = include_bytes!(env!("SECURITY_PATTERNS_PATH"));

#[cfg(not(feature = "embedded-patterns"))]
static EMBEDDED_PATTERNS: &[u8] = &[];
```

**Runtime Updates** (`embedded.rs:134-212`):
```rust
pub async fn fetch_pattern_updates(url: Option<&str>, force: bool) -> Result<PatternUpdateResult>
```
- URL from `SEMFORA_PATTERN_URL` env or default `https://patterns.semfora.dev/`
- Atomic hot-swap via `RwLock<Option<PatternDatabase>>`

**Local File Loading** (`embedded.rs:217-220`):
```rust
pub fn update_patterns_from_file(path: &Path) -> Result<PatternUpdateResult>
```

### 6. semfora-ci Integration

**Current Integration** (`analyzer.rs:158-279`):
- Wraps `semfora-engine` CLI as subprocess
- Calls: `--diff`, `--uncommitted`, `--file-symbols`, `--find-duplicates`
- **Does NOT currently use `cve_scan`**

**Missing Integration Point**:
semfora-ci could add CVE scanning to its quality gates but doesn't yet.

---

## Air-Gapped Distribution Strategy

### Current Mechanisms

```
┌────────────────────────────────────────────────────────────────┐
│                    DISTRIBUTION OPTIONS                        │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  Option 1: EMBEDDED AT BUILD TIME (Air-gapped)                │
│  ────────────────────────────────────────────                 │
│  1. Run: semfora-cve compile -o security_patterns.bin         │
│  2. Set: SECURITY_PATTERNS_PATH=./security_patterns.bin       │
│  3. Build: cargo build --features embedded-patterns           │
│  4. Ship: Single binary with patterns baked in                │
│                                                                │
│  Option 2: RUNTIME FETCH (Connected)                          │
│  ──────────────────────────────────                           │
│  • MCP tool: update_security_patterns                         │
│  • URL: SEMFORA_PATTERN_URL or default pattern server         │
│  • Hot-swap without restart                                   │
│                                                                │
│  Option 3: RUNTIME FILE LOAD (Air-gapped Runtime)             │
│  ─────────────────────────────────────────────                │
│  • MCP tool: update_security_patterns(file_path: "...")       │
│  • Load from local .bin file                                  │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### Recommended Air-Gapped Release Cadence

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    PROPOSED RELEASE PIPELINE                            │
└─────────────────────────────────────────────────────────────────────────┘

  DAILY (Automated - Connected Environment)
  ─────────────────────────────────────────

  1. semfora-cve daemon runs on schedule
     │
     ▼
  2. Fetches from NVD (+ GHSA when ready)
     │
     ▼
  3. Claude normalizes → generates vulnerable code
     │
     ▼
  4. Patterns fingerprinted and stored in SQLite
     │
     ▼
  5. semfora-cve compile -o security_patterns.bin
     │
     ▼
  6. Upload to pattern server (patterns.semfora.dev)


  WEEKLY/MONTHLY (Manual Review - Release Artifacts)
  ─────────────────────────────────────────────────

  1. Review pattern quality metrics
     │
     ▼
  2. Tag release version (e.g., v2025.01.15-1)
     │
     ▼
  3. Generate artifacts:
     │
     ├── security_patterns.bin         (binary artifact)
     ├── security_patterns.json        (metadata)
     ├── semfora-engine-linux-x64      (with embedded patterns)
     ├── semfora-engine-darwin-arm64   (with embedded patterns)
     └── semfora-engine-windows-x64    (with embedded patterns)
     │
     ▼
  4. Publish to GitHub Releases / S3 / Internal mirror
     │
     ▼
  5. Air-gapped clients pull artifacts via sneakernet/internal network
```

---

## Integration Recommendations

### 1. Add CVE Scanning to semfora-ci

```rust
// semfora-ci/src/analyzer.rs - proposed addition
pub fn scan_cves(&self, threshold: f64) -> Result<Vec<CVEMatch>> {
    let output = Command::new(&self.engine_path)
        .args(["--cve-scan", "--threshold", &format!("{:.2}", threshold)])
        .current_dir(&self.working_dir)
        .output()?;
    // Parse output...
}
```

### 2. Pattern Server Infrastructure

```
┌─────────────────────────────────────────┐
│         patterns.semfora.dev            │
├─────────────────────────────────────────┤
│                                         │
│  /latest/                               │
│    security_patterns.bin                │
│    security_patterns.json               │
│                                         │
│  /archive/                              │
│    2025.01.15-1/                        │
│      security_patterns.bin              │
│      security_patterns.json             │
│    2025.01.08-1/                        │
│      ...                                │
│                                         │
│  /api/                                  │
│    /version → { "version": "2025..." } │
│    /stats → { "pattern_count": 1234 }  │
│                                         │
└─────────────────────────────────────────┘
```

### 3. GitHub Source Status

The GHSA source (`ghsa.rs`) is implemented but needs:
1. `GITHUB_TOKEN` environment variable
2. Testing with actual GraphQL queries
3. Verification of advisory → RawAdvisory conversion

---

## Current Working Data Flow

```
NVD API ─────┐
             │
             ▼
        RawAdvisory
             │
     ┌───────┴───────┐
     ▼               ▼
 Claude API    Fallback Rules
     │               │
     └───────┬───────┘
             ▼
      NormalizedCve
      (with vulnerable_code)
             │
             ▼
    Pattern Fingerprinting
             │
             ▼
       CVEPattern
             │
             ▼
    PatternDatabase
             │
     ┌───────┴───────┐
     ▼               ▼
  .bin file     Embedded in
 (runtime)      binary (build)
     │               │
     └───────┬───────┘
             ▼
      semfora-engine
       cve_scan()
             │
             ▼
       CVEMatch[]
```

---

## Summary

| Component | Location | Function | Status |
|-----------|----------|----------|--------|
| NVD Source | `semfora-cve/src/sources/nvd.rs` | Fetch CVEs from NIST | ✅ Working |
| GHSA Source | `semfora-cve/src/sources/ghsa.rs` | Fetch from GitHub | 🚧 Needs testing |
| AI Normalizer | `semfora-cve/src/ai/normalizer.rs` | Claude code generation | ✅ Working |
| Pattern Generator | `semfora-cve/src/patterns/` | Fingerprint creation | ✅ Working |
| Compiler | `semfora-cve/src/compiler/` | Binary artifact creation | ✅ Working |
| Pattern Loader | `semfora-engine/src/security/patterns/` | Load patterns | ✅ Working |
| CVE Scanner | `semfora-engine/src/mcp_server/` | MCP tool `cve_scan` | ✅ Working |
| CI Integration | `semfora-ci/src/analyzer.rs` | Calls engine CLI | ⚠️ Missing CVE scan |
