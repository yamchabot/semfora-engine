//! Config file detector (JSON, YAML, TOML)

use tree_sitter::{Node, Tree};
use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::error::Result;
use crate::lang::Lang;
use crate::schema::SemanticSummary;

pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree, lang: Lang) -> Result<()> {
    let root = tree.root_node();

    match lang {
        Lang::Json => extract_json_structure(summary, source, &root),
        Lang::Yaml => extract_yaml_structure(summary, source, &root),
        Lang::Toml => extract_toml_structure(summary, source, &root),
        _ => {}
    }

    generate_insertions(summary);
    summary.extraction_complete = true;
    Ok(())
}

fn extract_json_structure(summary: &mut SemanticSummary, source: &str, root: &Node) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "object" {
            extract_json_keys(summary, source, &child, 0);
        }
    }
}

fn extract_json_keys(summary: &mut SemanticSummary, source: &str, node: &Node, depth: usize) {
    if depth > 1 { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "pair" {
            if let Some(key) = child.child_by_field_name("key") {
                let key_str = get_node_text(&key, source);
                let key_clean = key_str.trim_matches('"');
                if is_meaningful_key(key_clean) {
                    summary.added_dependencies.push(key_clean.to_string());
                }
                if let Some(value) = child.child_by_field_name("value") {
                    if value.kind() == "object" {
                        extract_json_keys(summary, source, &value, depth + 1);
                    }
                }
            }
        }
    }
}

fn extract_yaml_structure(summary: &mut SemanticSummary, source: &str, root: &Node) {
    visit_all(root, |node| {
        if node.kind() == "block_mapping_pair" {
            if let Some(key) = node.child_by_field_name("key") {
                let key_str = get_node_text(&key, source);
                if is_meaningful_key(&key_str) {
                    summary.added_dependencies.push(key_str);
                }
            }
        }
    });
}

fn extract_toml_structure(summary: &mut SemanticSummary, source: &str, root: &Node) {
    visit_all(root, |node| {
        if node.kind() == "table" || node.kind() == "table_array_element" {
            let table_text = get_node_text(node, source);
            if let Some(name) = table_text.lines().next() {
                let name = name.trim_matches('[').trim_matches(']').trim();
                if !name.is_empty() {
                    summary.added_dependencies.push(name.to_string());
                }
            }
        } else if node.kind() == "pair" {
            if let Some(key) = node.child(0) {
                let key_str = get_node_text(&key, source);
                if is_meaningful_key(&key_str) {
                    summary.added_dependencies.push(key_str);
                }
            }
        }
    });
}

fn is_meaningful_key(key: &str) -> bool {
    matches!(key,
        "name" | "version" | "description" | "main" | "type" | "license"
        | "scripts" | "dependencies" | "devDependencies" | "peerDependencies"
        | "engines" | "repository" | "author" | "keywords"
        | "image" | "services" | "volumes" | "ports" | "environment"
        | "schema" | "dialect" | "dbCredentials"
        | "compilerOptions" | "include" | "exclude" | "extends"
        | "plugins" | "rules" | "settings"
    )
}

fn generate_insertions(summary: &mut SemanticSummary) {
    let file_lower = summary.file.to_lowercase();

    if file_lower.ends_with("package.json") {
        let has_deps = summary.added_dependencies.iter().any(|d| d.contains("ependencies"));
        if has_deps {
            push_unique_insertion(&mut summary.insertions, "npm package manifest".to_string(), "npm");
        }
    } else if file_lower.ends_with("tsconfig.json") {
        push_unique_insertion(&mut summary.insertions, "TypeScript configuration".to_string(), "TypeScript");
    } else if file_lower.contains("docker-compose") {
        if summary.added_dependencies.iter().any(|d| d == "services") {
            push_unique_insertion(&mut summary.insertions, "Docker Compose configuration".to_string(), "Docker");
        }
    } else if file_lower.contains("eslint") {
        push_unique_insertion(&mut summary.insertions, "ESLint configuration".to_string(), "ESLint");
    } else if file_lower.contains("prettier") {
        push_unique_insertion(&mut summary.insertions, "Prettier configuration".to_string(), "Prettier");
    } else if file_lower.ends_with("cargo.toml") {
        push_unique_insertion(&mut summary.insertions, "Rust package manifest".to_string(), "Rust package");
    }

    let config_keys: Vec<String> = summary.added_dependencies.drain(..).collect();
    if !config_keys.is_empty() && summary.insertions.is_empty() {
        let key_summary = if config_keys.len() <= 3 {
            config_keys.join(", ")
        } else {
            format!("{} sections", config_keys.len())
        };
        summary.insertions.push(format!("config with {}", key_summary));
    }
}
