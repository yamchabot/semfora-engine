//! Language Grammar Definitions
//!
//! This module defines the AST node mappings for each supported language.
//! Instead of duplicating extraction logic in each detector, we define
//! language-specific node names here and use a generic extractor.
//!
//! # Architecture
//!
//! Each language has a `LangGrammar` that maps semantic concepts to
//! tree-sitter node kinds. The generic extractor in `generic.rs` uses
//! these mappings to extract symbols, control flow, state changes, etc.
//!
//! # Adding a New Language
//!
//! 1. Add a new `LangGrammar` constant (e.g., `KOTLIN_GRAMMAR`)
//! 2. Fill in the AST node kinds from the tree-sitter grammar
//! 3. Implement the `is_exported` function for visibility rules
//! 4. Register it in the dispatcher

use tree_sitter::Node;

/// Language-specific AST node mappings for semantic extraction
#[derive(Debug, Clone)]
pub struct LangGrammar {
    /// Language identifier
    pub name: &'static str,

    // =========================================================================
    // Symbol Detection
    // =========================================================================
    /// Function/method declaration nodes
    /// e.g., ["function_definition", "method_definition", "function_item"]
    pub function_nodes: &'static [&'static str],

    /// Class/struct declaration nodes
    /// e.g., ["class_declaration", "struct_item", "class_specifier"]
    pub class_nodes: &'static [&'static str],

    /// Interface/trait declaration nodes
    /// e.g., ["interface_declaration", "trait_item"]
    pub interface_nodes: &'static [&'static str],

    /// Enum declaration nodes
    /// e.g., ["enum_declaration", "enum_item"]
    pub enum_nodes: &'static [&'static str],

    // =========================================================================
    // Control Flow
    // =========================================================================
    /// Control flow nodes that increase nesting depth
    /// e.g., ["if_statement", "for_statement", "while_statement", "match_expression"]
    pub control_flow_nodes: &'static [&'static str],

    /// Try/catch/exception handling nodes
    /// e.g., ["try_statement", "try_expression"]
    pub try_nodes: &'static [&'static str],

    // =========================================================================
    // State Changes (Assignments)
    // =========================================================================
    /// Variable declaration nodes
    /// e.g., ["let_declaration", "variable_declaration", "short_var_declaration"]
    pub var_declaration_nodes: &'static [&'static str],

    /// Assignment expression nodes
    /// e.g., ["assignment_expression", "assignment_statement"]
    pub assignment_nodes: &'static [&'static str],

    // =========================================================================
    // Function Calls
    // =========================================================================
    /// Function/method call nodes
    /// e.g., ["call_expression", "method_invocation"]
    pub call_nodes: &'static [&'static str],

    /// Await expression nodes (for async detection)
    /// e.g., ["await_expression"]
    pub await_nodes: &'static [&'static str],

    // =========================================================================
    // Imports/Dependencies
    // =========================================================================
    /// Import statement nodes
    /// e.g., ["import_statement", "use_declaration", "import_declaration"]
    pub import_nodes: &'static [&'static str],

    // =========================================================================
    // Field Names for Child Access
    // =========================================================================
    /// Field name for symbol/function name
    /// Usually "name"
    pub name_field: &'static str,

    /// Field name for right-hand side of assignment
    /// e.g., "value", "right"
    pub value_field: &'static str,

    /// Field name for type annotation
    /// e.g., "type", "return_type"
    pub type_field: &'static str,

    /// Field name for function/block body
    /// e.g., "body", "block"
    pub body_field: &'static str,

    /// Field name for function parameters
    /// e.g., "parameters", "params"
    pub params_field: &'static str,

    /// Field name for condition in control flow
    /// e.g., "condition", "test"
    pub condition_field: &'static str,

    // =========================================================================
    // Visibility/Export Detection
    // =========================================================================
    /// Function to determine if a symbol is exported/public
    /// Takes the declaration node and source code
    pub is_exported: fn(&Node, &str) -> bool,

    // =========================================================================
    // Language-Specific Quirks
    // =========================================================================
    /// Whether the language uses uppercase for export (Go convention)
    pub uppercase_is_export: bool,

    /// Visibility modifier keywords to check
    /// e.g., ["pub", "public", "export"]
    pub visibility_modifiers: &'static [&'static str],

    /// Decorator/attribute nodes (Python decorators, Rust attributes, Java annotations)
    /// e.g., ["decorator", "attribute_item", "annotation"]
    pub decorator_nodes: &'static [&'static str],
}

// =============================================================================
// Visibility Checker Functions
// =============================================================================

/// Go: uppercase first letter = exported
pub fn go_is_exported(node: &Node, source: &str) -> bool {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
        name.chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    } else {
        false
    }
}

/// Rust: has `pub` visibility modifier
pub fn rust_is_exported(node: &Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            return text.starts_with("pub");
        }
    }
    false
}

/// Python: no underscore prefix = public
pub fn python_is_exported(node: &Node, source: &str) -> bool {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
        !name.starts_with('_')
    } else {
        true
    }
}

/// Java: has `public` modifier
pub fn java_is_exported(node: &Node, _source: &str) -> bool {
    if let Some(modifiers) = node.child_by_field_name("modifiers") {
        let mut cursor = modifiers.walk();
        for child in modifiers.children(&mut cursor) {
            if child.kind() == "public" {
                return true;
            }
        }
    }
    false
}

/// C#: has `public` or `internal` modifier (similar to Java but with different AST)
pub fn csharp_is_exported(node: &Node, source: &str) -> bool {
    // Check for modifiers field
    if let Some(modifiers) = node.child_by_field_name("modifiers") {
        let mut cursor = modifiers.walk();
        for child in modifiers.children(&mut cursor) {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                if text == "public" || text == "internal" {
                    return true;
                }
            }
        }
    }
    // Also check direct children for modifier nodes
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                if text == "public" || text == "internal" {
                    return true;
                }
            }
        }
    }
    false
}

/// JavaScript/TypeScript: has `export` keyword
pub fn js_is_exported(node: &Node, source: &str) -> bool {
    // Check if parent or sibling is export_statement
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            return true;
        }
    }
    // Check for export keyword in source before the node
    let start = node.start_byte();
    if start > 10 {
        let prefix = &source[start.saturating_sub(20)..start];
        return prefix.contains("export ");
    }
    false
}

/// C/C++: in header file or has extern
pub fn c_is_exported(node: &Node, source: &str) -> bool {
    // Check for extern keyword
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    text.contains("extern")
}

/// Default: always false (conservative)
pub fn default_is_exported(_node: &Node, _source: &str) -> bool {
    false
}

// =============================================================================
// Language Grammar Definitions
// =============================================================================

pub static RUST_GRAMMAR: LangGrammar = LangGrammar {
    name: "rust",
    function_nodes: &["function_item"],
    class_nodes: &["struct_item"],
    interface_nodes: &["trait_item"],
    enum_nodes: &["enum_item"],
    control_flow_nodes: &[
        "if_expression",
        "match_expression",
        "for_expression",
        "while_expression",
        "loop_expression",
    ],
    try_nodes: &["try_expression"],
    var_declaration_nodes: &["let_declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["call_expression"],
    await_nodes: &["await_expression"],
    import_nodes: &["use_declaration"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: rust_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["pub", "pub(crate)", "pub(super)"],
    decorator_nodes: &["attribute_item"],
};

pub static GO_GRAMMAR: LangGrammar = LangGrammar {
    name: "go",
    function_nodes: &["function_declaration", "method_declaration"],
    class_nodes: &[],     // Go uses struct_type inside type_declaration
    interface_nodes: &[], // Go uses interface_type inside type_declaration
    enum_nodes: &[],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "switch_statement",
        "select_statement",
        "type_switch_statement",
    ],
    try_nodes: &[], // Go uses defer/recover, not try/catch
    var_declaration_nodes: &["short_var_declaration", "var_declaration"],
    assignment_nodes: &["assignment_statement"],
    call_nodes: &["call_expression"],
    await_nodes: &[], // Go doesn't have await
    import_nodes: &["import_spec"],
    name_field: "name",
    value_field: "right",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: go_is_exported,
    uppercase_is_export: true,
    visibility_modifiers: &[],
    decorator_nodes: &[],
};

pub static JAVA_GRAMMAR: LangGrammar = LangGrammar {
    name: "java",
    function_nodes: &["method_declaration", "constructor_declaration"],
    class_nodes: &["class_declaration"],
    interface_nodes: &["interface_declaration"],
    enum_nodes: &["enum_declaration"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "enhanced_for_statement",
        "while_statement",
        "do_statement",
        "switch_expression",
        "switch_statement",
    ],
    try_nodes: &["try_statement", "try_with_resources_statement"],
    var_declaration_nodes: &["local_variable_declaration", "field_declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["method_invocation"],
    await_nodes: &[], // Java uses CompletableFuture, not await
    import_nodes: &["import_declaration"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: java_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "protected", "private"],
    decorator_nodes: &["annotation", "marker_annotation"],
};

pub static CSHARP_GRAMMAR: LangGrammar = LangGrammar {
    name: "csharp",
    function_nodes: &[
        "method_declaration",
        "constructor_declaration",
        "local_function_statement",
    ],
    class_nodes: &[
        "class_declaration",
        "struct_declaration",
        "record_declaration",
    ],
    interface_nodes: &["interface_declaration"],
    enum_nodes: &["enum_declaration"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "foreach_statement",
        "while_statement",
        "do_statement",
        "switch_statement",
        "switch_expression", // C# 8+ pattern matching switch
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &[
        "local_declaration_statement",
        "field_declaration",
        "property_declaration",
    ],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["invocation_expression"],
    await_nodes: &["await_expression"],
    import_nodes: &["using_directive"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: csharp_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "protected", "private", "internal"],
    decorator_nodes: &["attribute_list", "attribute"],
};

pub static PYTHON_GRAMMAR: LangGrammar = LangGrammar {
    name: "python",
    function_nodes: &["function_definition"],
    class_nodes: &["class_definition"],
    interface_nodes: &[], // Python uses ABC, not interfaces
    enum_nodes: &[],      // Python enums are classes
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "match_statement",
        "with_statement",
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &["assignment"],
    assignment_nodes: &["assignment", "augmented_assignment"],
    call_nodes: &["call"],
    await_nodes: &["await"],
    import_nodes: &["import_statement", "import_from_statement"],
    name_field: "name",
    value_field: "right",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: python_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &[],
    decorator_nodes: &["decorator"],
};

pub static JAVASCRIPT_GRAMMAR: LangGrammar = LangGrammar {
    name: "javascript",
    function_nodes: &[
        "function_declaration",
        "function_expression",
        "arrow_function",
        "method_definition",
    ],
    class_nodes: &["class_declaration", "class"],
    interface_nodes: &[], // JS doesn't have interfaces (TS does)
    enum_nodes: &[],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "for_in_statement",
        "for_of_statement",
        "while_statement",
        "do_statement",
        "switch_statement",
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &["variable_declaration", "lexical_declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["call_expression"],
    await_nodes: &["await_expression"],
    import_nodes: &["import_statement"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: js_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["export", "default"],
    decorator_nodes: &["decorator"],
};

pub static TYPESCRIPT_GRAMMAR: LangGrammar = LangGrammar {
    name: "typescript",
    function_nodes: &[
        "function_declaration",
        "function_expression",
        "arrow_function",
        "method_definition",
    ],
    class_nodes: &["class_declaration", "class"],
    interface_nodes: &["interface_declaration"],
    enum_nodes: &["enum_declaration"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "for_in_statement",
        "for_of_statement",
        "while_statement",
        "do_statement",
        "switch_statement",
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &["variable_declaration", "lexical_declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["call_expression"],
    await_nodes: &["await_expression"],
    import_nodes: &["import_statement"],
    name_field: "name",
    value_field: "value",
    type_field: "type_annotation",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: js_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["export", "default", "public", "private", "protected"],
    decorator_nodes: &["decorator"],
};

pub static C_GRAMMAR: LangGrammar = LangGrammar {
    name: "c",
    function_nodes: &["function_definition"],
    class_nodes: &["struct_specifier"],
    interface_nodes: &[],
    enum_nodes: &["enum_specifier"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "switch_statement",
    ],
    try_nodes: &[], // C doesn't have try/catch
    var_declaration_nodes: &["declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["call_expression"],
    await_nodes: &[],
    import_nodes: &["preproc_include"],
    name_field: "declarator",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: c_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["extern", "static"],
    decorator_nodes: &[],
};

pub static CPP_GRAMMAR: LangGrammar = LangGrammar {
    name: "cpp",
    function_nodes: &["function_definition"],
    class_nodes: &["struct_specifier", "class_specifier"],
    interface_nodes: &[], // C++ uses abstract classes
    enum_nodes: &["enum_specifier"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "for_range_loop",
        "while_statement",
        "do_statement",
        "switch_statement",
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &["declaration"],
    assignment_nodes: &["assignment_expression"],
    call_nodes: &["call_expression"],
    await_nodes: &["co_await_expression"],
    import_nodes: &["preproc_include"],
    name_field: "declarator",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: c_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "protected", "private", "extern"],
    decorator_nodes: &["attribute_declaration"],
};

// =============================================================================
// Kotlin Grammar
// =============================================================================

/// Kotlin: similar to Java visibility rules
pub fn kotlin_is_exported(node: &Node, source: &str) -> bool {
    // Check for visibility modifiers in the node text
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    // Internal, private, protected are restrictive; no modifier or public = exported
    !text.contains("private ") && !text.contains("internal ") && !text.contains("protected ")
}

pub static KOTLIN_GRAMMAR: LangGrammar = LangGrammar {
    name: "kotlin",
    function_nodes: &["function_declaration"],
    class_nodes: &["class_declaration", "object_declaration"],
    interface_nodes: &["interface_declaration"],
    enum_nodes: &["enum_class_body"],
    control_flow_nodes: &[
        "if_expression",
        "when_expression",
        "for_statement",
        "while_statement",
        "do_while_statement",
    ],
    try_nodes: &["try_expression"],
    var_declaration_nodes: &["property_declaration", "variable_declaration"],
    assignment_nodes: &["assignment"],
    call_nodes: &["call_expression"],
    await_nodes: &[], // Kotlin uses coroutines, not await expressions
    import_nodes: &["import_header"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: kotlin_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "private", "protected", "internal"],
    decorator_nodes: &["annotation"],
};

// =============================================================================
// Shell/Bash Grammar
// =============================================================================

pub static BASH_GRAMMAR: LangGrammar = LangGrammar {
    name: "bash",
    function_nodes: &["function_definition"],
    class_nodes: &[],
    interface_nodes: &[],
    enum_nodes: &[],
    control_flow_nodes: &[
        "if_statement",
        "case_statement",
        "for_statement",
        "while_statement",
        "until_statement",
    ],
    try_nodes: &[], // Bash uses trap, not try/catch
    var_declaration_nodes: &["variable_assignment"],
    assignment_nodes: &["variable_assignment"],
    call_nodes: &["command"],
    await_nodes: &[],
    import_nodes: &["command"], // source/dot commands act as imports
    name_field: "name",
    value_field: "value",
    type_field: "",
    body_field: "body",
    params_field: "",
    condition_field: "condition",
    is_exported: default_is_exported, // Shell functions are always "exported" in current context
    uppercase_is_export: false,
    visibility_modifiers: &[],
    decorator_nodes: &[],
};

// =============================================================================
// Gradle (Groovy-based) Grammar
// =============================================================================

pub static GRADLE_GRAMMAR: LangGrammar = LangGrammar {
    name: "gradle",
    // Groovy AST uses function_definition for `def func()` style
    function_nodes: &["function_definition"],
    class_nodes: &["class_definition"],
    interface_nodes: &["interface_definition"],
    enum_nodes: &["enum_definition"],
    control_flow_nodes: &[
        "if_statement",
        "for_statement",
        "for_in_statement",
        "while_statement",
        "switch_statement",
    ],
    try_nodes: &["try_statement"],
    var_declaration_nodes: &["variable_definition", "assignment"],
    assignment_nodes: &["assignment"],
    // Groovy has method_invocation for foo() and juxt_function_call for foo "bar"
    call_nodes: &["method_invocation", "juxt_function_call"],
    await_nodes: &[],
    import_nodes: &["import_statement"],
    name_field: "name",
    value_field: "value",
    type_field: "type",
    body_field: "body",
    params_field: "formal_parameters",
    condition_field: "condition",
    is_exported: default_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "private", "protected"],
    decorator_nodes: &["annotation"],
};

// =============================================================================
// HCL/Terraform Grammar
// =============================================================================

/// HCL: blocks are exported by default (infrastructure as code)
pub fn hcl_is_exported(_node: &Node, _source: &str) -> bool {
    // All HCL blocks are "public" - they define infrastructure
    true
}

pub static HCL_GRAMMAR: LangGrammar = LangGrammar {
    name: "hcl",
    // HCL blocks act like symbol definitions
    function_nodes: &["block"],
    class_nodes: &[],
    interface_nodes: &[],
    enum_nodes: &[],
    control_flow_nodes: &[
        // HCL uses expressions for conditionals
        "conditional",
        "for_tuple_expr",
        "for_object_expr",
    ],
    try_nodes: &[],
    var_declaration_nodes: &["attribute"],
    assignment_nodes: &["attribute"],
    // HCL function calls and references
    call_nodes: &["function_call"],
    await_nodes: &[],
    import_nodes: &[], // HCL uses module blocks for imports
    name_field: "identifier",
    value_field: "expression",
    type_field: "",
    body_field: "body",
    params_field: "",
    condition_field: "condition",
    is_exported: hcl_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &[],
    decorator_nodes: &[],
};

// =============================================================================
// Grammar Lookup
// =============================================================================

/// Get the grammar for a language by name
pub fn get_grammar(lang_name: &str) -> Option<&'static LangGrammar> {
    match lang_name.to_lowercase().as_str() {
        "rust" | "rs" => Some(&RUST_GRAMMAR),
        "go" => Some(&GO_GRAMMAR),
        "java" => Some(&JAVA_GRAMMAR),
        "csharp" | "c#" | "cs" => Some(&CSHARP_GRAMMAR),
        "python" | "py" => Some(&PYTHON_GRAMMAR),
        "javascript" | "js" | "jsx" => Some(&JAVASCRIPT_GRAMMAR),
        "typescript" | "ts" | "tsx" => Some(&TYPESCRIPT_GRAMMAR),
        "c" => Some(&C_GRAMMAR),
        "cpp" | "c++" | "cc" | "cxx" => Some(&CPP_GRAMMAR),
        "kotlin" | "kt" | "kts" => Some(&KOTLIN_GRAMMAR),
        "bash" | "sh" | "shell" => Some(&BASH_GRAMMAR),
        "gradle" | "groovy" => Some(&GRADLE_GRAMMAR),
        "hcl" | "tf" | "terraform" => Some(&HCL_GRAMMAR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_lookup() {
        assert!(get_grammar("rust").is_some());
        assert!(get_grammar("go").is_some());
        assert!(get_grammar("java").is_some());
        assert!(get_grammar("python").is_some());
        assert!(get_grammar("javascript").is_some());
        assert!(get_grammar("typescript").is_some());
        assert!(get_grammar("c").is_some());
        assert!(get_grammar("cpp").is_some());
        assert!(get_grammar("unknown").is_none());
    }

    #[test]
    fn test_grammar_completeness() {
        // Ensure all grammars have the essential fields populated
        let grammars = [
            &RUST_GRAMMAR,
            &GO_GRAMMAR,
            &JAVA_GRAMMAR,
            &CSHARP_GRAMMAR,
            &PYTHON_GRAMMAR,
            &JAVASCRIPT_GRAMMAR,
            &TYPESCRIPT_GRAMMAR,
            &C_GRAMMAR,
            &CPP_GRAMMAR,
        ];

        for grammar in grammars {
            assert!(!grammar.name.is_empty(), "{} has no name", grammar.name);
            assert!(
                !grammar.function_nodes.is_empty(),
                "{} has no function nodes",
                grammar.name
            );
            assert!(
                !grammar.control_flow_nodes.is_empty(),
                "{} has no control flow nodes",
                grammar.name
            );
            assert!(
                !grammar.call_nodes.is_empty(),
                "{} has no call nodes",
                grammar.name
            );
        }
    }
}
