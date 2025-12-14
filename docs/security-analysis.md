# RFC: Semantic Security Analysis for Semfora Engine

**Status**: Draft
**Author**: Semfora Team
**Created**: 2025-12-13
**Target Version**: 3.0

---

## Abstract

This RFC proposes adding security vulnerability detection to Semfora Engine using a semantic-first approach that leverages existing infrastructure (call graphs, symbol extraction, risk scoring) without compromising the sub-5ms query performance budget. Rather than traditional SAST approaches that parse code repeatedly or use slow dataflow analysis, we classify security patterns using the same two-phase fingerprint → evaluate strategy proven by duplicate detection. This enables high-confidence findings for insecure deserialization, SQL injection, authentication misconfigurations, server/client boundary violations, and supply chain risks across JavaScript/TypeScript, Rust, and C#/.NET ecosystems.

---

## 1. Motivation

### 1.1 Why Semantic Security Analysis

Traditional static analysis security tools (SAST) suffer from fundamental problems:

| Problem | Traditional SAST | Semfora Approach |
|---------|------------------|------------------|
| **Performance** | Minutes to hours per scan | <5ms queries on precomputed index |
| **False Positives** | 50-80% noise rate | High-confidence only (target <10%) |
| **Context** | Single-file analysis | Cross-file call graphs and boundaries |
| **Explainability** | "Potential vulnerability at line X" | "Function Y calls Z without validation, reachable from client code" |
| **Integration** | Separate tool, separate workflow | Built into existing semantic index |

### 1.2 Why Now

1. **Infrastructure Ready**: Call graphs, symbol-level extraction, risk scoring, and boilerplate detection prove the architectural patterns work
2. **React Server Actions**: The 2024-2025 wave of server/client boundary vulnerabilities demonstrates need for framework-aware analysis
3. **C# Enterprise Gap**: Competitors (GitHub Copilot, Claude Code) struggle with large .NET repos; semantic analysis differentiates
4. **Supply Chain Focus**: Dependency analysis is already built; extending to security is natural

### 1.3 Non-Goals

This RFC explicitly does NOT propose:
- Full taint tracking or symbolic execution
- Runtime instrumentation or DAST
- Compliance frameworks (SOC2, PCI-DSS checklists)
- AI/LLM-based vulnerability detection

---

## 2. Design Philosophy

### 2.1 Performance Budget

**Constraint**: Security analysis MUST NOT exceed the existing performance envelope.

| Operation | Current Budget | With Security |
|-----------|----------------|---------------|
| File extraction | <10ms | <10ms (unchanged) |
| Index generation | <2s for 10K files | <2.5s for 10K files |
| Symbol query | <1ms | <1ms |
| Security query | N/A | <5ms |

### 2.2 Accuracy Over Coverage

We prioritize **high-confidence findings** over comprehensive scanning:

```
Detection Tiers:
├── Tier 1: Known-dangerous APIs (BinaryFormatter, eval, etc.)
│   └── Confidence: >95%, False Positive: <5%
├── Tier 2: Pattern-based risks (SQL concat, missing auth checks)
│   └── Confidence: >80%, False Positive: <15%
└── Tier 3: Heuristic warnings (complexity, boundary crossings)
    └── Confidence: >60%, False Positive: <30%
```

Rules in Tier 3 are informational; Tiers 1-2 are actionable.

### 2.3 Deterministic and Explainable

Every finding must be:
- **Reproducible**: Same input → same output
- **Traceable**: Points to specific symbols, calls, and line numbers
- **Actionable**: Clear remediation guidance

---

## 3. Architecture

### 3.1 SecuritySignature Struct

Parallel to `FunctionSignature` in duplicate detection, we introduce `SecuritySignature` for efficient coarse filtering:

```rust
// src/security/mod.rs

/// Lightweight security fingerprint for O(n) coarse filtering (~64 bytes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySignature {
    /// Symbol hash for lookup
    pub symbol_hash: String,

    /// Bitmap of dangerous API categories called (128 bits = 128 categories)
    /// Bit positions defined in DangerousApiCategory enum
    pub dangerous_api_bitmap: u128,

    /// Control flow security flags
    pub control_flow_flags: SecurityControlFlags,

    /// State mutation security flags
    pub state_flags: SecurityStateFlags,

    /// Dependency/import security flags
    pub dependency_flags: SecurityDependencyFlags,

    /// Server/client boundary context
    pub boundary_context: BoundaryContext,
}

bitflags::bitflags! {
    /// Control flow patterns relevant to security
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct SecurityControlFlags: u32 {
        const HAS_TRY_CATCH       = 0b0000_0001;
        const HAS_ASYNC_AWAIT     = 0b0000_0010;
        const UNHANDLED_ASYNC     = 0b0000_0100;  // async without try
        const HAS_DYNAMIC_EVAL    = 0b0000_1000;
        const HAS_REFLECTION      = 0b0001_0000;
        const HAS_SERIALIZATION   = 0b0010_0000;
        const HAS_FILE_IO         = 0b0100_0000;
        const HAS_NETWORK_IO      = 0b1000_0000;
        const HAS_DB_ACCESS       = 0b0001_0000_0000;
        const HAS_CRYPTO          = 0b0010_0000_0000;
        const HAS_AUTH            = 0b0100_0000_0000;
        const HAS_USER_INPUT      = 0b1000_0000_0000;
    }
}

bitflags::bitflags! {
    /// State patterns relevant to security
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct SecurityStateFlags: u32 {
        const MUTATES_GLOBAL      = 0b0000_0001;
        const STORES_SECRETS      = 0b0000_0010;  // detected via naming
        const STORES_CREDENTIALS  = 0b0000_0100;
        const USES_ENV_VARS       = 0b0000_1000;
        const STORES_USER_DATA    = 0b0001_0000;
    }
}

bitflags::bitflags! {
    /// Dependency patterns relevant to security
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct SecurityDependencyFlags: u64 {
        // JavaScript/TypeScript
        const IMPORTS_REACT       = 0b0000_0000_0001;
        const IMPORTS_NEXTJS      = 0b0000_0000_0010;
        const IMPORTS_EXPRESS     = 0b0000_0000_0100;
        const IMPORTS_PRISMA      = 0b0000_0000_1000;

        // C#/.NET
        const IMPORTS_ASPNET      = 0b0000_0001_0000;
        const IMPORTS_EF_CORE     = 0b0000_0010_0000;
        const IMPORTS_NEWTONSOFT  = 0b0000_0100_0000;
        const IMPORTS_SYSTEM_WEB  = 0b0000_1000_0000;
        const IMPORTS_UNITY       = 0b0001_0000_0000;

        // Rust
        const IMPORTS_SERDE       = 0b0010_0000_0000;
        const IMPORTS_TOKIO       = 0b0100_0000_0000;
        const IMPORTS_ACTIX       = 0b1000_0000_0000;
    }
}

/// Server/client boundary context for React/Next.js
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum BoundaryContext {
    #[default]
    Unknown,
    ServerOnly,      // 'use server' directive
    ClientOnly,      // 'use client' directive
    ServerAction,    // Function-level 'use server'
    SharedUnsafe,    // Used in both contexts (potential leak)
}
```

### 3.2 Dangerous API Categories

The `dangerous_api_bitmap` uses bit positions for O(1) category checks:

```rust
// src/security/categories.rs

/// Categories of dangerous APIs (max 128 for u128 bitmap)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DangerousApiCategory {
    // Deserialization (bits 0-7)
    BinaryFormatterDeserialize = 0,
    JsonTypeNameHandling = 1,
    XmlSerializerUntrusted = 2,
    YamlDeserializeUntrusted = 3,
    PickleLoad = 4,

    // Injection (bits 8-15)
    SqlStringConcat = 8,
    CommandExecution = 9,
    EvalExecution = 10,
    TemplateInjection = 11,
    XPathInjection = 12,
    LdapInjection = 13,

    // Authentication (bits 16-23)
    WeakCookieConfig = 16,
    MissingCsrfProtection = 17,
    HardcodedCredentials = 18,
    WeakCrypto = 19,
    InsecureRandomness = 20,

    // Data Exposure (bits 24-31)
    SensitiveDataLogging = 24,
    ErrorMessageLeakage = 25,
    StackTraceExposure = 26,
    ServerCodeInClient = 27,
    SecretsInClientBundle = 28,

    // Network (bits 32-39)
    SsrfVulnerable = 32,
    OpenRedirect = 33,
    CorsWildcard = 34,
    InsecureTls = 35,

    // File System (bits 40-47)
    PathTraversal = 40,
    ArbitraryFileWrite = 41,
    SymlinkFollowing = 42,

    // Framework-Specific (bits 48-63)
    ViewStateWithoutMac = 48,
    AspNetRequestSmuggling = 49,
    EfCoreOverPosting = 50,
    ReactServerActionLeak = 51,
    NextjsBoundaryViolation = 52,
    UnityUnsafeNative = 53,
}

impl DangerousApiCategory {
    /// Convert to bitmap position
    pub fn to_bit(self) -> u128 {
        1u128 << (self as u8)
    }
}
```

### 3.3 Rule Engine Design

Following the boilerplate detection pattern, rules are declarative:

```rust
// src/security/rules.rs

/// A security detection rule
#[derive(Debug, Clone)]
pub struct SecurityRule {
    /// Unique rule identifier (e.g., "CSHARP_BINARY_FORMATTER")
    pub id: &'static str,

    /// Human-readable title
    pub title: &'static str,

    /// Severity level
    pub severity: Severity,

    /// Confidence level
    pub confidence: Confidence,

    /// Languages this rule applies to
    pub languages: &'static [Lang],

    /// Required dangerous API bits (ANY match triggers evaluation)
    pub required_api_bits: u128,

    /// Required control flow flags (ALL must be present)
    pub required_control_flags: SecurityControlFlags,

    /// Forbidden control flow flags (NONE must be present)
    pub forbidden_control_flags: SecurityControlFlags,

    /// Required state flags
    pub required_state_flags: SecurityStateFlags,

    /// Required dependency flags
    pub required_dependency_flags: SecurityDependencyFlags,

    /// Boundary context requirement
    pub boundary_requirement: Option<BoundaryContext>,

    /// Custom detector function for complex rules
    pub custom_detector: Option<fn(&SymbolInfo, &SecuritySignature) -> bool>,

    /// Remediation guidance
    pub remediation: &'static str,

    /// External reference (CWE, CVE, etc.)
    pub reference: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Critical,  // Immediate exploitation risk
    High,      // Significant security impact
    Medium,    // Potential security issue
    Low,       // Informational / best practice
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,    // >90% true positive rate expected
    Medium,  // >70% true positive rate expected
    Low,     // Heuristic, may have false positives
}
```

### 3.4 Two-Phase Detection

```rust
// src/security/detector.rs

pub struct SecurityDetector {
    rules: Vec<SecurityRule>,
    /// Precomputed OR of all rule required_api_bits for fast skip
    any_rule_api_mask: u128,
}

impl SecurityDetector {
    /// Phase A: Coarse filter using bitmap operations
    /// Returns rules that MIGHT match based on signatures
    fn coarse_filter(&self, sig: &SecuritySignature) -> Vec<&SecurityRule> {
        // Quick exit: if no dangerous APIs, only check boundary rules
        if sig.dangerous_api_bitmap == 0
            && sig.boundary_context == BoundaryContext::Unknown {
            return vec![];
        }

        self.rules
            .iter()
            .filter(|rule| {
                // Check API bits (any overlap)
                let api_match = rule.required_api_bits == 0
                    || (sig.dangerous_api_bitmap & rule.required_api_bits) != 0;

                // Check control flow flags
                let cf_match = sig.control_flow_flags
                    .contains(rule.required_control_flags);
                let cf_forbidden = !sig.control_flow_flags
                    .intersects(rule.forbidden_control_flags);

                // Check boundary context
                let boundary_match = rule.boundary_requirement
                    .map(|req| sig.boundary_context == req)
                    .unwrap_or(true);

                api_match && cf_match && cf_forbidden && boundary_match
            })
            .collect()
    }

    /// Phase B: Fine evaluation with full symbol info
    fn evaluate(&self, info: &SymbolInfo, sig: &SecuritySignature,
                rules: &[&SecurityRule]) -> Vec<SecurityFinding> {
        rules
            .iter()
            .filter_map(|rule| {
                // Run custom detector if present
                if let Some(detector) = rule.custom_detector {
                    if !detector(info, sig) {
                        return None;
                    }
                }

                // Additional validation checks could go here

                Some(SecurityFinding {
                    rule_id: rule.id.to_string(),
                    severity: rule.severity,
                    confidence: rule.confidence,
                    symbol_hash: sig.symbol_hash.clone(),
                    symbol_name: info.name.clone(),
                    file: String::new(), // Filled by caller
                    start_line: info.start_line,
                    end_line: info.end_line,
                    remediation: rule.remediation.to_string(),
                    reference: rule.reference.map(String::from),
                })
            })
            .collect()
    }
}
```

### 3.5 Storage Schema Additions

```rust
// Additions to src/schema.rs

/// A security finding associated with a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub rule_id: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub symbol_hash: String,
    pub symbol_name: String,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub remediation: String,
    pub reference: Option<String>,
}

// Addition to ModuleShard
pub struct ModuleShard {
    // ... existing fields ...

    /// Security findings in this module
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_findings: Vec<SecurityFinding>,

    /// Aggregated security risk for this module
    pub security_risk: SecurityRiskSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityRiskSummary {
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    /// Most severe rule IDs for quick reference
    pub top_findings: Vec<String>,
}
```

### 3.6 MCP API Extensions

```rust
// Additions to src/mcp_server/types.rs

#[derive(Debug, Deserialize)]
pub struct SearchSecurityRequest {
    /// Filter by severity
    pub severity: Option<Severity>,
    /// Filter by rule ID pattern
    pub rule_pattern: Option<String>,
    /// Filter by module
    pub module: Option<String>,
    /// Include remediation guidance
    pub include_remediation: bool,
    /// Maximum results
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SecurityOverview {
    pub total_findings: usize,
    pub by_severity: HashMap<Severity, usize>,
    pub by_rule: HashMap<String, usize>,
    pub by_module: HashMap<String, usize>,
    pub top_findings: Vec<SecurityFinding>,
}
```

---

## 4. Rule Catalog: JavaScript/TypeScript

### 4.1 High-Confidence Rules (Tier 1)

#### JS_EVAL_EXECUTION
```rust
SecurityRule {
    id: "JS_EVAL_EXECUTION",
    title: "Direct eval() execution",
    severity: Severity::Critical,
    confidence: Confidence::High,
    languages: &[Lang::JavaScript, Lang::TypeScript, Lang::Jsx, Lang::Tsx],
    required_api_bits: DangerousApiCategory::EvalExecution.to_bit(),
    required_control_flags: SecurityControlFlags::empty(),
    forbidden_control_flags: SecurityControlFlags::empty(),
    required_state_flags: SecurityStateFlags::empty(),
    required_dependency_flags: SecurityDependencyFlags::empty(),
    boundary_requirement: None,
    custom_detector: Some(|info, _| {
        info.calls.iter().any(|c| c.name == "eval" && c.object.is_none())
    }),
    remediation: "Remove eval() and use safer alternatives like JSON.parse() for data or Function constructors with strict input validation.",
    reference: Some("CWE-95"),
}
```

**AST Pattern**:
```
call_expression
├── function: identifier[name="eval"]
└── arguments: [any]
```

#### JS_SQL_STRING_CONCAT
```rust
SecurityRule {
    id: "JS_SQL_STRING_CONCAT",
    title: "SQL query built with string concatenation",
    severity: Severity::High,
    confidence: Confidence::High,
    languages: &[Lang::JavaScript, Lang::TypeScript],
    required_api_bits: DangerousApiCategory::SqlStringConcat.to_bit(),
    required_control_flags: SecurityControlFlags::HAS_DB_ACCESS,
    // ...
    custom_detector: Some(|info, _| {
        // Check for db.query/execute with template literal or concat
        let has_db_call = info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "query" | "execute" | "raw") &&
            matches!(c.object.as_deref(), Some("db") | Some("prisma") | Some("sql") | Some("knex"))
        });
        // Check state changes for SQL-like string building
        let has_sql_concat = info.state_changes.iter().any(|s| {
            s.initializer.contains("SELECT") ||
            s.initializer.contains("INSERT") ||
            s.initializer.contains("UPDATE")
        });
        has_db_call && has_sql_concat
    }),
    remediation: "Use parameterized queries: db.query('SELECT * FROM users WHERE id = $1', [userId])",
    reference: Some("CWE-89"),
}
```

#### JS_HARDCODED_SECRET
```rust
SecurityRule {
    id: "JS_HARDCODED_SECRET",
    title: "Hardcoded secret or API key",
    severity: Severity::High,
    confidence: Confidence::Medium,
    languages: &[Lang::JavaScript, Lang::TypeScript, Lang::Jsx, Lang::Tsx],
    required_api_bits: DangerousApiCategory::HardcodedCredentials.to_bit(),
    custom_detector: Some(|info, _| {
        let secret_patterns = [
            "api_key", "apikey", "api-key", "secret", "password",
            "token", "auth", "credential", "private_key"
        ];
        info.state_changes.iter().any(|s| {
            let name_lower = s.name.to_lowercase();
            let has_secret_name = secret_patterns.iter().any(|p| name_lower.contains(p));
            let has_literal_value = !s.initializer.is_empty()
                && !s.initializer.starts_with("process.env")
                && !s.initializer.starts_with("env.")
                && !s.initializer.contains("getenv");
            has_secret_name && has_literal_value
        })
    }),
    remediation: "Move secrets to environment variables: const apiKey = process.env.API_KEY",
    reference: Some("CWE-798"),
}
```

### 4.2 Framework-Specific Rules

#### NEXTJS_SERVER_ACTION_LEAK
```rust
SecurityRule {
    id: "NEXTJS_SERVER_ACTION_LEAK",
    title: "Server action accessible from client with sensitive operations",
    severity: Severity::Critical,
    confidence: Confidence::High,
    languages: &[Lang::TypeScript, Lang::Tsx],
    required_api_bits: DangerousApiCategory::ReactServerActionLeak.to_bit(),
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_NEXTJS,
    boundary_requirement: Some(BoundaryContext::ServerAction),
    custom_detector: Some(|info, sig| {
        // Server action that accesses secrets or performs privileged ops
        let accesses_secrets = sig.state_flags.contains(SecurityStateFlags::USES_ENV_VARS)
            || sig.state_flags.contains(SecurityStateFlags::STORES_SECRETS);
        let has_db_access = sig.control_flow_flags.contains(SecurityControlFlags::HAS_DB_ACCESS);
        let has_auth = sig.control_flow_flags.contains(SecurityControlFlags::HAS_AUTH);

        // Check if reachable from client without validation
        let lacks_validation = !info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "authenticate" | "authorize" | "checkPermission" | "validate")
        });

        (accesses_secrets || has_db_access) && lacks_validation
    }),
    remediation: "Add authentication check at the start of server actions: const user = await auth(); if (!user) throw new Error('Unauthorized');",
    reference: Some("CWE-284"),
}
```

#### REACT_DANGEROUSLY_SET_HTML
```rust
SecurityRule {
    id: "REACT_DANGEROUSLY_SET_HTML",
    title: "dangerouslySetInnerHTML with dynamic content",
    severity: Severity::High,
    confidence: Confidence::High,
    languages: &[Lang::Jsx, Lang::Tsx],
    required_api_bits: DangerousApiCategory::TemplateInjection.to_bit(),
    custom_detector: Some(|info, _| {
        // Check for dangerouslySetInnerHTML prop usage
        info.calls.iter().any(|c| c.name == "dangerouslySetInnerHTML")
    }),
    remediation: "Use a sanitization library like DOMPurify: dangerouslySetInnerHTML={{__html: DOMPurify.sanitize(content)}}",
    reference: Some("CWE-79"),
}
```

### 4.3 Additional JS/TS Rules (Summary)

| Rule ID | Severity | Description |
|---------|----------|-------------|
| `JS_PROTOTYPE_POLLUTION` | High | Object.assign/spread with untrusted input |
| `JS_OPEN_REDIRECT` | Medium | Redirect with user-controlled URL |
| `JS_COMMAND_INJECTION` | Critical | exec/spawn with string concat |
| `JS_PATH_TRAVERSAL` | High | File operations with user input |
| `JS_INSECURE_RANDOM` | Medium | Math.random() for security purposes |
| `JS_WEAK_CRYPTO` | Medium | MD5/SHA1 for passwords |
| `JS_CORS_WILDCARD` | Medium | Access-Control-Allow-Origin: * with credentials |
| `JS_MISSING_CSRF` | Medium | Form handler without CSRF token |
| `EXPRESS_BODY_PARSER_LIMIT` | Low | Missing request body size limit |
| `REACT_USEEFFECT_ASYNC` | Low | Async function in useEffect without cleanup |

---

## 5. Rule Catalog: Rust

### 5.1 High-Confidence Rules

#### RUST_UNSAFE_TRANSMUTE
```rust
SecurityRule {
    id: "RUST_UNSAFE_TRANSMUTE",
    title: "std::mem::transmute usage",
    severity: Severity::High,
    confidence: Confidence::High,
    languages: &[Lang::Rust],
    required_api_bits: 0, // Custom detection
    custom_detector: Some(|info, _| {
        info.calls.iter().any(|c| {
            c.name == "transmute" &&
            matches!(c.object.as_deref(), Some("mem") | Some("std::mem"))
        })
    }),
    remediation: "Prefer safe alternatives: as casts, From/Into traits, or bytemuck for Pod types",
    reference: Some("CWE-843"),
}
```

#### RUST_UNWRAP_IN_LIB
```rust
SecurityRule {
    id: "RUST_UNWRAP_IN_LIB",
    title: "unwrap()/expect() in library code",
    severity: Severity::Medium,
    confidence: Confidence::Medium,
    languages: &[Lang::Rust],
    custom_detector: Some(|info, sig| {
        // Only flag in non-test, non-main code
        let is_lib_code = !info.name.starts_with("test_")
            && info.name != "main";
        let has_unwrap = info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "unwrap" | "expect")
        });
        is_lib_code && has_unwrap
    }),
    remediation: "Return Result<T, E> and propagate errors with ? operator",
    reference: Some("CWE-248"),
}
```

#### RUST_SQL_FORMAT
```rust
SecurityRule {
    id: "RUST_SQL_FORMAT",
    title: "SQL query built with format!()",
    severity: Severity::High,
    confidence: Confidence::High,
    languages: &[Lang::Rust],
    required_api_bits: DangerousApiCategory::SqlStringConcat.to_bit(),
    custom_detector: Some(|info, _| {
        // Check for sqlx/diesel calls with format! in initializers
        let has_sql_call = info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "query" | "execute" | "query_as" | "sql")
        });
        let has_format = info.state_changes.iter().any(|s| {
            s.initializer.contains("format!") &&
            (s.initializer.contains("SELECT") ||
             s.initializer.contains("INSERT") ||
             s.initializer.contains("DELETE"))
        });
        has_sql_call && has_format
    }),
    remediation: "Use query parameters: sqlx::query!(\"SELECT * FROM users WHERE id = $1\", user_id)",
    reference: Some("CWE-89"),
}
```

### 5.2 Additional Rust Rules (Summary)

| Rule ID | Severity | Description |
|---------|----------|-------------|
| `RUST_UNSAFE_BLOCK` | Medium | Unsafe block without safety comment |
| `RUST_PANIC_IN_FFI` | Critical | panic!() in extern "C" function |
| `RUST_RACE_CONDITION` | High | Shared mutable state without synchronization |
| `RUST_PATH_TRAVERSAL` | High | File path from user input without canonicalize |
| `RUST_WEAK_CRYPTO` | Medium | Use of md5/sha1 crates for security |
| `RUST_HARDCODED_SECRET` | High | String literal with secret-like name |
| `RUST_DESERIALIZE_UNTRUSTED` | High | serde deserialize on network input |

---

## 6. Rule Catalog: C#/.NET

### 6.1 ASP.NET Core / Kestrel

#### CSHARP_REQUEST_SMUGGLING
```rust
SecurityRule {
    id: "CSHARP_REQUEST_SMUGGLING",
    title: "Potential HTTP request smuggling via header manipulation",
    severity: Severity::High,
    confidence: Confidence::Medium,
    languages: &[Lang::CSharp],
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_ASPNET,
    custom_detector: Some(|info, _| {
        // Check for Content-Length or Transfer-Encoding manipulation
        info.calls.iter().any(|c| {
            (c.name == "Add" || c.name == "Set") &&
            matches!(c.object.as_deref(), Some("Headers") | Some("Request.Headers"))
        }) && info.state_changes.iter().any(|s| {
            s.initializer.contains("Content-Length") ||
            s.initializer.contains("Transfer-Encoding")
        })
    }),
    remediation: "Avoid manual header manipulation. Use framework-provided methods and ensure Kestrel version >= 2.3.1",
    reference: Some("CVE-2025-55315"),
}
```

#### CSHARP_WEAK_COOKIE_CONFIG
```rust
SecurityRule {
    id: "CSHARP_WEAK_COOKIE_CONFIG",
    title: "Cookie without Secure, HttpOnly, or SameSite",
    severity: Severity::Medium,
    confidence: Confidence::High,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::WeakCookieConfig.to_bit(),
    custom_detector: Some(|info, _| {
        // Check for CookieOptions without security flags
        let creates_cookie = info.calls.iter().any(|c| {
            c.name == "Append" && c.object.as_deref() == Some("Cookies")
        });
        let has_cookie_options = info.state_changes.iter().any(|s| {
            s.type_annotation.as_deref() == Some("CookieOptions")
        });
        let missing_security = !info.state_changes.iter().any(|s| {
            s.initializer.contains("Secure = true") &&
            s.initializer.contains("HttpOnly = true") &&
            s.initializer.contains("SameSite")
        });
        creates_cookie && has_cookie_options && missing_security
    }),
    remediation: "Set cookie options: new CookieOptions { Secure = true, HttpOnly = true, SameSite = SameSiteMode.Strict }",
    reference: Some("CWE-614"),
}
```

### 6.2 Serialization Vulnerabilities

#### CSHARP_BINARY_FORMATTER
```rust
SecurityRule {
    id: "CSHARP_BINARY_FORMATTER",
    title: "BinaryFormatter deserialization (RCE risk)",
    severity: Severity::Critical,
    confidence: Confidence::High,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::BinaryFormatterDeserialize.to_bit(),
    custom_detector: Some(|info, _| {
        info.calls.iter().any(|c| {
            c.name == "Deserialize" &&
            c.object.as_deref() == Some("BinaryFormatter")
        })
    }),
    remediation: "BinaryFormatter is dangerous and deprecated. Use System.Text.Json or DataContractSerializer with known types only.",
    reference: Some("CWE-502"),
}
```

#### CSHARP_JSON_TYPE_HANDLING
```rust
SecurityRule {
    id: "CSHARP_JSON_TYPE_HANDLING",
    title: "Newtonsoft.Json with TypeNameHandling enabled",
    severity: Severity::Critical,
    confidence: Confidence::High,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::JsonTypeNameHandling.to_bit(),
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_NEWTONSOFT,
    custom_detector: Some(|info, _| {
        // Check for TypeNameHandling in JsonSerializerSettings
        info.state_changes.iter().any(|s| {
            s.initializer.contains("TypeNameHandling") &&
            !s.initializer.contains("TypeNameHandling.None")
        })
    }),
    remediation: "Set TypeNameHandling.None or use System.Text.Json. If polymorphic deserialization is needed, use a custom SerializationBinder.",
    reference: Some("CWE-502"),
}
```

#### CSHARP_VIEWSTATE_WITHOUT_MAC
```rust
SecurityRule {
    id: "CSHARP_VIEWSTATE_WITHOUT_MAC",
    title: "ViewState without MAC validation (legacy ASP.NET)",
    severity: Severity::Critical,
    confidence: Confidence::High,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::ViewStateWithoutMac.to_bit(),
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_SYSTEM_WEB,
    custom_detector: Some(|info, _| {
        // Check for EnableViewStateMac = false
        info.state_changes.iter().any(|s| {
            s.initializer.contains("EnableViewStateMac") &&
            s.initializer.contains("false")
        })
    }),
    remediation: "Never disable ViewState MAC validation. Ensure machine keys are unique per deployment and not committed to source control.",
    reference: Some("CWE-502"),
}
```

### 6.3 Entity Framework Core

#### CSHARP_EF_OVERPOSTING
```rust
SecurityRule {
    id: "CSHARP_EF_OVERPOSTING",
    title: "Entity Framework model binding without DTO (mass assignment risk)",
    severity: Severity::High,
    confidence: Confidence::Medium,
    languages: &[Lang::CSharp],
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_EF_CORE,
    custom_detector: Some(|info, _| {
        // Check for DbContext.Update/Add with direct entity binding
        let has_ef_mutation = info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "Update" | "Add" | "Attach") &&
            c.object.as_deref().map(|o| o.ends_with("Context") || o == "db").unwrap_or(false)
        });
        // Check if function is a controller action (takes model as param)
        let is_controller_action = info.arguments.iter().any(|a| {
            a.type_annotation.as_ref().map(|t| {
                !t.contains("Dto") && !t.contains("ViewModel") && !t.contains("Request")
            }).unwrap_or(false)
        });
        has_ef_mutation && is_controller_action
    }),
    remediation: "Use DTOs for model binding: public async Task<IActionResult> Update(UserUpdateDto dto) and map to entity explicitly.",
    reference: Some("CWE-915"),
}
```

#### CSHARP_EF_RAW_SQL
```rust
SecurityRule {
    id: "CSHARP_EF_RAW_SQL",
    title: "EF Core raw SQL with string interpolation",
    severity: Severity::High,
    confidence: Confidence::High,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::SqlStringConcat.to_bit(),
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_EF_CORE,
    custom_detector: Some(|info, _| {
        info.calls.iter().any(|c| {
            matches!(c.name.as_str(), "FromSqlRaw" | "ExecuteSqlRaw") &&
            // Check if argument looks like interpolation
            info.state_changes.iter().any(|s| {
                s.initializer.contains("$\"") || s.initializer.contains("String.Format")
            })
        })
    }),
    remediation: "Use FromSqlInterpolated for automatic parameterization: db.Users.FromSqlInterpolated($\"SELECT * FROM Users WHERE Id = {userId}\")",
    reference: Some("CWE-89"),
}
```

### 6.4 Unity-Specific Rules

#### UNITY_UNSAFE_NATIVE
```rust
SecurityRule {
    id: "UNITY_UNSAFE_NATIVE",
    title: "Unity native plugin with unvalidated input",
    severity: Severity::High,
    confidence: Confidence::Medium,
    languages: &[Lang::CSharp],
    required_api_bits: DangerousApiCategory::UnityUnsafeNative.to_bit(),
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_UNITY,
    custom_detector: Some(|info, _| {
        // Check for DllImport calls with user-controllable data
        let has_dll_import = info.calls.iter().any(|c| {
            c.name.starts_with("Native") ||
            info.state_changes.iter().any(|s| s.initializer.contains("[DllImport"))
        });
        let has_user_input = info.arguments.iter().any(|a| {
            matches!(a.name.as_str(), "input" | "data" | "buffer" | "path")
        });
        has_dll_import && has_user_input
    }),
    remediation: "Validate and sanitize all input before passing to native code. Use SafeHandle for native resources.",
    reference: Some("CWE-20"),
}
```

#### UNITY_INSECURE_PLAYERPREFS
```rust
SecurityRule {
    id: "UNITY_INSECURE_PLAYERPREFS",
    title: "Sensitive data in PlayerPrefs (unencrypted)",
    severity: Severity::Medium,
    confidence: Confidence::Medium,
    languages: &[Lang::CSharp],
    required_dependency_flags: SecurityDependencyFlags::IMPORTS_UNITY,
    custom_detector: Some(|info, _| {
        let uses_playerprefs = info.calls.iter().any(|c| {
            c.name.starts_with("SetString") && c.object.as_deref() == Some("PlayerPrefs")
        });
        let stores_sensitive = info.state_changes.iter().any(|s| {
            let name_lower = s.name.to_lowercase();
            name_lower.contains("token") || name_lower.contains("password") ||
            name_lower.contains("key") || name_lower.contains("secret")
        });
        uses_playerprefs && stores_sensitive
    }),
    remediation: "Use Unity's encrypted PlayerPrefs or a secure storage solution. Never store authentication tokens in plain PlayerPrefs.",
    reference: Some("CWE-312"),
}
```

### 6.5 Additional C# Rules (Summary)

| Rule ID | Severity | Description |
|---------|----------|-------------|
| `CSHARP_OPEN_REDIRECT` | Medium | Response.Redirect with user input |
| `CSHARP_CORS_WILDCARD` | Medium | AllowAnyOrigin with AllowCredentials |
| `CSHARP_PATH_TRAVERSAL` | High | Path.Combine with user input |
| `CSHARP_WEAK_CRYPTO` | Medium | MD5/SHA1CryptoServiceProvider usage |
| `CSHARP_HARDCODED_CONNECTION` | High | Connection string in source code |
| `CSHARP_MISSING_ANTIFORGERY` | Medium | POST action without ValidateAntiForgeryToken |
| `CSHARP_LDAP_INJECTION` | High | DirectorySearcher with string concat |
| `CSHARP_XPATH_INJECTION` | High | XPathNavigator with user input |
| `CSHARP_REGEX_DOS` | Medium | Regex with unbounded repetition on user input |
| `CSHARP_DESERIALIZE_UNTRUSTED` | High | XmlSerializer with unknown types |

---

## 7. React Server Actions Boundary Detection

### 7.1 Vulnerability Class

The React Server Actions / Server Components vulnerability class involves **server-only code being exposed to or executed from client contexts**, potentially leaking:
- Uncompiled server function source code
- Environment variables and secrets
- Database connection logic
- Internal API endpoints

This is NOT a "bug" in React itself, but a **framework integration and build-boundary failure**.

### 7.2 Existing Signals in Semfora

| Signal | Current Status | Gap |
|--------|----------------|-----|
| `'use server'` directive (file-level) | Detected | Need function-level |
| `'use client'` directive | Detected | N/A |
| Call graph | Fully built | Need reachability analysis |
| Import tracking | Fully tracked | N/A |
| Environment access | Partially detected | Need `process.env` pattern |
| Framework detection (Next.js) | Full support | N/A |

### 7.3 Implementation Approach

#### Step 1: Enhance Directive Detection

```rust
// In src/detectors/javascript/frameworks/nextjs.rs

/// Detect function-level "use server" directives within client components
pub fn detect_server_actions_in_client(
    summary: &mut SemanticSummary,
    source: &str,
    tree: &Tree,
) {
    // Check if file is marked as client
    let is_client_file = source.trim_start().starts_with("'use client'")
        || source.trim_start().starts_with("\"use client\"");

    if !is_client_file {
        return;
    }

    // Find function bodies with "use server" directive
    let mut cursor = tree.walk();
    for node in tree.root_node().children(&mut cursor) {
        if node.kind() == "function_declaration" ||
           node.kind() == "arrow_function" {
            if let Some(body) = node.child_by_field_name("body") {
                let body_text = &source[body.start_byte()..body.end_byte()];
                if body_text.trim_start().starts_with("'use server'")
                    || body_text.trim_start().starts_with("\"use server\"") {
                    // Mark this function as a server action
                    if let Some(ref mut symbol) = summary.symbols.iter_mut()
                        .find(|s| s.start_line == node.start_position().row + 1) {
                        symbol.boundary_context = Some(BoundaryContext::ServerAction);
                    }
                }
            }
        }
    }
}
```

#### Step 2: Build Client Reachability Graph

```rust
// In src/security/boundary.rs

/// Compute which symbols are reachable from client entry points
pub fn compute_client_reachability(
    call_graph: &HashMap<String, Vec<String>>,
    symbols: &HashMap<String, SymbolInfo>,
    entry_points: &[String],  // Symbols in 'use client' files
) -> HashSet<String> {
    let mut reachable = HashSet::new();
    let mut queue: VecDeque<&str> = entry_points.iter().map(|s| s.as_str()).collect();

    while let Some(hash) = queue.pop_front() {
        if reachable.contains(hash) {
            continue;
        }
        reachable.insert(hash.to_string());

        // Add all callees
        if let Some(callees) = call_graph.get(hash) {
            for callee in callees {
                if !callee.starts_with("ext:") && !reachable.contains(callee) {
                    queue.push_back(callee);
                }
            }
        }
    }

    reachable
}
```

#### Step 3: Detect Boundary Violations

```rust
// In src/security/boundary.rs

/// Find server actions that are reachable from client code
pub fn find_boundary_violations(
    symbols: &HashMap<String, SymbolInfo>,
    signatures: &HashMap<String, SecuritySignature>,
    client_reachable: &HashSet<String>,
) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();

    for (hash, sig) in signatures {
        // Skip if not reachable from client
        if !client_reachable.contains(hash) {
            continue;
        }

        // Check if this is a server-only symbol
        let is_server_only = sig.boundary_context == BoundaryContext::ServerOnly
            || sig.boundary_context == BoundaryContext::ServerAction;

        // Check for sensitive operations
        let has_sensitive_ops = sig.state_flags.contains(SecurityStateFlags::USES_ENV_VARS)
            || sig.state_flags.contains(SecurityStateFlags::STORES_SECRETS)
            || sig.control_flow_flags.contains(SecurityControlFlags::HAS_DB_ACCESS);

        if is_server_only && has_sensitive_ops {
            let info = symbols.get(hash).unwrap();
            findings.push(SecurityFinding {
                rule_id: "NEXTJS_BOUNDARY_VIOLATION".to_string(),
                severity: Severity::Critical,
                confidence: Confidence::High,
                symbol_hash: hash.clone(),
                symbol_name: info.name.clone(),
                file: String::new(),
                start_line: info.start_line,
                end_line: info.end_line,
                remediation: "Ensure server actions validate authentication and don't expose secrets. Consider using a separate API layer.".to_string(),
                reference: Some("CWE-200".to_string()),
            });
        }
    }

    findings
}
```

### 7.4 Expected Accuracy

| Scenario | Detection | False Positive Risk |
|----------|-----------|---------------------|
| Server action with `process.env` access | HIGH | LOW |
| Server action with DB calls | HIGH | LOW |
| Legitimate server action with auth check | Filtered out | N/A |
| Shared utility function | MEDIUM | MEDIUM |
| Dynamic imports | NOT DETECTED | N/A (known limitation) |

---

## 8. Implementation Phases

### Phase 1: Foundation (2 weeks)

**Week 1:**
- [ ] Create `src/security/mod.rs` module structure
- [ ] Implement `SecuritySignature` and bit flags
- [ ] Implement `SecurityRule` and `SecurityDetector`
- [ ] Add signature generation to index pipeline

**Week 2:**
- [ ] Implement 10 high-confidence JS/TS rules
- [ ] Implement 5 high-confidence Rust rules
- [ ] Add `security_findings` to module shards
- [ ] Expose `search_security` MCP endpoint

### Phase 2: React Boundary Detection (1 week)

- [ ] Enhance `'use server'` detection for function-level
- [ ] Implement client reachability analysis
- [ ] Add `NEXTJS_BOUNDARY_VIOLATION` rule
- [ ] Add `REACT_SERVER_ACTION_LEAK` rule
- [ ] Test on real Next.js applications

### Phase 3: C# Full Support (3 weeks)

**Week 1:**
- [ ] Integrate tree-sitter-c-sharp (assuming available)
- [ ] Implement C# detector with ASP.NET patterns
- [ ] Add C# dependency detection

**Week 2-3:**
- [ ] Implement 20 C# security rules
- [ ] Add EF Core over-posting detection
- [ ] Add Unity-specific rules
- [ ] Test on real .NET repositories

### Phase 4: Polish & Documentation (1 week)

- [ ] Performance benchmarking
- [ ] False positive tuning
- [ ] User documentation
- [ ] Integration tests

---

## 9. Alternatives Considered

### 9.1 Full Taint Analysis

**Rejected** because:
- O(n²) or worse complexity
- Requires whole-program analysis
- High false positive rates without context
- Doesn't align with Semfora's fast-query philosophy

### 9.2 LLM-Based Detection

**Rejected** because:
- Non-deterministic results
- Cannot guarantee reproducibility
- High latency (seconds vs milliseconds)
- Difficult to explain findings

### 9.3 External SAST Integration

**Rejected** because:
- Adds external dependency
- Loses semantic context we already have
- Cannot leverage call graphs and boundary analysis
- Different performance characteristics

---

## 10. Migration Path

### 10.1 Backward Compatibility

- Security analysis is **additive only**
- No changes to existing schema structures
- New fields use `skip_serializing_if` for empty
- MCP API is extended, not modified

### 10.2 Opt-In Activation

```bash
# Generate index with security analysis
semfora-engine --dir . --shard --with-security

# Query security findings
semfora-engine --search-security --severity=high
```

### 10.3 Gradual Rollout

1. **Alpha**: Security findings stored but not surfaced in default output
2. **Beta**: Security findings in verbose output with confidence levels
3. **GA**: Security findings in standard output with remediation

---

## 11. Appendix: Complete Rule Definitions

See `src/security/rules/` directory (to be created) for complete rule implementations organized by language:

```
src/security/
├── mod.rs              # Module root, detector engine
├── signature.rs        # SecuritySignature implementation
├── categories.rs       # DangerousApiCategory enum
├── boundary.rs         # Server/client boundary analysis
└── rules/
    ├── mod.rs          # Rule registry
    ├── javascript.rs   # JS/TS rules (15)
    ├── rust.rs         # Rust rules (10)
    └── csharp.rs       # C# rules (20+)
```

---

## References

- CVE-2025-55315: ASP.NET Core Kestrel Request Smuggling
- CVE-2024-21907: Newtonsoft.Json DoS via Deep Nesting
- CWE-89: SQL Injection
- CWE-502: Deserialization of Untrusted Data
- CWE-79: Cross-site Scripting (XSS)
- CWE-798: Use of Hard-coded Credentials
- React Server Components RFC
- Next.js App Router Security Considerations
