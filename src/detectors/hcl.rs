//! HCL/Terraform detector
//!
//! Extracts semantic information from HCL files (Terraform .tf, .hcl, .tfvars).
//! HCL has unique semantics where "blocks" define infrastructure resources.
//!
//! # Supported Blocks
//! - `resource "type" "name"` - Infrastructure resources
//! - `data "type" "name"` - Data sources
//! - `module "name"` - Module calls
//! - `variable "name"` - Input variables
//! - `output "name"` - Output values
//! - `locals` - Local values
//! - `provider "name"` - Provider configuration
//! - `terraform` - Terraform configuration

use tree_sitter::{Node, Tree};

use crate::detectors::common::{find_containing_symbol_by_line, get_node_text, visit_all};
use crate::error::Result;
use crate::schema::{Call, RefKind, RiskLevel, SemanticSummary, StateChange, SymbolInfo, SymbolKind};
use crate::utils::truncate_to_char_boundary;

/// Extract semantic information from an HCL file
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree) -> Result<()> {
    let root = tree.root_node();

    // Extract blocks as symbols
    extract_blocks(summary, &root, source);

    // Extract function calls and attribute references
    extract_calls(summary, &root, source);

    // Extract attributes as state changes
    extract_attributes(summary, &root, source);

    // Set primary symbol to first resource/module block
    if let Some(first_symbol) = summary.symbols.first() {
        summary.symbol = Some(first_symbol.name.clone());
        summary.symbol_kind = Some(first_symbol.kind);
        summary.start_line = Some(first_symbol.start_line);
        summary.end_line = Some(first_symbol.end_line);
        summary.public_surface_changed = true; // HCL defines infrastructure
    }

    Ok(())
}

/// Extract HCL blocks as symbols
fn extract_blocks(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "block" {
            if let Some(symbol) = extract_block_symbol(node, source) {
                summary.symbols.push(symbol);
            }
        }
    });
}

/// Extract a symbol from an HCL block
fn extract_block_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    // HCL blocks have the structure:
    // block [block_type] [labels...] { body }
    // e.g., resource "aws_instance" "web" { ... }
    //       locals { ... }

    let mut cursor = node.walk();
    let mut block_type: Option<String> = None;
    let mut labels: Vec<String> = Vec::new();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // First identifier is the block type
                if block_type.is_none() {
                    block_type = Some(get_node_text(&child, source));
                }
            }
            "string_lit" => {
                // String literals are labels
                let text = get_node_text(&child, source);
                // Remove quotes
                let label = text.trim_matches('"').to_string();
                labels.push(label);
            }
            _ => {}
        }
    }

    let block_type = block_type?;

    // Construct the symbol name based on block type
    let (name, kind) = match block_type.as_str() {
        "resource" => {
            // resource "type" "name" -> type.name
            if labels.len() >= 2 {
                (format!("{}.{}", labels[0], labels[1]), SymbolKind::Struct)
            } else if labels.len() == 1 {
                (labels[0].clone(), SymbolKind::Struct)
            } else {
                return None;
            }
        }
        "data" => {
            // data "type" "name" -> data.type.name
            if labels.len() >= 2 {
                (
                    format!("data.{}.{}", labels[0], labels[1]),
                    SymbolKind::Struct,
                )
            } else if labels.len() == 1 {
                (format!("data.{}", labels[0]), SymbolKind::Struct)
            } else {
                return None;
            }
        }
        "module" => {
            // module "name" -> module.name
            if !labels.is_empty() {
                (format!("module.{}", labels[0]), SymbolKind::Module)
            } else {
                return None;
            }
        }
        "variable" => {
            // variable "name" -> var.name
            if !labels.is_empty() {
                (format!("var.{}", labels[0]), SymbolKind::Function)
            } else {
                return None;
            }
        }
        "output" => {
            // output "name"
            if !labels.is_empty() {
                (format!("output.{}", labels[0]), SymbolKind::Function)
            } else {
                return None;
            }
        }
        "locals" => {
            // locals block
            ("locals".to_string(), SymbolKind::Module)
        }
        "provider" => {
            // provider "name"
            if !labels.is_empty() {
                (format!("provider.{}", labels[0]), SymbolKind::Module)
            } else {
                ("provider".to_string(), SymbolKind::Module)
            }
        }
        "terraform" => ("terraform".to_string(), SymbolKind::Module),
        _ => {
            // Unknown block type, use as-is with labels
            if !labels.is_empty() {
                (
                    format!("{}.{}", block_type, labels.join(".")),
                    SymbolKind::Struct,
                )
            } else {
                (block_type, SymbolKind::Struct)
            }
        }
    };

    Some(SymbolInfo {
        name,
        kind,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        is_exported: true, // HCL blocks are always "exported"
        is_default_export: false,
        hash: None,
        arguments: Vec::new(),
        props: Vec::new(),
        return_type: None,
        calls: Vec::new(),
        control_flow: Vec::new(),
        state_changes: Vec::new(),
        behavioral_risk: RiskLevel::Low,
        decorators: Vec::new(),
    })
}

/// Extract function calls from HCL expressions
fn extract_calls(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Collect all calls with their line numbers
    let mut all_calls: Vec<(Call, usize)> = Vec::new();

    visit_all(root, |node| {
        if node.kind() == "function_call" {
            if let Some(call) = extract_function_call(node, source) {
                let line = node.start_position().row + 1;
                all_calls.push((call, line));
            }
        }
    });

    // Attribute calls to symbols based on line ranges
    let mut calls_by_symbol: std::collections::HashMap<usize, Vec<Call>> =
        std::collections::HashMap::new();
    let mut file_level_calls: Vec<Call> = Vec::new();

    for (call, line) in all_calls {
        if let Some(symbol_idx) = find_containing_symbol_by_line(line, &summary.symbols) {
            calls_by_symbol.entry(symbol_idx).or_default().push(call);
        } else {
            file_level_calls.push(call);
        }
    }

    // Assign calls to their respective symbols (deduplicated per symbol)
    for (symbol_idx, calls) in calls_by_symbol {
        if symbol_idx < summary.symbols.len() {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for call in calls {
                let key = format!("{}:{}", call.name, call.is_awaited);
                if !seen.contains(&key) {
                    seen.insert(key);
                    summary.symbols[symbol_idx].calls.push(call);
                }
            }
        }
    }

    // Keep file-level calls in summary.calls for backward compatibility
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for call in file_level_calls {
        let key = format!("{}:{}", call.name, call.is_awaited);
        if !seen.contains(&key) {
            seen.insert(key);
            summary.calls.push(call);
        }
    }
}

/// Extract a function call from a function_call node
fn extract_function_call(node: &Node, source: &str) -> Option<Call> {
    // function_call has children: identifier, (, arguments, )
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let name = get_node_text(&child, source);
            return Some(Call {
                name,
                object: None,
                is_awaited: false,
                in_try: false,
                is_hook: false,
                is_io: false,
                ref_kind: RefKind::None,
                location: crate::schema::Location {
                    line: node.start_position().row + 1,
                    column: node.start_position().column,
                },
            });
        }
    }
    None
}

/// Extract attributes as state changes
fn extract_attributes(summary: &mut SemanticSummary, root: &Node, source: &str) {
    visit_all(root, |node| {
        if node.kind() == "attribute" {
            if let Some(state_change) = extract_attribute_state_change(node, source) {
                let line = node.start_position().row + 1;

                // Try to attribute to a symbol
                if let Some(symbol_idx) = find_containing_symbol_by_line(line, &summary.symbols) {
                    if symbol_idx < summary.symbols.len() {
                        summary.symbols[symbol_idx].state_changes.push(state_change);
                    }
                } else {
                    summary.state_changes.push(state_change);
                }
            }
        }
    });
}

/// Extract a state change from an attribute node
fn extract_attribute_state_change(node: &Node, source: &str) -> Option<StateChange> {
    let mut cursor = node.walk();
    let mut name: Option<String> = None;
    let mut initializer = String::new();

    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" && name.is_none() {
            name = Some(get_node_text(&child, source));
        } else if child.kind() == "expression" {
            let init_text = get_node_text(&child, source);
            // Truncate long initializers
            initializer = if init_text.len() > 50 {
                format!("{}...", truncate_to_char_boundary(&init_text, 47))
            } else {
                init_text
            };
        }
    }

    Some(StateChange {
        name: name?,
        state_type: "hcl".to_string(),
        initializer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_hcl(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_hcl::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_hcl_resource_extraction() {
        let source = r#"
resource "aws_instance" "web" {
    ami           = data.aws_ami.ubuntu.id
    instance_type = var.instance_type
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 1);
        assert_eq!(summary.symbols[0].name, "aws_instance.web");
        assert_eq!(summary.symbols[0].kind, SymbolKind::Struct);
        assert!(summary.symbols[0].state_changes.len() >= 2);
    }

    #[test]
    fn test_hcl_multiple_blocks() {
        let source = r#"
resource "aws_instance" "web" {
    ami = "ami-123"
}

locals {
    common_tags = {
        Environment = "prod"
    }
}

output "instance_id" {
    value = aws_instance.web.id
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 3);

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"aws_instance.web"));
        assert!(names.contains(&"locals"));
        assert!(names.contains(&"output.instance_id"));
    }

    #[test]
    fn test_hcl_function_call_attribution() {
        let source = r#"
resource "aws_instance" "web" {
    tags = merge(local.common_tags, {
        Name = "WebServer"
    })
}

locals {
    common_tags = tomap({
        Environment = "prod"
    })
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        // Find the resource and locals blocks
        let resource_symbol = summary
            .symbols
            .iter()
            .find(|s| s.name == "aws_instance.web")
            .expect("Should have resource symbol");
        let locals_symbol = summary
            .symbols
            .iter()
            .find(|s| s.name == "locals")
            .expect("Should have locals symbol");

        // Check that calls are attributed correctly
        let resource_calls: Vec<&str> = resource_symbol
            .calls
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        let locals_calls: Vec<&str> = locals_symbol
            .calls
            .iter()
            .map(|c| c.name.as_str())
            .collect();

        assert!(
            resource_calls.contains(&"merge"),
            "Resource should have merge() call"
        );
        assert!(
            locals_calls.contains(&"tomap"),
            "Locals should have tomap() call"
        );
    }

    #[test]
    fn test_hcl_data_source() {
        let source = r#"
data "aws_ami" "ubuntu" {
    most_recent = true
    filter {
        name   = "name"
        values = ["ubuntu/images/*"]
    }
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 2); // data block + filter block
        assert_eq!(summary.symbols[0].name, "data.aws_ami.ubuntu");
    }

    #[test]
    fn test_hcl_module_block() {
        let source = r#"
module "vpc" {
    source = "./modules/vpc"
    cidr   = "10.0.0.0/16"
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 1);
        assert_eq!(summary.symbols[0].name, "module.vpc");
        assert_eq!(summary.symbols[0].kind, SymbolKind::Module);
    }

    #[test]
    fn test_hcl_variable_and_output() {
        let source = r#"
variable "instance_type" {
    description = "EC2 instance type"
    default     = "t2.micro"
}

output "instance_id" {
    value = aws_instance.web.id
}
"#;
        let tree = parse_hcl(source);
        let mut summary = SemanticSummary::default();

        extract(&mut summary, source, &tree).unwrap();

        assert_eq!(summary.symbols.len(), 2);

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"var.instance_type"));
        assert!(names.contains(&"output.instance_id"));

        // Variables and outputs use Function kind
        for symbol in &summary.symbols {
            assert_eq!(symbol.kind, SymbolKind::Function);
        }
    }
}
