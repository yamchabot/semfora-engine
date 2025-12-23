//! Semantic model data structures for code analysis

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current schema version for output stability
/// 2.0 - Added layered index support (SEM-45)
/// 2.1 - Two-part hash for uniqueness (file_hash:semantic_hash)
pub const SCHEMA_VERSION: &str = "2.1";

// FNV-1a constants for 64-bit hash
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Compute a stable FNV-1a hash (deterministic across runs and platforms)
///
/// Used for generating stable symbol IDs and repo cache keys.
pub fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Stable symbol identifier for cross-commit tracking
///
/// Uses two-part hash format: `{file_hash}:{semantic_hash}` (25 chars)
/// - file_hash (8 chars): Hash of relative file path - ensures uniqueness
/// - semantic_hash (16 chars): Hash of namespace:symbol:kind:arity - enables move detection
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SymbolId {
    /// Full unique hash: "file_hash:semantic_hash" (25-char string)
    pub hash: String,

    /// Semantic hash only (16-char hex string) for move detection and duplicate finding
    /// Symbols with same semantic_hash have the same signature (name + kind + arity)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub semantic_hash: String,

    /// Module/package namespace (extracted from file structure, NOT full path)
    pub namespace: String,

    /// Symbol name
    pub symbol: String,

    /// Symbol kind
    pub kind: SymbolKind,

    /// Arity (number of arguments/props)
    pub arity: usize,
}

/// Per-symbol semantic information for multi-symbol files
///
/// This captures semantic data for each exported symbol in a file,
/// enabling complete extraction from files with many exports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,

    /// Kind of symbol
    pub kind: SymbolKind,

    /// Start line (1-indexed)
    pub start_line: usize,

    /// End line (1-indexed, inclusive)
    pub end_line: usize,

    /// Whether this symbol is exported
    pub is_exported: bool,

    /// Whether this is a default export
    pub is_default_export: bool,

    /// Whether this is a local variable that escapes its defining scope
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_escape_local: bool,

    /// Stable hash identifier for this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,

    /// Function arguments (for functions/methods)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<Argument>,

    /// Component props (for components)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Prop>,

    /// Return type annotation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,

    /// Function calls within this symbol's body
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<Call>,

    /// Control flow constructs within this symbol
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub control_flow: Vec<ControlFlowChange>,

    /// State changes within this symbol (for components)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_changes: Vec<StateChange>,

    /// Decorators/attributes applied to this symbol
    /// Examples: C# \[HttpGet\], Python @decorator, Java @Annotation
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,

    /// Behavioral risk level for this symbol
    pub behavioral_risk: RiskLevel,

    /// Framework entry point type (if this symbol is invoked by a framework)
    /// Symbols with non-None values should not be flagged as "dead code"
    #[serde(default, skip_serializing_if = "FrameworkEntryPoint::is_none")]
    pub framework_entry_point: FrameworkEntryPoint,
}

impl SymbolInfo {
    /// Create a SymbolId for this symbol given a namespace and file path
    pub fn to_symbol_id(&self, namespace: &str, file_path: &str) -> SymbolId {
        let arity = self.arguments.len() + self.props.len();
        SymbolId::new(namespace, &self.name, self.kind, arity, file_path)
    }

    /// Calculate behavioral risk from calls and control flow
    pub fn calculate_risk(&self) -> RiskLevel {
        let mut score = 0;

        // Control flow complexity
        score += self.control_flow.len().min(3);

        // I/O operations
        for call in &self.calls {
            if Call::check_is_io(&call.name) {
                score += 2;
            }
        }

        // Async without try
        for call in &self.calls {
            if call.is_awaited && !call.in_try {
                score += 1;
            }
        }

        RiskLevel::from_score(score)
    }
}

impl SymbolId {
    /// Create a new SymbolId from components
    ///
    /// The hash is computed as two parts:
    /// - file_hash (8 chars): Hash of the file path for uniqueness
    /// - semantic_hash (16 chars): Hash of namespace:symbol:kind:arity for move detection
    pub fn new(
        namespace: &str,
        symbol: &str,
        kind: SymbolKind,
        arity: usize,
        file_path: &str,
    ) -> Self {
        // Semantic hash (existing formula - for move detection and duplicate finding)
        let semantic_input = format!("{}:{}:{}:{}", namespace, symbol, kind.as_str(), arity);
        let semantic_hash = format!("{:016x}", fnv1a_hash(&semantic_input));

        // File hash (for uniqueness across different files)
        // Truncate to 32 bits (8 hex chars) for compactness
        let file_hash = format!("{:08x}", fnv1a_hash(file_path) as u32);

        // Combined hash: file_hash:semantic_hash
        let hash = format!("{}:{}", file_hash, semantic_hash);

        Self {
            hash,
            semantic_hash,
            namespace: namespace.to_string(),
            symbol: symbol.to_string(),
            kind,
            arity,
        }
    }

    /// Create a SymbolId without file path (for backward compatibility)
    /// NOTE: This produces only a semantic hash, not the two-part format.
    /// Prefer `new()` with file_path when possible.
    #[deprecated(note = "Use new() with file_path for unique hashes")]
    pub fn new_semantic_only(
        namespace: &str,
        symbol: &str,
        kind: SymbolKind,
        arity: usize,
    ) -> Self {
        let semantic_input = format!("{}:{}:{}:{}", namespace, symbol, kind.as_str(), arity);
        let semantic_hash = format!("{:016x}", fnv1a_hash(&semantic_input));

        Self {
            hash: semantic_hash.clone(),
            semantic_hash,
            namespace: namespace.to_string(),
            symbol: symbol.to_string(),
            kind,
            arity,
        }
    }

    /// Extract namespace from a file path
    ///
    /// Converts paths like "src/components/Button.tsx" to "components"
    /// or "src/lib/utils/helpers.ts" to "lib.utils"
    pub fn namespace_from_path(file_path: &str) -> String {
        let path = std::path::Path::new(file_path);

        // Get parent directory components, skip common roots
        let components: Vec<&str> = path
            .parent()
            .map(|p| p.components())
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_os_str().to_str())
            .filter(|&s| !matches!(s, "src" | "lib" | "." | ".." | "app" | "pages"))
            .collect();

        if components.is_empty() {
            // Use filename without extension as fallback
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("root")
                .to_string()
        } else {
            components.join(".")
        }
    }

    /// Create a SymbolId from a SemanticSummary
    pub fn from_summary(summary: &SemanticSummary) -> Option<Self> {
        let symbol = summary.symbol.as_ref()?;
        let kind = summary.symbol_kind.unwrap_or_default();
        let arity = summary.arguments.len() + summary.props.len();
        let namespace = Self::namespace_from_path(&summary.file);

        Some(Self::new(&namespace, symbol, kind, arity, &summary.file))
    }

    /// Extract the semantic hash from a full hash
    ///
    /// Full hash format: "file_hash:semantic_hash" (25 chars)
    /// Returns the semantic_hash portion (16 chars) or the full hash if not in two-part format
    pub fn extract_semantic_hash(full_hash: &str) -> &str {
        full_hash.split(':').nth(1).unwrap_or(full_hash)
    }

    /// Check if a hash is in the two-part format
    pub fn is_two_part_hash(hash: &str) -> bool {
        hash.len() == 25 && hash.chars().nth(8) == Some(':')
    }
}

// ============================================================================
// Surface Deltas for Typed Change Classification
// ============================================================================

/// Typed surface change for safety gates and agent constraints
///
/// Each variant represents a semantic change category that agents
/// can use for policy enforcement and impact assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurfaceDelta {
    /// New state variable introduced
    StateAddition { name: String, state_type: String },
    /// State variable removed
    StateRemoval { name: String },
    /// New dependency/import added
    DependencyAdded { name: String },
    /// Dependency/import removed
    DependencyRemoved { name: String },
    /// Control flow complexity changed
    ControlFlowComplexityChanged { before: usize, after: usize },
    /// Public API surface changed
    PublicApiChanged {
        /// Whether this is a breaking change
        breaking: bool,
    },
    /// Function/method arity changed
    CallArityChanged {
        symbol: String,
        before: usize,
        after: usize,
    },
    /// New persistence operation introduced (database, file, etc.)
    PersistenceIntroduced,
    /// New network operation introduced
    NetworkIntroduced,
    /// Authentication boundary changed
    AuthenticationBoundaryChanged,
    /// Privilege/permission boundary changed
    PrivilegeBoundaryChanged,
    /// New symbol introduced
    SymbolAdded { name: String, kind: SymbolKind },
    /// Symbol removed
    SymbolRemoved { name: String, kind: SymbolKind },
}

/// Semantic diff between two versions of a file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticDiff {
    /// File path
    pub file: String,

    /// Symbol ID (if symbol exists in current version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<SymbolId>,

    /// Typed change deltas
    pub deltas: Vec<SurfaceDelta>,

    /// Risk change (-2 to +2, negative means risk decreased)
    pub risk_change: i8,

    /// Before risk level
    pub risk_before: RiskLevel,

    /// After risk level
    pub risk_after: RiskLevel,
}

impl SemanticDiff {
    /// Create a new SemanticDiff from before/after summaries
    pub fn from_summaries(before: Option<&SemanticSummary>, after: &SemanticSummary) -> Self {
        let mut deltas = Vec::new();

        match before {
            None => {
                // New file - everything is an addition
                if let Some(ref symbol) = after.symbol {
                    deltas.push(SurfaceDelta::SymbolAdded {
                        name: symbol.clone(),
                        kind: after.symbol_kind.unwrap_or_default(),
                    });
                }
                for dep in &after.added_dependencies {
                    deltas.push(SurfaceDelta::DependencyAdded { name: dep.clone() });
                }
                for state in &after.state_changes {
                    deltas.push(SurfaceDelta::StateAddition {
                        name: state.name.clone(),
                        state_type: state.state_type.clone(),
                    });
                }
                // Check for persistence/network in new file
                for insertion in &after.insertions {
                    let lower = insertion.to_lowercase();
                    if lower.contains("database")
                        || lower.contains("storage")
                        || lower.contains("persist")
                    {
                        deltas.push(SurfaceDelta::PersistenceIntroduced);
                        break;
                    }
                }
                for insertion in &after.insertions {
                    let lower = insertion.to_lowercase();
                    if lower.contains("network") || lower.contains("fetch") || lower.contains("api")
                    {
                        deltas.push(SurfaceDelta::NetworkIntroduced);
                        break;
                    }
                }

                Self {
                    file: after.file.clone(),
                    symbol_id: after.symbol_id.clone(),
                    deltas,
                    risk_change: match after.behavioral_risk {
                        RiskLevel::Low => 0,
                        RiskLevel::Medium => 1,
                        RiskLevel::High => 2,
                    },
                    risk_before: RiskLevel::Low,
                    risk_after: after.behavioral_risk,
                }
            }
            Some(before) => {
                // Existing file - compute deltas

                // Symbol changes
                if before.symbol != after.symbol {
                    if let Some(ref old_sym) = before.symbol {
                        deltas.push(SurfaceDelta::SymbolRemoved {
                            name: old_sym.clone(),
                            kind: before.symbol_kind.unwrap_or_default(),
                        });
                    }
                    if let Some(ref new_sym) = after.symbol {
                        deltas.push(SurfaceDelta::SymbolAdded {
                            name: new_sym.clone(),
                            kind: after.symbol_kind.unwrap_or_default(),
                        });
                    }
                }

                // Dependency changes
                let before_deps: std::collections::HashSet<_> =
                    before.added_dependencies.iter().collect();
                let after_deps: std::collections::HashSet<_> =
                    after.added_dependencies.iter().collect();

                for dep in after_deps.difference(&before_deps) {
                    deltas.push(SurfaceDelta::DependencyAdded {
                        name: (*dep).clone(),
                    });
                }
                for dep in before_deps.difference(&after_deps) {
                    deltas.push(SurfaceDelta::DependencyRemoved {
                        name: (*dep).clone(),
                    });
                }

                // State changes
                let before_states: std::collections::HashSet<_> =
                    before.state_changes.iter().map(|s| &s.name).collect();
                let after_states: std::collections::HashSet<_> =
                    after.state_changes.iter().map(|s| &s.name).collect();

                for state_name in after_states.difference(&before_states) {
                    if let Some(state) = after.state_changes.iter().find(|s| &s.name == *state_name)
                    {
                        deltas.push(SurfaceDelta::StateAddition {
                            name: state.name.clone(),
                            state_type: state.state_type.clone(),
                        });
                    }
                }
                for state_name in before_states.difference(&after_states) {
                    deltas.push(SurfaceDelta::StateRemoval {
                        name: (*state_name).clone(),
                    });
                }

                // Control flow complexity
                let cf_before = before.control_flow_changes.len();
                let cf_after = after.control_flow_changes.len();
                if cf_before != cf_after {
                    deltas.push(SurfaceDelta::ControlFlowComplexityChanged {
                        before: cf_before,
                        after: cf_after,
                    });
                }

                // Public API change
                if after.public_surface_changed && !before.public_surface_changed {
                    deltas.push(SurfaceDelta::PublicApiChanged { breaking: true });
                }

                // Arity changes
                let before_arity = before.arguments.len() + before.props.len();
                let after_arity = after.arguments.len() + after.props.len();
                if before_arity != after_arity {
                    if let Some(ref symbol) = after.symbol {
                        deltas.push(SurfaceDelta::CallArityChanged {
                            symbol: symbol.clone(),
                            before: before_arity,
                            after: after_arity,
                        });
                    }
                }

                // Check for new persistence/network
                let before_has_persistence = before.insertions.iter().any(|i| {
                    let l = i.to_lowercase();
                    l.contains("database") || l.contains("storage") || l.contains("persist")
                });
                let after_has_persistence = after.insertions.iter().any(|i| {
                    let l = i.to_lowercase();
                    l.contains("database") || l.contains("storage") || l.contains("persist")
                });
                if after_has_persistence && !before_has_persistence {
                    deltas.push(SurfaceDelta::PersistenceIntroduced);
                }

                let before_has_network = before.insertions.iter().any(|i| {
                    let l = i.to_lowercase();
                    l.contains("network") || l.contains("fetch") || l.contains("api")
                });
                let after_has_network = after.insertions.iter().any(|i| {
                    let l = i.to_lowercase();
                    l.contains("network") || l.contains("fetch") || l.contains("api")
                });
                if after_has_network && !before_has_network {
                    deltas.push(SurfaceDelta::NetworkIntroduced);
                }

                // Calculate risk change
                let risk_before_val: i8 = match before.behavioral_risk {
                    RiskLevel::Low => 0,
                    RiskLevel::Medium => 1,
                    RiskLevel::High => 2,
                };
                let risk_after_val: i8 = match after.behavioral_risk {
                    RiskLevel::Low => 0,
                    RiskLevel::Medium => 1,
                    RiskLevel::High => 2,
                };

                Self {
                    file: after.file.clone(),
                    symbol_id: after.symbol_id.clone(),
                    deltas,
                    risk_change: risk_after_val - risk_before_val,
                    risk_before: before.behavioral_risk,
                    risk_after: after.behavioral_risk,
                }
            }
        }
    }
}

// ============================================================================
// Repository Overview
// ============================================================================

/// Repository overview for whole-repo analysis
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RepoOverview {
    /// Detected framework (Next.js, React, Express, etc.)
    pub framework: Option<String>,

    /// Detected database/ORM (Drizzle, Prisma, etc.)
    pub database: Option<String>,

    /// Package manager (npm, pnpm, yarn, cargo, etc.)
    pub package_manager: Option<String>,

    /// Detected patterns/architectures
    pub patterns: Vec<String>,

    /// Module groups (files organized by directory/purpose)
    pub modules: Vec<ModuleGroup>,

    /// Key entry points
    pub entry_points: Vec<String>,

    /// Internal data flow (file -> files it imports from)
    pub data_flow: HashMap<String, Vec<String>>,

    /// Total statistics
    pub stats: RepoStats,
}

/// A group of related files (by directory or purpose)
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ModuleGroup {
    /// Module name/path
    pub name: String,

    /// Purpose description
    pub purpose: String,

    /// Number of files
    pub file_count: usize,

    /// Risk level for this module
    pub risk: RiskLevel,

    /// Key files in this module
    pub key_files: Vec<String>,
}

/// Repository statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    /// Total files analyzed
    pub total_files: usize,

    /// Total lines of code
    pub total_lines: usize,

    /// Risk breakdown
    pub high_risk: usize,
    pub medium_risk: usize,
    pub low_risk: usize,

    /// Files by language
    pub by_language: HashMap<String, usize>,

    /// Total API endpoints
    pub api_endpoints: usize,

    /// Total database tables
    pub database_tables: usize,

    /// Total React components
    pub components: usize,
}

/// Complete semantic summary of a file
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SemanticSummary {
    /// File path
    pub file: String,

    /// Language name
    pub language: String,

    /// Stable symbol identifier for cross-commit tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<SymbolId>,

    /// Primary symbol name (function, class, component)
    pub symbol: Option<String>,

    /// Kind of the primary symbol
    pub symbol_kind: Option<SymbolKind>,

    /// All symbols in this file (for multi-symbol files)
    ///
    /// This captures every exported symbol, solving the "single symbol per file"
    /// limitation. Each SymbolInfo contains full semantic data for that symbol.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<SymbolInfo>,

    /// Start line of the primary symbol (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,

    /// End line of the primary symbol (1-indexed, inclusive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,

    /// Component props (for React/Vue components)
    pub props: Vec<Prop>,

    /// Function arguments
    pub arguments: Vec<Argument>,

    /// Return type annotation
    pub return_type: Option<String>,

    /// Descriptive insertions (rule-based summaries)
    pub insertions: Vec<String>,

    /// Added imports/dependencies
    pub added_dependencies: Vec<String>,

    /// Local file imports (for data flow tracking)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_imports: Vec<String>,

    /// Mapping of imported symbol names to their source packages
    /// e.g., {"useState": "react", "ChevronDown": "lucide-react"}
    /// Used for tracking which external package each external call comes from
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub import_sources: HashMap<String, String>,

    /// State variable changes
    pub state_changes: Vec<StateChange>,

    /// Control flow changes
    pub control_flow_changes: Vec<ControlFlowChange>,

    /// Function calls detected
    pub calls: Vec<Call>,

    /// Whether the public API surface changed
    pub public_surface_changed: bool,

    /// Behavioral risk level
    pub behavioral_risk: RiskLevel,

    /// Raw source fallback (for incomplete extraction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_fallback: Option<String>,

    /// Framework entry point type for the primary symbol
    /// Symbols with non-None values should not be flagged as "dead code"
    #[serde(default, skip_serializing_if = "FrameworkEntryPoint::is_none")]
    pub framework_entry_point: FrameworkEntryPoint,

    /// Whether extraction was complete
    #[serde(skip)]
    pub extraction_complete: bool,
}

/// Kind of symbol being analyzed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    /// Regular function
    #[default]
    Function,
    /// React/Vue component
    Component,
    /// Class definition
    Class,
    /// Method inside a class/impl
    Method,
    /// TypeScript/Java interface
    Interface,
    /// Rust trait
    Trait,
    /// Rust/Go struct
    Struct,
    /// Enum definition
    Enum,
    /// Module/namespace
    Module,
    /// Type alias
    TypeAlias,
    /// Variable declaration (module-level constant, static, or class field)
    Variable,
}

impl SymbolKind {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Component => "component",
            Self::Class => "class",
            Self::Method => "method",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Module => "module",
            Self::TypeAlias => "type_alias",
            Self::Variable => "variable",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "function" | "fn" => Self::Function,
            "component" => Self::Component,
            "class" => Self::Class,
            "method" => Self::Method,
            "interface" => Self::Interface,
            "trait" => Self::Trait,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "module" | "mod" => Self::Module,
            "type_alias" | "type" => Self::TypeAlias,
            "variable" | "var" | "const" | "static" | "field" => Self::Variable,
            _ => Self::Function, // Default fallback
        }
    }
}

/// Framework entry point type - symbols invoked by frameworks, not application code
///
/// These symbols appear to have "no callers" in static analysis but are actually
/// invoked by the framework's runtime (routing, dependency injection, lifecycle hooks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FrameworkEntryPoint {
    /// Not a framework entry point (default)
    #[default]
    None,

    // === Next.js Entry Points ===
    /// Next.js page component (pages/ or app/ directory)
    NextPage,
    /// Next.js layout component (app/ directory)
    NextLayout,
    /// Next.js API route handler (GET, POST, etc.)
    NextApiRoute,
    /// Next.js special file (_app, _document, _error)
    NextSpecialFile,
    /// Next.js SSR data fetching (getServerSideProps, getStaticProps, getInitialProps)
    NextDataFetching,
    /// Next.js middleware
    NextMiddleware,

    // === NestJS Entry Points ===
    /// NestJS @Injectable() decorated service
    NestService,
    /// NestJS @Controller() decorated controller
    NestController,
    /// NestJS @Module() decorated module
    NestModule,
    /// NestJS bootstrap function (main.ts entry point)
    NestBootstrap,

    // === Express/Koa Entry Points ===
    /// Express/Koa route handler
    ExpressRoute,
    /// Express middleware function
    ExpressMiddleware,

    // === React Entry Points ===
    /// React component exported from index file
    ReactRootComponent,
    /// React useState/useReducer state variable
    ReactState,
    /// React Context created via createContext
    ReactContext,

    // === Redux Entry Points ===
    /// Redux store configuration (configureStore/createStore)
    ReduxStore,
    /// Redux reducer function
    ReduxReducer,
    /// Redux Toolkit slice (createSlice)
    ReduxSlice,
    /// Redux async thunk (createAsyncThunk)
    ReduxThunk,
    /// Redux selector function
    ReduxSelector,

    // === Generic Entry Points ===
    /// CLI entry point (main function)
    CliMain,
    /// Test function
    TestFunction,
    /// Exported from package index
    PackageExport,
}

impl FrameworkEntryPoint {
    /// Check if this is None (for serde skip_serializing_if)
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Check if this is a framework entry point (not None)
    pub fn is_entry_point(&self) -> bool {
        !self.is_none()
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::None => "not a framework entry point",
            Self::NextPage => "Next.js page component",
            Self::NextLayout => "Next.js layout component",
            Self::NextApiRoute => "Next.js API route handler",
            Self::NextSpecialFile => "Next.js special file (_app/_document/_error)",
            Self::NextDataFetching => "Next.js SSR data fetching function",
            Self::NextMiddleware => "Next.js middleware",
            Self::NestService => "NestJS injectable service",
            Self::NestController => "NestJS controller",
            Self::NestModule => "NestJS module",
            Self::NestBootstrap => "NestJS bootstrap entry point",
            Self::ExpressRoute => "Express route handler",
            Self::ExpressMiddleware => "Express middleware",
            Self::ReactRootComponent => "React root component",
            Self::ReactState => "React useState/useReducer state variable",
            Self::ReactContext => "React context via createContext",
            Self::ReduxStore => "Redux store configuration",
            Self::ReduxReducer => "Redux reducer function",
            Self::ReduxSlice => "Redux Toolkit slice",
            Self::ReduxThunk => "Redux async thunk",
            Self::ReduxSelector => "Redux selector function",
            Self::CliMain => "CLI main entry point",
            Self::TestFunction => "test function",
            Self::PackageExport => "package export",
        }
    }
}

/// Component prop definition
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prop {
    /// Prop name
    pub name: String,

    /// Type annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop_type: Option<String>,

    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,

    /// Whether the prop is required
    pub required: bool,
}

/// Function argument definition
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    /// Argument name
    pub name: String,

    /// Type annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_type: Option<String>,

    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// State variable change
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    /// Variable name
    pub name: String,

    /// Variable type
    pub state_type: String,

    /// Initializer expression
    pub initializer: String,
}

/// Control flow change
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFlowChange {
    /// Kind of control flow
    pub kind: ControlFlowKind,

    /// Location in source
    pub location: Location,

    /// Nesting depth (0 = top level, 1 = inside one control structure, etc.)
    /// Used for cognitive complexity calculation
    #[serde(default, skip_serializing_if = "is_zero")]
    pub nesting_depth: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Kind of control flow construct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlFlowKind {
    /// If statement/expression
    #[default]
    If,
    /// For loop
    For,
    /// While loop
    While,
    /// Switch statement
    Switch,
    /// Match expression (Rust)
    Match,
    /// Try-catch block
    Try,
    /// Infinite loop (Rust)
    Loop,
}

impl ControlFlowKind {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::If => "if",
            Self::For => "for",
            Self::While => "while",
            Self::Switch => "switch",
            Self::Match => "match",
            Self::Try => "try",
            Self::Loop => "loop",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "if" => Self::If,
            "for" => Self::For,
            "while" => Self::While,
            "switch" => Self::Switch,
            "match" => Self::Match,
            "try" => Self::Try,
            "loop" => Self::Loop,
            _ => Self::If, // Default fallback
        }
    }
}

/// Source code location
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Line number (1-indexed)
    pub line: usize,

    /// Column number (0-indexed)
    pub column: usize,
}

impl Location {
    /// Create a new location
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Behavioral risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low risk (0-1 points)
    #[default]
    Low,
    /// Medium risk (2-3 points)
    Medium,
    /// High risk (4+ points)
    High,
}

impl RiskLevel {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Calculate risk level from a score
    pub fn from_score(score: usize) -> Self {
        match score {
            0..=1 => Self::Low,
            2..=3 => Self::Medium,
            _ => Self::High,
        }
    }

    /// Parse risk level from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "high" => Self::High,
            "medium" => Self::Medium,
            _ => Self::Low,
        }
    }
}

/// JSX element for insertion rule processing
#[derive(Debug, Clone, Default)]
pub struct JsxElement {
    /// Tag name
    pub tag: String,

    /// Props/attributes
    pub props: Vec<(String, Option<String>)>,

    /// Whether it's self-closing
    pub is_self_closing: bool,

    /// Source location
    pub location: Location,
}

/// Kind of reference for variable tracking
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RefKind {
    /// Not a variable reference (regular function call)
    #[default]
    None,
    /// Reading a variable's value
    Read,
    /// Writing/assigning to a variable
    Write,
    /// Both reading and writing (e.g., x += 1)
    ReadWrite,
    /// Reading a local variable that escapes its scope (passed/returned)
    EscapeRead,
    /// Writing a local variable that escapes its scope
    EscapeWrite,
    /// Read+write on a local variable that escapes its scope
    EscapeReadWrite,
}

impl RefKind {
    /// Returns true if this is any kind of variable reference
    pub fn is_variable_ref(&self) -> bool {
        !matches!(self, RefKind::None)
    }

    /// Returns true if this is a local-escape variable reference
    pub fn is_escape_ref(&self) -> bool {
        matches!(
            self,
            RefKind::EscapeRead | RefKind::EscapeWrite | RefKind::EscapeReadWrite
        )
    }

    /// Returns the edge kind string for SQLite export
    pub fn as_edge_kind(&self) -> &'static str {
        match self {
            RefKind::None => "call",
            RefKind::Read => "read",
            RefKind::Write => "write",
            RefKind::ReadWrite => "readwrite",
            RefKind::EscapeRead => "escape_read",
            RefKind::EscapeWrite => "escape_write",
            RefKind::EscapeReadWrite => "escape_readwrite",
        }
    }

    /// Parse from edge kind string (for call graph format)
    pub fn from_edge_kind(s: &str) -> Self {
        match s {
            "read" => RefKind::Read,
            "write" => RefKind::Write,
            "readwrite" => RefKind::ReadWrite,
            "escape_read" => RefKind::EscapeRead,
            "escape_write" => RefKind::EscapeWrite,
            "escape_readwrite" => RefKind::EscapeReadWrite,
            _ => RefKind::None, // "call" or any other value
        }
    }
}

/// Edge in call graph with edge kind information
///
/// Used for call graph storage and SQLite export. Includes
/// edge_kind to distinguish function calls from variable references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallGraphEdge {
    /// Target symbol hash (callee)
    pub callee: String,
    /// Kind of edge: call (function), read, write, or readwrite (variable)
    pub edge_kind: RefKind,
}

impl CallGraphEdge {
    /// Create a new call graph edge
    pub fn new(callee: String, edge_kind: RefKind) -> Self {
        Self { callee, edge_kind }
    }

    /// Create a function call edge (edge_kind = None/call)
    pub fn call(callee: String) -> Self {
        Self {
            callee,
            edge_kind: RefKind::None,
        }
    }

    /// Encode to string for call graph format: "hash" or "hash:kind"
    pub fn encode(&self) -> String {
        if self.edge_kind == RefKind::None {
            format!("\"{}\"", self.callee)
        } else {
            format!("\"{}:{}\"", self.callee, self.edge_kind.as_edge_kind())
        }
    }

    /// Decode from string: "hash" or "hash:kind"
    pub fn decode(s: &str) -> Self {
        let s = s.trim().trim_matches('"');
        if let Some(colon_pos) = s.rfind(':') {
            // Check if the suffix after the last colon is an edge kind
            let suffix = &s[colon_pos + 1..];
            if matches!(
                suffix,
                "read"
                    | "write"
                    | "readwrite"
                    | "escape_read"
                    | "escape_write"
                    | "escape_readwrite"
                    | "call"
            ) {
                return Self {
                    callee: s[..colon_pos].to_string(),
                    edge_kind: RefKind::from_edge_kind(suffix),
                };
            }
        }
        // No edge kind suffix - default to call
        Self {
            callee: s.to_string(),
            edge_kind: RefKind::None,
        }
    }
}

/// Function/method call for analysis
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// Function/method name
    pub name: String,

    /// Object for method calls (e.g., "console" for console.log)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,

    /// Whether this call is awaited
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_awaited: bool,

    /// Whether this call is inside a try block
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub in_try: bool,

    /// Whether this is a React hook
    #[serde(skip)]
    pub is_hook: bool,

    /// Whether this is an I/O operation
    #[serde(skip)]
    pub is_io: bool,

    /// Kind of variable reference (None = function call, Read/Write/ReadWrite = variable)
    #[serde(skip)]
    pub ref_kind: RefKind,

    /// Source location
    #[serde(skip)]
    pub location: Location,
}

impl Call {
    /// Check if this call is a React hook based on naming convention
    pub fn check_is_hook(name: &str) -> bool {
        name.starts_with("use")
            && name
                .chars()
                .nth(3)
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
    }

    /// Check if this call is an I/O operation
    pub fn check_is_io(name: &str) -> bool {
        matches!(
            name,
            "fetch"
                | "invoke"
                | "axios"
                | "request"
                | "get"
                | "post"
                | "put"
                | "delete"
                | "open"
                | "read"
                | "write"
                | "readFile"
                | "writeFile"
                | "readFileSync"
                | "writeFileSync"
        )
    }
}

/// Import statement
#[derive(Debug, Clone, Default)]
pub struct Import {
    /// Module source path
    pub source: String,

    /// Imported names
    pub names: Vec<ImportedName>,

    /// Whether this is a default import
    pub is_default: bool,

    /// Whether this is a namespace import
    pub is_namespace: bool,
}

/// Individual imported name
#[derive(Debug, Clone, Default)]
pub struct ImportedName {
    /// Original name
    pub name: String,

    /// Alias (if renamed)
    pub alias: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(1), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(2), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(3), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(4), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(10), RiskLevel::High);
    }

    #[test]
    fn test_call_is_hook() {
        assert!(Call::check_is_hook("useState"));
        assert!(Call::check_is_hook("useEffect"));
        assert!(Call::check_is_hook("useCallback"));
        assert!(!Call::check_is_hook("use")); // Too short
        assert!(!Call::check_is_hook("usestuff")); // Lowercase after "use"
        assert!(!Call::check_is_hook("fetch"));
    }

    #[test]
    fn test_call_is_io() {
        assert!(Call::check_is_io("fetch"));
        assert!(Call::check_is_io("invoke"));
        assert!(Call::check_is_io("readFile"));
        assert!(!Call::check_is_io("useState"));
        assert!(!Call::check_is_io("map"));
    }

    #[test]
    fn test_symbol_kind_str() {
        assert_eq!(SymbolKind::Function.as_str(), "function");
        assert_eq!(SymbolKind::Component.as_str(), "component");
        assert_eq!(SymbolKind::Class.as_str(), "class");
    }

    #[test]
    fn test_control_flow_kind_str() {
        assert_eq!(ControlFlowKind::If.as_str(), "if");
        assert_eq!(ControlFlowKind::For.as_str(), "for");
        assert_eq!(ControlFlowKind::Match.as_str(), "match");
    }

    #[test]
    fn test_symbol_id_creation() {
        let id = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            3,
            "src/components/Button.tsx",
        );
        assert_eq!(id.namespace, "components");
        assert_eq!(id.symbol, "Button");
        assert_eq!(id.kind, SymbolKind::Component);
        assert_eq!(id.arity, 3);
        // Two-part hash format: "file_hash:semantic_hash" (25 chars)
        assert_eq!(id.hash.len(), 25);
        assert!(id.hash.contains(':'));
        // Semantic hash should be 16 chars
        assert_eq!(id.semantic_hash.len(), 16);
    }

    #[test]
    fn test_symbol_id_deterministic() {
        // Same inputs should always produce the same hash
        let id1 = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            3,
            "src/components/Button.tsx",
        );
        let id2 = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            3,
            "src/components/Button.tsx",
        );
        assert_eq!(id1.hash, id2.hash);
        assert_eq!(id1.semantic_hash, id2.semantic_hash);

        // Different arity should produce different hashes
        let id3 = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            4,
            "src/components/Button.tsx",
        );
        assert_ne!(id1.hash, id3.hash);
        assert_ne!(id1.semantic_hash, id3.semantic_hash);
    }

    #[test]
    fn test_symbol_id_unique_per_file() {
        // Same signature in different files should have different full hashes
        let id1 = SymbolId::new(
            "frameworks",
            "enhance",
            SymbolKind::Function,
            3,
            "src/frameworks/nextjs.rs",
        );
        let id2 = SymbolId::new(
            "frameworks",
            "enhance",
            SymbolKind::Function,
            3,
            "src/frameworks/react.rs",
        );
        let id3 = SymbolId::new(
            "frameworks",
            "enhance",
            SymbolKind::Function,
            3,
            "src/frameworks/vue.rs",
        );

        // Full hashes should be unique (different files)
        assert_ne!(id1.hash, id2.hash);
        assert_ne!(id2.hash, id3.hash);
        assert_ne!(id1.hash, id3.hash);

        // Semantic hashes should be the same (same signature)
        assert_eq!(id1.semantic_hash, id2.semantic_hash);
        assert_eq!(id2.semantic_hash, id3.semantic_hash);
    }

    #[test]
    fn test_namespace_from_path() {
        // Standard component path
        assert_eq!(
            SymbolId::namespace_from_path("src/components/Button.tsx"),
            "components"
        );

        // Nested path
        assert_eq!(
            SymbolId::namespace_from_path("src/lib/utils/helpers.ts"),
            "utils"
        );

        // Root level file (uses filename)
        assert_eq!(SymbolId::namespace_from_path("src/main.rs"), "main");

        // Deep nesting
        assert_eq!(
            SymbolId::namespace_from_path("src/features/auth/components/LoginForm.tsx"),
            "features.auth.components"
        );
    }

    #[test]
    fn test_symbol_id_from_summary() {
        let summary = SemanticSummary {
            file: "src/components/Button.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("Button".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            props: vec![
                Prop {
                    name: "onClick".to_string(),
                    ..Default::default()
                },
                Prop {
                    name: "children".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let id = SymbolId::from_summary(&summary).unwrap();
        assert_eq!(id.namespace, "components");
        assert_eq!(id.symbol, "Button");
        assert_eq!(id.kind, SymbolKind::Component);
        assert_eq!(id.arity, 2); // 2 props
    }

    #[test]
    fn test_symbol_id_file_move_detection() {
        // Moving a file changes the full hash but keeps semantic_hash the same
        let id1 = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            2,
            "src/components/Button.tsx",
        );
        let id2 = SymbolId::new(
            "components",
            "Button",
            SymbolKind::Component,
            2,
            "lib/components/Button.tsx",
        );

        // Full hashes should differ (different file paths)
        assert_ne!(
            id1.hash, id2.hash,
            "Full hash should change when file moves"
        );

        // Semantic hashes should match (same signature - can detect as potential move)
        assert_eq!(
            id1.semantic_hash, id2.semantic_hash,
            "Semantic hash should match for potential move detection"
        );
    }

    #[test]
    fn test_extract_semantic_hash() {
        // Two-part hash
        assert_eq!(
            SymbolId::extract_semantic_hash("a1b2c3d4:8e021ca5a492c67d"),
            "8e021ca5a492c67d"
        );

        // Legacy single-part hash (backward compatibility)
        assert_eq!(
            SymbolId::extract_semantic_hash("8e021ca5a492c67d"),
            "8e021ca5a492c67d"
        );
    }

    #[test]
    fn test_is_two_part_hash() {
        assert!(SymbolId::is_two_part_hash("a1b2c3d4:8e021ca5a492c67d"));
        assert!(!SymbolId::is_two_part_hash("8e021ca5a492c67d")); // Legacy 16-char hash
        assert!(!SymbolId::is_two_part_hash("short"));
    }

    #[test]
    fn test_semantic_diff_new_file() {
        let after = SemanticSummary {
            file: "src/components/Button.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("Button".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            added_dependencies: vec!["useState".to_string(), "useEffect".to_string()],
            state_changes: vec![StateChange {
                name: "open".to_string(),
                state_type: "boolean".to_string(),
                initializer: "false".to_string(),
            }],
            behavioral_risk: RiskLevel::Medium,
            ..Default::default()
        };

        let diff = SemanticDiff::from_summaries(None, &after);

        assert_eq!(diff.file, "src/components/Button.tsx");
        assert_eq!(diff.risk_before, RiskLevel::Low);
        assert_eq!(diff.risk_after, RiskLevel::Medium);
        assert_eq!(diff.risk_change, 1);

        // Check for expected deltas
        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::SymbolAdded { name, .. } if name == "Button")));
        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::DependencyAdded { name } if name == "useState")));
        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::StateAddition { name, .. } if name == "open")));
    }

    #[test]
    fn test_semantic_diff_modified_file() {
        let before = SemanticSummary {
            file: "src/components/Button.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("Button".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            added_dependencies: vec!["useState".to_string()],
            state_changes: vec![StateChange {
                name: "open".to_string(),
                state_type: "boolean".to_string(),
                initializer: "false".to_string(),
            }],
            behavioral_risk: RiskLevel::Low,
            ..Default::default()
        };

        let after = SemanticSummary {
            file: "src/components/Button.tsx".to_string(),
            language: "tsx".to_string(),
            symbol: Some("Button".to_string()),
            symbol_kind: Some(SymbolKind::Component),
            added_dependencies: vec!["useState".to_string(), "useCallback".to_string()],
            state_changes: vec![
                StateChange {
                    name: "open".to_string(),
                    state_type: "boolean".to_string(),
                    initializer: "false".to_string(),
                },
                StateChange {
                    name: "count".to_string(),
                    state_type: "number".to_string(),
                    initializer: "0".to_string(),
                },
            ],
            control_flow_changes: vec![ControlFlowChange {
                kind: ControlFlowKind::If,
                location: Location::default(),
                nesting_depth: 0,
            }],
            behavioral_risk: RiskLevel::Medium,
            ..Default::default()
        };

        let diff = SemanticDiff::from_summaries(Some(&before), &after);

        assert_eq!(diff.risk_before, RiskLevel::Low);
        assert_eq!(diff.risk_after, RiskLevel::Medium);
        assert_eq!(diff.risk_change, 1);

        // Check for expected deltas
        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::DependencyAdded { name } if name == "useCallback")));
        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::StateAddition { name, .. } if name == "count")));
        assert!(diff.deltas.iter().any(|d| matches!(
            d,
            SurfaceDelta::ControlFlowComplexityChanged {
                before: 0,
                after: 1
            }
        )));

        // Should NOT have symbol added (same symbol)
        assert!(!diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::SymbolAdded { .. })));
    }

    #[test]
    fn test_semantic_diff_dependency_removal() {
        let before = SemanticSummary {
            file: "test.ts".to_string(),
            language: "ts".to_string(),
            added_dependencies: vec!["foo".to_string(), "bar".to_string()],
            ..Default::default()
        };

        let after = SemanticSummary {
            file: "test.ts".to_string(),
            language: "ts".to_string(),
            added_dependencies: vec!["bar".to_string()],
            ..Default::default()
        };

        let diff = SemanticDiff::from_summaries(Some(&before), &after);

        assert!(diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::DependencyRemoved { name } if name == "foo")));
        assert!(!diff
            .deltas
            .iter()
            .any(|d| matches!(d, SurfaceDelta::DependencyRemoved { name } if name == "bar")));
    }
}
