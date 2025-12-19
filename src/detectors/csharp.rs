//! C# language detector
//!
//! Extracts semantic information from C# source files using the generic extractor.
//! C# supports classes, records, structs, interfaces, enums, and modern features
//! like async/await, pattern matching, and nullable types.
//!
//! # Supported Constructs
//!
//! - **Symbols**: class_declaration, struct_declaration, record_declaration,
//!   interface_declaration, enum_declaration, method_declaration,
//!   constructor_declaration, local_function_statement
//! - **Imports**: using_directive
//! - **State changes**: local_declaration_statement, field_declaration,
//!   property_declaration, assignment_expression
//! - **Control flow**: if, for, foreach, while, do, switch, switch_expression, try
//! - **Calls**: invocation_expression
//! - **Async**: await_expression

use tree_sitter::Tree;

use crate::detectors::generic::extract_with_grammar;
use crate::detectors::grammar::CSHARP_GRAMMAR;
use crate::error::Result;
use crate::schema::SemanticSummary;

/// Extract semantic information from a C# source file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    extract_with_grammar(summary, source, tree, &CSHARP_GRAMMAR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn parse_source(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&Lang::CSharp.tree_sitter_language())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_csharp_class_extraction() {
        let source = r#"
using System;
using System.Collections.Generic;

namespace MyApp
{
    public class UserService
    {
        private readonly IRepository _repository;

        public UserService(IRepository repository)
        {
            _repository = repository;
        }

        public List<User> GetUsers()
        {
            var users = _repository.GetAll();
            return users;
        }
    }
}
"#;
        let tree = parse_source(source);
        let mut summary = SemanticSummary {
            file: "/test/UserService.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        // Should detect the class and methods
        assert!(!summary.symbols.is_empty(), "Should detect symbols");
    }

    #[test]
    fn test_csharp_async_await() {
        let source = r#"
using System.Threading.Tasks;
using System.Net.Http;

public class ApiService
{
    private readonly HttpClient _httpClient;

    public async Task<User> GetUserAsync(int id)
    {
        var response = await _httpClient.GetAsync($"/users/{id}");
        var content = await response.Content.ReadAsStringAsync();
        return JsonSerializer.Deserialize<User>(content);
    }
}
"#;
        let tree = parse_source(source);
        let mut summary = SemanticSummary {
            file: "/test/ApiService.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        // Should detect symbols
        assert!(!summary.symbols.is_empty(), "Should detect async method");
    }

    #[test]
    fn test_csharp_record_types() {
        let source = r#"
namespace MyApp.Models
{
    public record User(string Name, int Age);

    public record class Address
    {
        public string Street { get; init; }
        public string City { get; init; }
    }
}
"#;
        let tree = parse_source(source);
        let mut summary = SemanticSummary {
            file: "/test/Models.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        // Should detect record types
        assert!(!summary.symbols.is_empty(), "Should detect record types");
    }

    #[test]
    fn test_csharp_control_flow() {
        let source = r#"
public class Calculator
{
    public string Classify(int value)
    {
        if (value > 0)
        {
            return "positive";
        }
        else if (value < 0)
        {
            return "negative";
        }
        else
        {
            return "zero";
        }
    }

    public int Sum(int[] numbers)
    {
        int total = 0;
        foreach (var n in numbers)
        {
            total += n;
        }
        return total;
    }
}
"#;
        let tree = parse_source(source);
        let mut summary = SemanticSummary {
            file: "/test/Calculator.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        // Should detect control flow
        assert!(
            !summary.symbols.is_empty(),
            "Should detect methods with control flow"
        );
    }

    #[test]
    fn test_csharp_ast_debug() {
        let source = r#"
public class TestController : Controller
{
    public IActionResult Index()
    {
        return View();
    }

    public async Task<IActionResult> List(int page)
    {
        return Ok();
    }
}
"#;
        fn print_ast(node: &tree_sitter::Node, depth: usize) {
            let indent = "  ".repeat(depth);
            println!(
                "{}{} [L{}-L{}]",
                indent,
                node.kind(),
                node.start_position().row + 1,
                node.end_position().row + 1
            );

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                print_ast(&child, depth + 1);
            }
        }

        let tree = parse_source(source);
        println!("\n=== C# AST Structure ===");
        print_ast(&tree.root_node(), 0);
        println!("========================\n");

        let mut summary = SemanticSummary {
            file: "/test/TestController.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        println!("\n=== Extracted Symbols ===");
        for sym in &summary.symbols {
            println!(
                "  {} ({:?}) lines {}-{}",
                sym.name, sym.kind, sym.start_line, sym.end_line
            );
        }
        println!("=========================\n");

        // We should find: class + 2 methods = 3 symbols
        assert!(
            summary.symbols.len() >= 3,
            "Expected at least 3 symbols (class + 2 methods), got {}",
            summary.symbols.len()
        );
    }

    #[test]
    fn test_csharp_switch_expression() {
        let source = r#"
public class Matcher
{
    public string GetDescription(object obj) => obj switch
    {
        int i when i > 0 => "positive integer",
        int i when i < 0 => "negative integer",
        string s => $"string: {s}",
        null => "null",
        _ => "unknown"
    };
}
"#;
        let tree = parse_source(source);
        let mut summary = SemanticSummary {
            file: "/test/Matcher.cs".to_string(),
            ..Default::default()
        };

        extract(&mut summary, source, &tree).unwrap();

        // Should detect switch expression
        assert!(
            !summary.symbols.is_empty(),
            "Should detect pattern matching"
        );
    }
}
