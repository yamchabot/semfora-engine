# Adding a New Language

This guide walks through adding support for a new programming language to Semfora.

## Overview

Semfora uses [tree-sitter](https://tree-sitter.github.io/) for parsing. Adding a new language requires:

1. Adding the tree-sitter grammar crate
2. Defining the `Lang` enum variant
3. Creating AST node mappings (`LangGrammar`)
4. (Optional) Creating a dedicated detector for special features
5. Wiring up the dispatcher

## Step 1: Add Tree-Sitter Grammar

Find the tree-sitter grammar for your language on crates.io. Most follow the naming pattern `tree-sitter-{language}`.

**Cargo.toml:**
```toml
[dependencies]
tree-sitter-ruby = "0.21"  # Example for Ruby
```

**Common grammars:**
- `tree-sitter-rust`, `tree-sitter-go`, `tree-sitter-python`
- `tree-sitter-typescript` (includes TypeScript and TSX)
- `tree-sitter-java`, `tree-sitter-kotlin-ng`
- `tree-sitter-c`, `tree-sitter-cpp`

## Step 2: Add Lang Variant

**src/lang.rs:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    // ... existing variants ...
    Ruby,  // Add new variant
}
```

### Extension Mapping

```rust
impl Lang {
    pub fn from_extension(ext: &str) -> Result<Self> {
        match ext.to_lowercase().as_str() {
            // ... existing mappings ...
            "rb" | "rake" | "gemspec" => Ok(Self::Ruby),
            _ => Err(McpDiffError::UnsupportedLanguage {
                extension: ext.to_string(),
            }),
        }
    }
}
```

### Tree-Sitter Language

```rust
impl Lang {
    pub fn tree_sitter_language(&self) -> Language {
        match self {
            // ... existing mappings ...
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        }
    }
}
```

### Language Family

```rust
impl Lang {
    pub fn family(&self) -> LangFamily {
        match self {
            // ... existing mappings ...
            Self::Ruby => LangFamily::Ruby, // Or use existing family if similar
        }
    }
}
```

If creating a new family:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangFamily {
    // ... existing families ...
    Ruby,
}
```

## Step 3: Create Language Grammar

**src/detectors/grammar.rs:**

The `LangGrammar` struct defines AST node mappings for semantic extraction:

```rust
pub static RUBY_GRAMMAR: LangGrammar = LangGrammar {
    name: "ruby",

    // Symbol detection - what AST nodes represent functions/classes?
    function_nodes: &["method", "singleton_method"],
    class_nodes: &["class", "singleton_class"],
    interface_nodes: &["module"],  // Ruby modules
    enum_nodes: &[],

    // Control flow - what nodes create branches?
    control_flow_nodes: &[
        "if", "unless", "case", "when",
        "while", "until", "for",
    ],
    try_nodes: &["begin"],  // Ruby's begin/rescue/end

    // State changes
    var_declaration_nodes: &["assignment"],
    assignment_nodes: &["assignment", "operator_assignment"],

    // Function calls
    call_nodes: &["call", "method_call"],
    await_nodes: &[],  // Ruby doesn't have await

    // Imports
    import_nodes: &["require", "require_relative"],

    // Field names for child access
    name_field: "name",
    value_field: "right",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",

    // Visibility detection
    is_exported: ruby_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "private", "protected"],
    decorator_nodes: &[],
};
```

### Visibility Function

Create a function to determine if a symbol is exported/public:

```rust
/// Ruby: methods after `private` are private
pub fn ruby_is_exported(node: &Node, source: &str) -> bool {
    // Ruby visibility is complex - check for preceding private/protected
    // Simplified: assume public unless proven otherwise
    true
}
```

### Finding AST Node Names

Use `tree-sitter` CLI to explore the grammar:

```bash
# Install tree-sitter CLI
cargo install tree-sitter-cli

# Parse a file and print the AST
tree-sitter parse example.rb

# Output shows node kinds:
# (program
#   (method
#     name: (identifier)
#     parameters: (method_parameters)
#     body: (...)
#   )
# )
```

Or use the `--print-ast` flag in semfora-mcp:

```bash
semfora-mcp example.rb --print-ast
```

## Step 4: Wire Up the Dispatcher

**src/extract.rs:**

```rust
pub fn extract(path: &Path, source: &str, tree: &Tree, lang: Lang) -> Result<SemanticSummary> {
    match lang.family() {
        // ... existing families ...
        LangFamily::Ruby => {
            generic::extract_with_grammar(path, source, tree, &grammar::RUBY_GRAMMAR)
        }
    }
}
```

## Step 5: (Optional) Dedicated Detector

For languages with complex features (frameworks, decorators, etc.), create a dedicated detector:

**src/detectors/ruby.rs:**

```rust
//! Ruby-specific semantic extraction

use crate::schema::SemanticSummary;
use tree_sitter::Tree;
use std::path::Path;

pub fn extract(path: &Path, source: &str, tree: &Tree) -> crate::error::Result<SemanticSummary> {
    // Start with generic extraction
    let mut summary = super::generic::extract_with_grammar(
        path, source, tree, &super::grammar::RUBY_GRAMMAR
    )?;

    // Add Ruby-specific features
    detect_rails_patterns(&mut summary, source, tree);
    detect_rspec_tests(&mut summary, source, tree);

    Ok(summary)
}

fn detect_rails_patterns(summary: &mut SemanticSummary, source: &str, tree: &Tree) {
    // Detect Rails controllers, models, concerns, etc.
}

fn detect_rspec_tests(summary: &mut SemanticSummary, source: &str, tree: &Tree) {
    // Detect RSpec describe/it blocks
}
```

**src/detectors/mod.rs:**

```rust
pub mod ruby;
```

**src/extract.rs:**

```rust
LangFamily::Ruby => detectors::ruby::extract(path, source, tree),
```

## Step 6: Add Tests

**src/detectors/ruby.rs:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ruby_method_extraction() {
        let source = r#"
class User
  def initialize(name)
    @name = name
  end

  def greet
    puts "Hello, #{@name}!"
  end
end
"#;
        let path = Path::new("user.rb");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_ruby::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let summary = extract(path, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 3);  // class + 2 methods
        assert!(summary.symbols.iter().any(|s| s.name == "User"));
        assert!(summary.symbols.iter().any(|s| s.name == "initialize"));
        assert!(summary.symbols.iter().any(|s| s.name == "greet"));
    }
}
```

## Step 7: Update Documentation

**README.md** - Add to the supported languages table:

```markdown
| **Ruby** | `.rb`, `.rake`, `.gemspec` | Ruby | Classes, modules, methods; Rails detection |
```

## Example: Complete Ruby Addition

Here's a minimal but complete example:

### Cargo.toml
```toml
tree-sitter-ruby = "0.21"
```

### src/lang.rs
```rust
pub enum Lang {
    // ...
    Ruby,
}

impl Lang {
    pub fn from_extension(ext: &str) -> Result<Self> {
        match ext.to_lowercase().as_str() {
            // ...
            "rb" | "rake" | "gemspec" => Ok(Self::Ruby),
            _ => Err(...)
        }
    }

    pub fn tree_sitter_language(&self) -> Language {
        match self {
            // ...
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        }
    }

    pub fn family(&self) -> LangFamily {
        match self {
            // ...
            Self::Ruby => LangFamily::Ruby,
        }
    }
}

pub enum LangFamily {
    // ...
    Ruby,
}
```

### src/detectors/grammar.rs
```rust
pub fn ruby_is_exported(_node: &Node, _source: &str) -> bool {
    true  // Simplified
}

pub static RUBY_GRAMMAR: LangGrammar = LangGrammar {
    name: "ruby",
    function_nodes: &["method", "singleton_method"],
    class_nodes: &["class", "singleton_class"],
    interface_nodes: &["module"],
    enum_nodes: &[],
    control_flow_nodes: &["if", "unless", "case", "while", "until", "for"],
    try_nodes: &["begin"],
    var_declaration_nodes: &["assignment"],
    assignment_nodes: &["assignment", "operator_assignment"],
    call_nodes: &["call", "method_call"],
    await_nodes: &[],
    import_nodes: &["call"],  // require/require_relative are method calls
    name_field: "name",
    value_field: "right",
    type_field: "type",
    body_field: "body",
    params_field: "parameters",
    condition_field: "condition",
    is_exported: ruby_is_exported,
    uppercase_is_export: false,
    visibility_modifiers: &["public", "private", "protected"],
    decorator_nodes: &[],
};
```

### src/extract.rs
```rust
LangFamily::Ruby => {
    generic::extract_with_grammar(path, source, tree, &grammar::RUBY_GRAMMAR)
}
```

## Tips

1. **Start with generic extraction** - The `LangGrammar` + generic extractor handles most cases
2. **Use tree-sitter playground** - Many grammars have online playgrounds to explore AST structure
3. **Check existing implementations** - Look at similar languages for patterns
4. **Test with real code** - Use actual project files to validate extraction
5. **Iterate on visibility** - Export detection varies widely between languages

## See Also

- [tree-sitter documentation](https://tree-sitter.github.io/tree-sitter/)
- [Available tree-sitter grammars](https://github.com/tree-sitter)
- [src/detectors/grammar.rs](../src/detectors/grammar.rs) - All grammar definitions
- [src/detectors/generic.rs](../src/detectors/generic.rs) - Generic extractor
