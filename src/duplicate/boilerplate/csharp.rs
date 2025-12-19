//! C# boilerplate pattern detection
//!
//! This module contains detection functions for common C# boilerplate patterns
//! that should be excluded from duplicate detection.
//!
//! # Patterns Detected
//!
//! ## ASP.NET Core (4 patterns)
//! - **AspNetController**: Controller action methods (\[HttpGet\], \[HttpPost\], IActionResult)
//! - **AspNetMinimalApi**: Minimal API endpoints (app.MapGet, app.MapPost)
//! - **AspNetMiddleware**: Middleware patterns (Invoke, InvokeAsync)
//! - **AspNetDI**: DI registrations (services.AddScoped, AddSingleton, AddTransient)
//!
//! ## Entity Framework (4 patterns)
//! - **EFDbContext**: DbContext boilerplate (OnConfiguring, OnModelCreating)
//! - **EFDbSet**: DbSet property declarations
//! - **EFFluentApi**: Fluent API configuration (HasKey, HasOne, HasMany)
//! - **EFMigration**: Migration patterns (Up, Down methods)
//!
//! ## Testing (3 patterns)
//! - **XUnitTest**: xUnit test methods (\[Fact\], \[Theory\])
//! - **NUnitTest**: NUnit test methods (\[Test\], \[TestCase\])
//! - **MoqSetup**: Moq mock setup patterns
//!
//! ## LINQ (2 patterns)
//! - **LinqChain**: LINQ Select/Where/GroupBy boilerplate chains
//! - **LinqProjection**: LINQ projection-only pipelines
//!
//! ## Unity (3 patterns)
//! - **UnityLifecycle**: MonoBehaviour lifecycle methods (Start, Update, Awake)
//! - **UnitySerializedField**: \[SerializeField\] field patterns
//! - **UnityScriptableObject**: ScriptableObject configs (CreateAssetMenu)
//!
//! ## General C# (2 patterns)
//! - **CSharpProperty**: Auto-property accessor (get; set;)
//! - **CSharpRecord**: Record types (primary constructors)

use super::{BoilerplateCategory, PatternMatcher};
use crate::lang::Lang;
use crate::schema::SymbolInfo;

/// All C# boilerplate patterns
///
/// Order matters! Patterns are checked in order, so more specific patterns
/// should come before more general ones.
pub static PATTERNS: &[PatternMatcher] = &[
    // ==========================================================================
    // Testing Patterns (most specific - check first)
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::XUnitTest,
        languages: &[Lang::CSharp],
        detector: is_xunit_test,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::NUnitTest,
        languages: &[Lang::CSharp],
        detector: is_nunit_test,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::MoqSetup,
        languages: &[Lang::CSharp],
        detector: is_moq_setup,
        enabled_by_default: true,
    },
    // ==========================================================================
    // Unity Patterns (framework-specific)
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::UnityLifecycle,
        languages: &[Lang::CSharp],
        detector: is_unity_lifecycle,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::UnitySerializedField,
        languages: &[Lang::CSharp],
        detector: is_unity_serialized_field,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::UnityScriptableObject,
        languages: &[Lang::CSharp],
        detector: is_unity_scriptable_object,
        enabled_by_default: true,
    },
    // ==========================================================================
    // ASP.NET Core Patterns
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::AspNetController,
        languages: &[Lang::CSharp],
        detector: is_aspnet_controller,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::AspNetMinimalApi,
        languages: &[Lang::CSharp],
        detector: is_aspnet_minimal_api,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::AspNetMiddleware,
        languages: &[Lang::CSharp],
        detector: is_aspnet_middleware,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::AspNetDI,
        languages: &[Lang::CSharp],
        detector: is_aspnet_di,
        enabled_by_default: true,
    },
    // ==========================================================================
    // Entity Framework Patterns
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::EFDbContext,
        languages: &[Lang::CSharp],
        detector: is_ef_dbcontext,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::EFDbSet,
        languages: &[Lang::CSharp],
        detector: is_ef_dbset,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::EFFluentApi,
        languages: &[Lang::CSharp],
        detector: is_ef_fluent_api,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::EFMigration,
        languages: &[Lang::CSharp],
        detector: is_ef_migration,
        enabled_by_default: true,
    },
    // ==========================================================================
    // LINQ Patterns
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::LinqChain,
        languages: &[Lang::CSharp],
        detector: is_linq_chain,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::LinqProjection,
        languages: &[Lang::CSharp],
        detector: is_linq_projection,
        enabled_by_default: true,
    },
    // ==========================================================================
    // General C# Patterns (most generic - check last)
    // ==========================================================================
    PatternMatcher {
        category: BoilerplateCategory::CSharpProperty,
        languages: &[Lang::CSharp],
        detector: is_csharp_property,
        enabled_by_default: true,
    },
    PatternMatcher {
        category: BoilerplateCategory::CSharpRecord,
        languages: &[Lang::CSharp],
        detector: is_csharp_record,
        enabled_by_default: true,
    },
];

// =============================================================================
// Detection Functions - Testing
// =============================================================================

/// xUnit test: \[Fact\], \[Theory\] attributes or test naming patterns with Assert calls
pub fn is_xunit_test(info: &SymbolInfo) -> bool {
    // Check for xUnit attributes in decorators
    let has_xunit_attr = info.decorators.iter().any(|d| {
        let d_lower = d.to_lowercase();
        d_lower.contains("fact") || d_lower.contains("theory") || d_lower.contains("inlinedata")
    });

    if has_xunit_attr {
        return true;
    }

    // Fallback: Check for test naming patterns with Assert calls
    let name = &info.name;
    let is_test_name = name.starts_with("Test")
        || name.ends_with("Test")
        || name.ends_with("Tests")
        || name.contains("Should")
        || name.contains("_Should_")
        || name.contains("_When_")
        || name.contains("_Given_")
        || name.contains("_Then_");

    if !is_test_name {
        return false;
    }

    // Should have xUnit-style Assert calls
    let has_xunit_assert = info.calls.iter().any(|c| {
        matches!(
            c.name.as_str(),
            "Equal"
                | "NotEqual"
                | "True"
                | "False"
                | "Null"
                | "NotNull"
                | "Empty"
                | "NotEmpty"
                | "Contains"
                | "DoesNotContain"
                | "Throws"
                | "ThrowsAsync"
                | "ThrowsAny"
                | "ThrowsAnyAsync"
                | "Same"
                | "NotSame"
                | "IsType"
                | "IsNotType"
                | "InRange"
                | "NotInRange"
                | "All"
                | "Collection"
        ) || c.object.as_ref().map_or(false, |o| o == "Assert")
    });

    has_xunit_assert
}

/// NUnit test: \[Test\], \[TestCase\] attributes or Assert.That/AreEqual calls
pub fn is_nunit_test(info: &SymbolInfo) -> bool {
    // Check for NUnit attributes
    let has_nunit_attr = info.decorators.iter().any(|d| {
        let d_lower = d.to_lowercase();
        d_lower.contains("test")
            || d_lower.contains("testcase")
            || d_lower.contains("testfixture")
            || d_lower.contains("setup")
            || d_lower.contains("teardown")
    });

    if has_nunit_attr {
        return true;
    }

    // Fallback: Check for NUnit-style Assert calls
    let has_nunit_assert = info.calls.iter().any(|c| {
        matches!(
            c.name.as_str(),
            "That"
                | "AreEqual"
                | "AreNotEqual"
                | "AreSame"
                | "AreNotSame"
                | "IsTrue"
                | "IsFalse"
                | "IsNull"
                | "IsNotNull"
                | "IsEmpty"
                | "IsNotEmpty"
                | "Greater"
                | "Less"
                | "GreaterOrEqual"
                | "LessOrEqual"
                | "Throws"
                | "DoesNotThrow"
                | "Pass"
                | "Fail"
                | "Ignore"
                | "Inconclusive"
        ) || c.object.as_ref().map_or(false, |o| o == "Assert")
    });

    // Check for test naming patterns
    let name = &info.name;
    let is_test_name = name.starts_with("Test")
        || name.ends_with("Test")
        || name.ends_with("Tests")
        || name.contains("Should");

    // Either has NUnit asserts with test-like name, or lots of NUnit-specific calls
    (has_nunit_assert && is_test_name) || (has_nunit_assert && info.calls.len() <= 5)
}

/// Moq setup: Dominant calls to Setup/Returns/Verify on mock objects
pub fn is_moq_setup(info: &SymbolInfo) -> bool {
    if info.calls.is_empty() {
        return false;
    }

    let moq_calls = [
        "Setup",
        "SetupGet",
        "SetupSet",
        "SetupSequence",
        "Returns",
        "ReturnsAsync",
        "Throws",
        "ThrowsAsync",
        "Verify",
        "VerifyAll",
        "VerifyNoOtherCalls",
        "Callback",
        "CallBase",
    ];

    let moq_call_count = info
        .calls
        .iter()
        .filter(|c| {
            moq_calls.contains(&c.name.as_str())
                || c.object
                    .as_ref()
                    .map_or(false, |o| o.contains("Mock") || o.contains("mock"))
        })
        .count();

    // At least 60% of calls should be Moq-related
    let ratio = moq_call_count as f32 / info.calls.len() as f32;
    ratio >= 0.6 && info.control_flow.len() <= 1
}

// =============================================================================
// Detection Functions - Unity
// =============================================================================

/// Unity lifecycle: Start, Update, Awake, OnEnable, OnDestroy, etc.
pub fn is_unity_lifecycle(info: &SymbolInfo) -> bool {
    let lifecycle_methods = [
        // Core lifecycle
        "Awake",
        "Start",
        "Update",
        "FixedUpdate",
        "LateUpdate",
        "OnEnable",
        "OnDisable",
        "OnDestroy",
        // Physics callbacks
        "OnTriggerEnter",
        "OnTriggerExit",
        "OnTriggerStay",
        "OnTriggerEnter2D",
        "OnTriggerExit2D",
        "OnTriggerStay2D",
        "OnCollisionEnter",
        "OnCollisionExit",
        "OnCollisionStay",
        "OnCollisionEnter2D",
        "OnCollisionExit2D",
        "OnCollisionStay2D",
        // UI callbacks
        "OnMouseEnter",
        "OnMouseExit",
        "OnMouseDown",
        "OnMouseUp",
        "OnMouseOver",
        "OnMouseDrag",
        // Rendering
        "OnRenderObject",
        "OnPreRender",
        "OnPostRender",
        "OnBecameVisible",
        "OnBecameInvisible",
        "OnWillRenderObject",
        "OnRenderImage",
        // Application events
        "OnApplicationFocus",
        "OnApplicationPause",
        "OnApplicationQuit",
        // Other
        "OnGUI",
        "OnDrawGizmos",
        "OnDrawGizmosSelected",
        "OnValidate",
        "Reset",
    ];

    lifecycle_methods.contains(&info.name.as_str())
}

/// Unity SerializeField: \[SerializeField\] attribute or field-like patterns
pub fn is_unity_serialized_field(info: &SymbolInfo) -> bool {
    // Check for [SerializeField] attribute
    let has_serialize_field = info.decorators.iter().any(|d| {
        let d_lower = d.to_lowercase();
        d_lower.contains("serializefield")
            || d_lower.contains("serializedfield")
            || d_lower.contains("header")
            || d_lower.contains("tooltip")
            || d_lower.contains("range")
            || d_lower.contains("hideininspector")
    });

    if has_serialize_field {
        return true;
    }

    // Fallback: Must have no logic
    if !info.control_flow.is_empty() || info.calls.len() > 2 {
        return false;
    }

    // Check for common Unity field naming patterns
    let name = &info.name;

    // Unity fields are often prefixed with underscore or lowercase camelCase
    let is_field_like = name.starts_with('_')
        || name.chars().next().map_or(false, |c| c.is_lowercase())
        // Or getter/setter accessor
        || name.starts_with("get_")
        || name.starts_with("set_");

    // Common Unity component field names
    let is_unity_field = matches!(
        name.as_str(),
        "speed"
            | "health"
            | "damage"
            | "force"
            | "radius"
            | "target"
            | "player"
            | "enemy"
            | "prefab"
            | "transform"
            | "rigidbody"
            | "collider"
            | "animator"
            | "audioSource"
            | "spriteRenderer"
            | "camera"
            | "light"
    ) || name.ends_with("Prefab")
        || name.ends_with("Object")
        || name.ends_with("Transform")
        || name.ends_with("Component");

    is_field_like || is_unity_field
}

/// Unity ScriptableObject: \[CreateAssetMenu\] attribute or ScriptableObject patterns
pub fn is_unity_scriptable_object(info: &SymbolInfo) -> bool {
    // Check for [CreateAssetMenu] attribute
    let has_scriptable_attr = info.decorators.iter().any(|d| {
        let d_lower = d.to_lowercase();
        d_lower.contains("createassetmenu") || d_lower.contains("scriptableobject")
    });

    if has_scriptable_attr {
        return true;
    }

    // Fallback: Check for ScriptableObject creation patterns
    let has_create_instance = info.calls.iter().any(|c| {
        c.name == "CreateInstance"
            || c.object
                .as_ref()
                .map_or(false, |o| o.contains("ScriptableObject"))
    });

    if has_create_instance {
        return true;
    }

    // Check naming patterns for ScriptableObject types
    let name = &info.name;
    (name.ends_with("Data") || name.ends_with("Config") || name.ends_with("Settings"))
        && info.control_flow.is_empty()
        && info.calls.is_empty()
}

// =============================================================================
// Detection Functions - ASP.NET Core
// =============================================================================

/// ASP.NET Controller: HTTP verb attributes or ActionResult patterns
pub fn is_aspnet_controller(info: &SymbolInfo) -> bool {
    // Check for HTTP verb attributes
    let has_http_attr = info.decorators.iter().any(|d| {
        let d_lower = d.to_lowercase();
        d_lower.contains("httpget")
            || d_lower.contains("httppost")
            || d_lower.contains("httpput")
            || d_lower.contains("httpdelete")
            || d_lower.contains("httppatch")
            || d_lower.contains("route")
            || d_lower.contains("apicontroller")
            || d_lower.contains("authorize")
            || d_lower.contains("allowanoynmous")
            || d_lower.contains("produces")
            || d_lower.contains("consumes")
    });

    if has_http_attr {
        // With HTTP attribute, allow more complexity (still controller action)
        return info.control_flow.len() <= 4 && info.calls.len() <= 10;
    }

    // Fallback: Check for ActionResult return patterns
    let has_action_result = info.calls.iter().any(|c| {
        matches!(
            c.name.as_str(),
            "Ok" | "BadRequest"
                | "NotFound"
                | "Created"
                | "CreatedAtAction"
                | "CreatedAtRoute"
                | "NoContent"
                | "Accepted"
                | "AcceptedAtAction"
                | "Unauthorized"
                | "Forbid"
                | "StatusCode"
                | "Json"
                | "Content"
                | "View"
                | "PartialView"
                | "RedirectToAction"
                | "Redirect"
                | "RedirectToRoute"
                | "LocalRedirect"
                | "File"
                | "PhysicalFile"
        )
    });

    if has_action_result {
        return info.control_flow.len() <= 3 && info.calls.len() <= 8;
    }

    // Check for HTTP action naming patterns
    let name = &info.name;
    let is_action_name = name.starts_with("Get")
        || name.starts_with("Post")
        || name.starts_with("Put")
        || name.starts_with("Delete")
        || name.starts_with("Patch")
        || name.starts_with("Create")
        || name.starts_with("Update")
        || name.starts_with("Remove")
        || name.starts_with("Index")
        || name.starts_with("Details")
        || name.starts_with("Edit")
        || name.starts_with("List");

    // Actions with HTTP naming should have moderate complexity
    if is_action_name {
        return info.control_flow.len() <= 2 && info.calls.len() <= 6;
    }

    false
}

/// ASP.NET Minimal API: app.MapGet/MapPost/MapPut/MapDelete
pub fn is_aspnet_minimal_api(info: &SymbolInfo) -> bool {
    let minimal_api_calls = [
        "MapGet",
        "MapPost",
        "MapPut",
        "MapDelete",
        "MapPatch",
        "MapMethods",
        "Map",
        "MapGroup",
        "MapFallback",
    ];

    let has_map_call = info.calls.iter().any(|c| {
        minimal_api_calls.contains(&c.name.as_str())
            || c.object
                .as_ref()
                .map_or(false, |o| o == "app" || o == "endpoints")
    });

    has_map_call && info.calls.len() <= 5
}

/// ASP.NET Middleware: Invoke/InvokeAsync that calls _next
pub fn is_aspnet_middleware(info: &SymbolInfo) -> bool {
    // Name must be Invoke or InvokeAsync
    if info.name != "Invoke" && info.name != "InvokeAsync" {
        return false;
    }

    // Should call _next or next (the next middleware in pipeline)
    let calls_next = info.calls.iter().any(|c| {
        c.name == "_next"
            || c.name == "next"
            || c.name == "Invoke"
            || c.name == "InvokeAsync"
            || c.object
                .as_ref()
                .map_or(false, |o| o == "_next" || o == "next")
    });

    calls_next
}

/// ASP.NET DI: 70%+ calls are AddScoped/AddSingleton/AddTransient/Configure
pub fn is_aspnet_di(info: &SymbolInfo) -> bool {
    if info.calls.is_empty() {
        return false;
    }

    let di_calls = [
        "AddScoped",
        "AddSingleton",
        "AddTransient",
        "AddHostedService",
        "AddDbContext",
        "AddDbContextPool",
        "Configure",
        "AddOptions",
        "Bind",
        "AddControllers",
        "AddEndpointsApiExplorer",
        "AddSwaggerGen",
        "AddAuthentication",
        "AddAuthorization",
        "AddCors",
        "AddHttpClient",
        "AddMemoryCache",
        "AddDistributedMemoryCache",
        "AddMvc",
        "AddRazorPages",
        "AddSignalR",
    ];

    let di_call_count = info
        .calls
        .iter()
        .filter(|c| {
            di_calls.contains(&c.name.as_str())
                || c.object
                    .as_ref()
                    .map_or(false, |o| o == "services" || o == "builder.Services")
        })
        .count();

    let ratio = di_call_count as f32 / info.calls.len() as f32;
    ratio >= 0.7 && info.control_flow.len() <= 1
}

// =============================================================================
// Detection Functions - Entity Framework
// =============================================================================

/// EF DbContext: OnConfiguring or OnModelCreating methods
pub fn is_ef_dbcontext(info: &SymbolInfo) -> bool {
    matches!(info.name.as_str(), "OnConfiguring" | "OnModelCreating")
}

/// EF DbSet: Property getter/setter with no logic, DbSet type
pub fn is_ef_dbset(info: &SymbolInfo) -> bool {
    // DbSet properties are typically simple getter/setters
    if !info.calls.is_empty() || !info.control_flow.is_empty() {
        return false;
    }

    // Check if return type mentions DbSet (would need to check type info)
    // For now, check naming patterns
    let name = &info.name;

    // DbSet properties are typically PascalCase plural nouns
    let is_pascal_case = name.chars().next().map_or(false, |c| c.is_uppercase());
    let is_plural = name.ends_with('s') || name.ends_with("ies") || name.ends_with("es");

    is_pascal_case && is_plural && info.calls.is_empty() && info.control_flow.is_empty()
}

/// EF Fluent API: 60%+ calls are HasKey/HasOne/HasMany/Property/ToTable
pub fn is_ef_fluent_api(info: &SymbolInfo) -> bool {
    if info.calls.is_empty() {
        return false;
    }

    let fluent_calls = [
        "HasKey",
        "HasOne",
        "HasMany",
        "HasIndex",
        "Property",
        "ToTable",
        "HasColumnName",
        "HasColumnType",
        "IsRequired",
        "HasMaxLength",
        "HasDefaultValue",
        "HasConversion",
        "HasForeignKey",
        "WithOne",
        "WithMany",
        "OnDelete",
        "HasPrecision",
        "HasComment",
        "Entity",
        "OwnsOne",
        "OwnsMany",
    ];

    let fluent_call_count = info
        .calls
        .iter()
        .filter(|c| {
            fluent_calls.contains(&c.name.as_str())
                || c.object.as_ref().map_or(false, |o| {
                    o.contains("modelBuilder") || o.contains("entity")
                })
        })
        .count();

    let ratio = fluent_call_count as f32 / info.calls.len() as f32;
    ratio >= 0.6
}

/// EF Migration: Up/Down methods that call CreateTable/DropTable/AddColumn
pub fn is_ef_migration(info: &SymbolInfo) -> bool {
    // Name must be Up or Down
    if info.name != "Up" && info.name != "Down" {
        return false;
    }

    let migration_calls = [
        "CreateTable",
        "DropTable",
        "AddColumn",
        "DropColumn",
        "AlterColumn",
        "RenameColumn",
        "AddForeignKey",
        "DropForeignKey",
        "CreateIndex",
        "DropIndex",
        "RenameTable",
        "RenameIndex",
        "AddPrimaryKey",
        "DropPrimaryKey",
        "InsertData",
        "DeleteData",
        "UpdateData",
        "Sql",
    ];

    let has_migration_call = info.calls.iter().any(|c| {
        migration_calls.contains(&c.name.as_str())
            || c.object
                .as_ref()
                .map_or(false, |o| o.contains("migrationBuilder"))
    });

    has_migration_call
}

// =============================================================================
// Detection Functions - LINQ
// =============================================================================

/// LINQ Chain: 2+ LINQ calls (Select/Where/OrderBy/GroupBy) with <=1 control flow
pub fn is_linq_chain(info: &SymbolInfo) -> bool {
    let linq_methods = [
        "Select",
        "Where",
        "OrderBy",
        "OrderByDescending",
        "ThenBy",
        "ThenByDescending",
        "GroupBy",
        "Join",
        "SelectMany",
        "Distinct",
        "Take",
        "Skip",
        "First",
        "FirstOrDefault",
        "Single",
        "SingleOrDefault",
        "Last",
        "LastOrDefault",
        "Any",
        "All",
        "Count",
        "Sum",
        "Average",
        "Min",
        "Max",
        "Aggregate",
        "Zip",
        "Concat",
        "Union",
        "Intersect",
        "Except",
    ];

    let linq_count = info
        .calls
        .iter()
        .filter(|c| linq_methods.contains(&c.name.as_str()))
        .count();

    linq_count >= 2 && info.control_flow.len() <= 1
}

/// LINQ Projection: Only Select + ToList/ToArray calls, no control flow
pub fn is_linq_projection(info: &SymbolInfo) -> bool {
    if !info.control_flow.is_empty() {
        return false;
    }

    let has_select = info.calls.iter().any(|c| c.name == "Select");
    let has_materialization = info.calls.iter().any(|c| {
        matches!(
            c.name.as_str(),
            "ToList" | "ToArray" | "ToDictionary" | "ToHashSet"
        )
    });

    has_select && has_materialization && info.calls.len() <= 4
}

// =============================================================================
// Detection Functions - General C#
// =============================================================================

/// C# Property: No calls, no control flow, PascalCase name
pub fn is_csharp_property(info: &SymbolInfo) -> bool {
    // Properties have no logic
    if !info.calls.is_empty() || !info.control_flow.is_empty() {
        return false;
    }

    // Check for PascalCase (typical property naming)
    let is_pascal_case = info.name.chars().next().map_or(false, |c| c.is_uppercase());

    // Common property prefixes/patterns
    let is_property_name =
        is_pascal_case || info.name.starts_with("get_") || info.name.starts_with("set_");

    is_property_name
}

/// C# Record: Equals/GetHashCode/ToString/Deconstruct with minimal logic
pub fn is_csharp_record(info: &SymbolInfo) -> bool {
    let record_methods = [
        "Equals",
        "GetHashCode",
        "ToString",
        "Deconstruct",
        "PrintMembers",
        "Clone",
        "op_Equality",
        "op_Inequality",
    ];

    if !record_methods.contains(&info.name.as_str()) {
        return false;
    }

    // Record-generated methods should be simple
    info.control_flow.len() <= 2 && info.calls.len() <= 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate::boilerplate::tests::make_symbol;
    use crate::schema::{Call, ControlFlowChange, ControlFlowKind, Location};

    /// Create a symbol with calls that have object context
    fn make_symbol_with_object_calls(
        name: &str,
        calls: Vec<(&str, Option<&str>)>,
        control_flow: usize,
    ) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            calls: calls
                .into_iter()
                .map(|(n, obj)| Call {
                    name: n.to_string(),
                    object: obj.map(|s| s.to_string()),
                    is_awaited: false,
                    in_try: false,
                    is_hook: false,
                    is_io: false,
                    location: Location::default(),
                })
                .collect(),
            control_flow: (0..control_flow)
                .map(|_| ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                    nesting_depth: 0,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Create a symbol with decorators for attribute-based testing
    fn make_symbol_with_decorators(
        name: &str,
        calls: Vec<&str>,
        control_flow: usize,
        decorators: Vec<&str>,
    ) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            calls: calls
                .into_iter()
                .map(|n| Call {
                    name: n.to_string(),
                    object: None,
                    is_awaited: false,
                    in_try: false,
                    is_hook: false,
                    is_io: false,
                    location: Location::default(),
                })
                .collect(),
            control_flow: (0..control_flow)
                .map(|_| ControlFlowChange {
                    kind: ControlFlowKind::If,
                    location: Location::default(),
                    nesting_depth: 0,
                })
                .collect(),
            decorators: decorators.into_iter().map(|d| d.to_string()).collect(),
            ..Default::default()
        }
    }

    // =========================================================================
    // xUnit Test Tests
    // =========================================================================

    #[test]
    fn test_xunit_should_pattern() {
        let symbol = make_symbol_with_object_calls(
            "User_Should_HaveValidEmail",
            vec![("True", Some("Assert"))],
            0,
        );
        assert!(is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_test_prefix_with_assert() {
        let symbol =
            make_symbol_with_object_calls("TestUserCreation", vec![("NotNull", Some("Assert"))], 0);
        assert!(is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_when_then_pattern() {
        let symbol = make_symbol("User_When_Created_Then_HasId", vec!["Equal"], 0);
        assert!(is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_not_a_test() {
        let symbol = make_symbol("ProcessUser", vec!["Validate"], 0);
        assert!(!is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_no_test_naming() {
        // Has Assert but no test naming pattern
        let symbol = make_symbol("GetUser", vec!["Equal"], 0);
        assert!(!is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_fact_attribute() {
        let symbol = make_symbol_with_decorators("SomeMethod", vec![], 0, vec!["Fact"]);
        assert!(is_xunit_test(&symbol));
    }

    #[test]
    fn test_xunit_theory_attribute() {
        let symbol =
            make_symbol_with_decorators("DataDrivenTest", vec![], 0, vec!["Theory", "InlineData"]);
        assert!(is_xunit_test(&symbol));
    }

    // =========================================================================
    // NUnit Test Tests
    // =========================================================================

    #[test]
    fn test_nunit_assert_that() {
        let symbol = make_symbol("TestSomething", vec!["That"], 0);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_are_equal() {
        let symbol = make_symbol("TestComparison", vec!["AreEqual"], 0);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_assert_object() {
        let symbol = make_symbol_with_object_calls("TestMethod", vec![("That", Some("Assert"))], 0);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_is_true() {
        let symbol = make_symbol("VerifyCondition", vec!["IsTrue", "IsNotNull"], 0);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_test_attribute() {
        let symbol = make_symbol_with_decorators("SomeMethod", vec![], 0, vec!["Test"]);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_testcase_attribute() {
        let symbol = make_symbol_with_decorators("ParameterizedTest", vec![], 0, vec!["TestCase"]);
        assert!(is_nunit_test(&symbol));
    }

    #[test]
    fn test_nunit_testfixture_attribute() {
        let symbol = make_symbol_with_decorators("TestClass", vec![], 0, vec!["TestFixture"]);
        assert!(is_nunit_test(&symbol));
    }

    // =========================================================================
    // Moq Setup Tests
    // =========================================================================

    #[test]
    fn test_moq_setup_returns() {
        let symbol = make_symbol_with_object_calls(
            "SetupMocks",
            vec![("Setup", Some("mockService")), ("Returns", None)],
            0,
        );
        assert!(is_moq_setup(&symbol));
    }

    #[test]
    fn test_moq_verify() {
        let symbol = make_symbol_with_object_calls(
            "VerifyMocks",
            vec![("Verify", Some("mockService")), ("VerifyAll", None)],
            0,
        );
        assert!(is_moq_setup(&symbol));
    }

    #[test]
    fn test_moq_not_enough_moq_calls() {
        let symbol = make_symbol(
            "ProcessData",
            vec!["Setup", "Process", "Transform", "Save"],
            0,
        );
        assert!(!is_moq_setup(&symbol));
    }

    // =========================================================================
    // Unity Lifecycle Tests
    // =========================================================================

    #[test]
    fn test_unity_start() {
        let symbol = make_symbol("Start", vec!["Initialize"], 0);
        assert!(is_unity_lifecycle(&symbol));
    }

    #[test]
    fn test_unity_update() {
        let symbol = make_symbol("Update", vec!["MovePlayer"], 1);
        assert!(is_unity_lifecycle(&symbol));
    }

    #[test]
    fn test_unity_awake() {
        let symbol = make_symbol("Awake", vec!["GetComponent"], 0);
        assert!(is_unity_lifecycle(&symbol));
    }

    #[test]
    fn test_unity_on_trigger_enter() {
        let symbol = make_symbol("OnTriggerEnter", vec!["ApplyDamage"], 1);
        assert!(is_unity_lifecycle(&symbol));
    }

    #[test]
    fn test_unity_not_lifecycle() {
        let symbol = make_symbol("CustomMethod", vec!["DoSomething"], 0);
        assert!(!is_unity_lifecycle(&symbol));
    }

    // =========================================================================
    // Unity SerializeField Tests (naming pattern based)
    // =========================================================================

    #[test]
    fn test_unity_serialized_field_underscore() {
        let symbol = make_symbol("_speed", vec![], 0);
        assert!(is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_lowercase() {
        let symbol = make_symbol("health", vec![], 0);
        assert!(is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_prefab() {
        let symbol = make_symbol("playerPrefab", vec![], 0);
        assert!(is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_with_logic() {
        let symbol = make_symbol("_speed", vec!["Calculate"], 1);
        assert!(!is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_attribute() {
        let symbol = make_symbol_with_decorators(
            "someField",
            vec!["DoSomething"],
            1,
            vec!["SerializeField"],
        );
        assert!(is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_header_attribute() {
        let symbol =
            make_symbol_with_decorators("_health", vec![], 0, vec!["Header", "SerializeField"]);
        assert!(is_unity_serialized_field(&symbol));
    }

    #[test]
    fn test_unity_serialized_field_range_attribute() {
        let symbol = make_symbol_with_decorators("speed", vec![], 0, vec!["Range"]);
        assert!(is_unity_serialized_field(&symbol));
    }

    // =========================================================================
    // Unity ScriptableObject Tests
    // =========================================================================

    #[test]
    fn test_unity_scriptable_object_create_instance() {
        let symbol = make_symbol_with_object_calls(
            "CreateConfig",
            vec![("CreateInstance", Some("ScriptableObject"))],
            0,
        );
        assert!(is_unity_scriptable_object(&symbol));
    }

    #[test]
    fn test_unity_scriptable_object_naming() {
        let symbol = make_symbol("GameConfig", vec![], 0);
        assert!(is_unity_scriptable_object(&symbol));
    }

    #[test]
    fn test_unity_scriptable_object_data() {
        let symbol = make_symbol("PlayerData", vec![], 0);
        assert!(is_unity_scriptable_object(&symbol));
    }

    #[test]
    fn test_unity_scriptable_object_create_asset_menu_attribute() {
        let symbol = make_symbol_with_decorators(
            "WeaponData",
            vec!["Initialize"],
            1,
            vec!["CreateAssetMenu"],
        );
        assert!(is_unity_scriptable_object(&symbol));
    }

    // =========================================================================
    // ASP.NET Controller Tests
    // =========================================================================

    #[test]
    fn test_aspnet_controller_ok_result() {
        let symbol = make_symbol("GetUsers", vec!["Ok", "_repository.GetAll"], 0);
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_controller_created_result() {
        let symbol = make_symbol("CreateUser", vec!["Created", "_repository.Add"], 1);
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_action_result_pattern() {
        let symbol = make_symbol("GetById", vec!["NotFound", "Ok"], 1);
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_naming_pattern() {
        let symbol = make_symbol("GetAllProducts", vec!["_service.GetAll"], 0);
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_too_complex() {
        let symbol = make_symbol(
            "GetUsers",
            vec!["a", "b", "c", "d", "e", "f", "g", "h", "i"],
            4,
        );
        assert!(!is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_controller_httpget_attribute() {
        let symbol = make_symbol_with_decorators(
            "ProcessRequest",
            vec!["Service.DoWork"],
            2,
            vec!["HttpGet"],
        );
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_controller_httppost_attribute() {
        let symbol = make_symbol_with_decorators(
            "SubmitForm",
            vec!["Validate", "Save"],
            1,
            vec!["HttpPost", "Route"],
        );
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_controller_authorize_attribute() {
        let symbol = make_symbol_with_decorators(
            "SecureEndpoint",
            vec!["GetData"],
            0,
            vec!["Authorize", "HttpGet"],
        );
        assert!(is_aspnet_controller(&symbol));
    }

    #[test]
    fn test_aspnet_controller_with_attribute_allows_more_complexity() {
        // With attribute, we allow more calls/control flow
        let symbol = make_symbol_with_decorators(
            "ComplexAction",
            vec!["a", "b", "c", "d", "e", "f", "g", "h"],
            3,
            vec!["HttpGet"],
        );
        assert!(is_aspnet_controller(&symbol));
    }

    // =========================================================================
    // ASP.NET Minimal API Tests
    // =========================================================================

    #[test]
    fn test_aspnet_map_get() {
        let symbol = make_symbol_with_object_calls(
            "ConfigureEndpoints",
            vec![("MapGet", Some("app")), ("MapPost", Some("app"))],
            0,
        );
        assert!(is_aspnet_minimal_api(&symbol));
    }

    #[test]
    fn test_aspnet_map_group() {
        let symbol = make_symbol("SetupRoutes", vec!["MapGroup", "MapGet"], 0);
        assert!(is_aspnet_minimal_api(&symbol));
    }

    // =========================================================================
    // ASP.NET Middleware Tests
    // =========================================================================

    #[test]
    fn test_aspnet_middleware_invoke() {
        let symbol = make_symbol_with_object_calls(
            "Invoke",
            vec![("Invoke", Some("_next")), ("LogRequest", None)],
            0,
        );
        assert!(is_aspnet_middleware(&symbol));
    }

    #[test]
    fn test_aspnet_middleware_invoke_async() {
        let symbol = make_symbol("InvokeAsync", vec!["_next", "LogRequest"], 0);
        assert!(is_aspnet_middleware(&symbol));
    }

    #[test]
    fn test_aspnet_not_middleware() {
        let symbol = make_symbol("Process", vec!["_next"], 0);
        assert!(!is_aspnet_middleware(&symbol));
    }

    // =========================================================================
    // ASP.NET DI Tests
    // =========================================================================

    #[test]
    fn test_aspnet_di_services() {
        let symbol = make_symbol_with_object_calls(
            "ConfigureServices",
            vec![
                ("AddScoped", Some("services")),
                ("AddSingleton", Some("services")),
                ("AddTransient", Some("services")),
            ],
            0,
        );
        assert!(is_aspnet_di(&symbol));
    }

    #[test]
    fn test_aspnet_di_mixed() {
        let symbol = make_symbol(
            "ConfigureServices",
            vec!["AddControllers", "AddSwaggerGen", "AddAuthentication"],
            0,
        );
        assert!(is_aspnet_di(&symbol));
    }

    // =========================================================================
    // EF DbContext Tests
    // =========================================================================

    #[test]
    fn test_ef_on_configuring() {
        let symbol = make_symbol("OnConfiguring", vec!["UseSqlServer"], 0);
        assert!(is_ef_dbcontext(&symbol));
    }

    #[test]
    fn test_ef_on_model_creating() {
        let symbol = make_symbol("OnModelCreating", vec!["Entity", "HasKey"], 0);
        assert!(is_ef_dbcontext(&symbol));
    }

    #[test]
    fn test_ef_not_dbcontext() {
        let symbol = make_symbol("Initialize", vec!["UseSqlServer"], 0);
        assert!(!is_ef_dbcontext(&symbol));
    }

    // =========================================================================
    // EF DbSet Tests
    // =========================================================================

    #[test]
    fn test_ef_dbset_users() {
        let symbol = make_symbol("Users", vec![], 0);
        assert!(is_ef_dbset(&symbol));
    }

    #[test]
    fn test_ef_dbset_products() {
        let symbol = make_symbol("Products", vec![], 0);
        assert!(is_ef_dbset(&symbol));
    }

    #[test]
    fn test_ef_dbset_with_calls_not_dbset() {
        let symbol = make_symbol("Users", vec!["GetAll"], 0);
        assert!(!is_ef_dbset(&symbol));
    }

    // =========================================================================
    // EF Fluent API Tests
    // =========================================================================

    #[test]
    fn test_ef_fluent_api() {
        let symbol = make_symbol_with_object_calls(
            "ConfigureUser",
            vec![
                ("HasKey", Some("entity")),
                ("Property", Some("entity")),
                ("HasMaxLength", None),
            ],
            0,
        );
        assert!(is_ef_fluent_api(&symbol));
    }

    #[test]
    fn test_ef_fluent_api_relationships() {
        let symbol = make_symbol(
            "ConfigureRelations",
            vec!["HasOne", "WithMany", "HasForeignKey"],
            0,
        );
        assert!(is_ef_fluent_api(&symbol));
    }

    // =========================================================================
    // EF Migration Tests
    // =========================================================================

    #[test]
    fn test_ef_migration_up() {
        let symbol = make_symbol_with_object_calls(
            "Up",
            vec![
                ("CreateTable", Some("migrationBuilder")),
                ("AddColumn", None),
            ],
            0,
        );
        assert!(is_ef_migration(&symbol));
    }

    #[test]
    fn test_ef_migration_down() {
        let symbol = make_symbol("Down", vec!["DropTable", "DropColumn"], 0);
        assert!(is_ef_migration(&symbol));
    }

    #[test]
    fn test_ef_not_migration() {
        let symbol = make_symbol("Initialize", vec!["CreateTable"], 0);
        assert!(!is_ef_migration(&symbol));
    }

    // =========================================================================
    // LINQ Chain Tests
    // =========================================================================

    #[test]
    fn test_linq_chain_select_where() {
        let symbol = make_symbol("GetActiveUsers", vec!["Where", "Select"], 0);
        assert!(is_linq_chain(&symbol));
    }

    #[test]
    fn test_linq_chain_complex() {
        let symbol = make_symbol("QueryData", vec!["Where", "OrderBy", "Select", "Take"], 1);
        assert!(is_linq_chain(&symbol));
    }

    #[test]
    fn test_linq_chain_too_much_control_flow() {
        let symbol = make_symbol("ComplexQuery", vec!["Where", "Select"], 2);
        assert!(!is_linq_chain(&symbol));
    }

    #[test]
    fn test_linq_single_call_not_chain() {
        let symbol = make_symbol("GetFirst", vec!["First"], 0);
        assert!(!is_linq_chain(&symbol));
    }

    // =========================================================================
    // LINQ Projection Tests
    // =========================================================================

    #[test]
    fn test_linq_projection_to_list() {
        let symbol = make_symbol("GetNames", vec!["Select", "ToList"], 0);
        assert!(is_linq_projection(&symbol));
    }

    #[test]
    fn test_linq_projection_to_array() {
        let symbol = make_symbol("GetIds", vec!["Select", "ToArray"], 0);
        assert!(is_linq_projection(&symbol));
    }

    #[test]
    fn test_linq_projection_with_control_flow() {
        let symbol = make_symbol("GetData", vec!["Select", "ToList"], 1);
        assert!(!is_linq_projection(&symbol));
    }

    // =========================================================================
    // C# Property Tests
    // =========================================================================

    #[test]
    fn test_csharp_property_pascal_case() {
        let symbol = make_symbol("UserName", vec![], 0);
        assert!(is_csharp_property(&symbol));
    }

    #[test]
    fn test_csharp_property_get_prefix() {
        let symbol = make_symbol("get_Value", vec![], 0);
        assert!(is_csharp_property(&symbol));
    }

    #[test]
    fn test_csharp_property_with_logic() {
        let symbol = make_symbol("ComputedValue", vec!["Calculate"], 1);
        assert!(!is_csharp_property(&symbol));
    }

    // =========================================================================
    // C# Record Tests
    // =========================================================================

    #[test]
    fn test_csharp_record_equals() {
        let symbol = make_symbol("Equals", vec!["Equals"], 1);
        assert!(is_csharp_record(&symbol));
    }

    #[test]
    fn test_csharp_record_get_hash_code() {
        let symbol = make_symbol("GetHashCode", vec!["HashCode.Combine"], 0);
        assert!(is_csharp_record(&symbol));
    }

    #[test]
    fn test_csharp_record_to_string() {
        let symbol = make_symbol("ToString", vec!["StringBuilder"], 0);
        assert!(is_csharp_record(&symbol));
    }

    #[test]
    fn test_csharp_record_deconstruct() {
        let symbol = make_symbol("Deconstruct", vec![], 0);
        assert!(is_csharp_record(&symbol));
    }

    #[test]
    fn test_csharp_record_too_complex() {
        let symbol = make_symbol("Equals", vec!["a", "b", "c", "d", "e"], 3);
        assert!(!is_csharp_record(&symbol));
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[test]
    fn test_classify_csharp_boilerplate_all_types() {
        use crate::duplicate::boilerplate::classify_boilerplate_with_lang;
        use crate::lang::Lang;

        // xUnit test
        let xunit =
            make_symbol_with_decorators("TestMethod", vec!["Assert.True"], 0, vec!["[Fact]"]);
        assert_eq!(
            classify_boilerplate_with_lang(&xunit, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::XUnitTest)
        );

        // Unity lifecycle
        let unity = make_symbol("Update", vec!["Move"], 0);
        assert_eq!(
            classify_boilerplate_with_lang(&unity, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::UnityLifecycle)
        );

        // ASP.NET controller
        let controller = make_symbol("GetUser", vec!["Ok", "NotFound"], 1);
        assert_eq!(
            classify_boilerplate_with_lang(&controller, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::AspNetController)
        );

        // EF DbContext
        let dbcontext = make_symbol("OnModelCreating", vec!["Entity"], 0);
        assert_eq!(
            classify_boilerplate_with_lang(&dbcontext, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::EFDbContext)
        );

        // LINQ chain
        let linq = make_symbol("QueryUsers", vec!["Where", "Select", "ToList"], 0);
        assert_eq!(
            classify_boilerplate_with_lang(&linq, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::LinqChain)
        );

        // C# record
        let record = make_symbol("Equals", vec!["Equals"], 0);
        assert_eq!(
            classify_boilerplate_with_lang(&record, Some(Lang::CSharp), None),
            Some(BoilerplateCategory::CSharpRecord)
        );
    }

    #[test]
    fn test_csharp_boilerplate_disabled() {
        use crate::duplicate::boilerplate::{
            classify_boilerplate_with_lang, BoilerplateConfig, BuiltinBoilerplate,
        };
        use crate::lang::Lang;

        let mut builtin = BuiltinBoilerplate::default();
        builtin.disable(BoilerplateCategory::XUnitTest);
        builtin.disable(BoilerplateCategory::UnityLifecycle);

        let config = BoilerplateConfig {
            builtin,
            custom: vec![],
        };

        // Should not match when disabled
        let xunit =
            make_symbol_with_decorators("TestMethod", vec!["Assert.True"], 0, vec!["[Fact]"]);
        assert_ne!(
            classify_boilerplate_with_lang(&xunit, Some(Lang::CSharp), Some(&config)),
            Some(BoilerplateCategory::XUnitTest)
        );

        let unity = make_symbol("Update", vec!["Move"], 0);
        assert_ne!(
            classify_boilerplate_with_lang(&unity, Some(Lang::CSharp), Some(&config)),
            Some(BoilerplateCategory::UnityLifecycle)
        );
    }
}
