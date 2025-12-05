//! Visual demonstration of SEM-45 Layer Cache Integration
//!
//! Run with: cargo run --example layer_cache_demo

use std::path::PathBuf;
use semfora_mcp::{
    CacheDir, LayerKind, LayeredIndex, SymbolState,
    schema::{SymbolInfo, SymbolKind, RiskLevel},
};

fn main() -> semfora_mcp::Result<()> {
    println!("=== SEM-45: Layer Cache Integration Demo ===\n");

    // Use a temp directory for the demo
    let temp_dir = std::env::temp_dir().join("semfora-layer-demo");
    std::fs::create_dir_all(&temp_dir)?;

    // Create a CacheDir pointing to our temp location
    let cache = CacheDir {
        root: temp_dir.clone(),
        repo_root: std::env::current_dir()?,
        repo_hash: "demo_hash".to_string(),
    };

    println!("Cache location: {}\n", cache.root.display());

    // =========================================================================
    // Step 1: Create a LayeredIndex with some symbols
    // =========================================================================
    println!("Step 1: Creating LayeredIndex with sample symbols...");

    let mut index = LayeredIndex::new();

    // Add symbols to base layer (simulating main branch index)
    let base_symbol = SymbolInfo {
        name: "process_data".to_string(),
        kind: SymbolKind::Function,
        start_line: 10,
        end_line: 50,
        is_exported: true,
        behavioral_risk: RiskLevel::Medium,
        ..Default::default()
    };
    index.base.upsert(
        "hash_base_001".to_string(),
        SymbolState::active_at(base_symbol, PathBuf::from("src/processor.rs")),
    );
    index.base.meta.indexed_sha = Some("abc123def456".to_string());

    // Add symbols to branch layer (simulating feature branch changes)
    let branch_symbol = SymbolInfo {
        name: "validate_input".to_string(),
        kind: SymbolKind::Function,
        start_line: 5,
        end_line: 25,
        is_exported: true,
        behavioral_risk: RiskLevel::Low,
        ..Default::default()
    };
    index.branch.upsert(
        "hash_branch_001".to_string(),
        SymbolState::active_at(branch_symbol, PathBuf::from("src/validator.rs")),
    );
    index.branch.meta.indexed_sha = Some("feature123".to_string());

    // Add symbols to working layer (simulating uncommitted changes)
    let working_symbol = SymbolInfo {
        name: "temp_helper".to_string(),
        kind: SymbolKind::Function,
        start_line: 1,
        end_line: 10,
        is_exported: false,
        behavioral_risk: RiskLevel::Low,
        ..Default::default()
    };
    index.working.upsert(
        "hash_working_001".to_string(),
        SymbolState::active_at(working_symbol, PathBuf::from("src/helpers.rs")),
    );

    // Add to AI layer (ephemeral - won't be saved)
    let ai_symbol = SymbolInfo {
        name: "proposed_refactor".to_string(),
        kind: SymbolKind::Function,
        start_line: 100,
        end_line: 150,
        is_exported: true,
        behavioral_risk: RiskLevel::High,
        ..Default::default()
    };
    index.ai.upsert(
        "hash_ai_001".to_string(),
        SymbolState::active_at(ai_symbol, PathBuf::from("src/refactored.rs")),
    );

    let stats = index.stats();
    println!("  Base layer:    {} symbols", stats.base_symbols);
    println!("  Branch layer:  {} symbols", stats.branch_symbols);
    println!("  Working layer: {} symbols", stats.working_symbols);
    println!("  AI layer:      {} symbols (ephemeral)", stats.ai_symbols);
    println!();

    // =========================================================================
    // Step 2: Save the LayeredIndex to cache
    // =========================================================================
    println!("Step 2: Saving LayeredIndex to cache...");

    cache.save_layered_index(&index)?;

    println!("  Saved to: {}", cache.layers_dir().display());
    println!("  Directories created:");
    for kind in [LayerKind::Base, LayerKind::Branch, LayerKind::Working] {
        if let Some(dir) = cache.layer_dir(kind) {
            println!("    - {}/", kind.as_str());
            for file in ["symbols.jsonl", "deleted.txt", "moves.jsonl", "meta.json"] {
                let path = dir.join(file);
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                println!("        - {} ({} bytes)", file, size);
            }
        }
    }
    let meta_size = std::fs::metadata(cache.layer_meta_path()).map(|m| m.len()).unwrap_or(0);
    println!("    - meta.json ({} bytes)", meta_size);
    println!();

    // =========================================================================
    // Step 3: Load the LayeredIndex back
    // =========================================================================
    println!("Step 3: Loading LayeredIndex from cache...");

    let loaded_index = cache.load_layered_index()?.expect("Should load cached index");

    let loaded_stats = loaded_index.stats();
    println!("  Base layer:    {} symbols", loaded_stats.base_symbols);
    println!("  Branch layer:  {} symbols", loaded_stats.branch_symbols);
    println!("  Working layer: {} symbols", loaded_stats.working_symbols);
    println!("  AI layer:      {} symbols (always fresh after load)", loaded_stats.ai_symbols);
    println!();

    // =========================================================================
    // Step 4: Verify symbol resolution works
    // =========================================================================
    println!("Step 4: Testing symbol resolution...");

    if let Some(symbol) = loaded_index.resolve_symbol("hash_base_001") {
        println!("  Resolved base symbol: {} ({})", symbol.name, symbol.kind.as_str());
    }
    if let Some(symbol) = loaded_index.resolve_symbol("hash_branch_001") {
        println!("  Resolved branch symbol: {} ({})", symbol.name, symbol.kind.as_str());
    }
    if let Some(symbol) = loaded_index.resolve_symbol("hash_working_001") {
        println!("  Resolved working symbol: {} ({})", symbol.name, symbol.kind.as_str());
    }

    // AI symbol should NOT be found (wasn't persisted)
    if loaded_index.resolve_symbol("hash_ai_001").is_none() {
        println!("  AI symbol correctly NOT persisted (ephemeral)");
    }
    println!();

    // =========================================================================
    // Step 5: Show cache structure
    // =========================================================================
    println!("Step 5: Cache directory structure:");
    print_dir_tree(&cache.layers_dir(), 0);
    println!();

    // =========================================================================
    // Step 6: Demonstrate layer shadowing
    // =========================================================================
    println!("Step 6: Demonstrating layer shadowing...");

    // Add a symbol in working that shadows one in base
    let mut shadowing_index = loaded_index;
    let modified_symbol = SymbolInfo {
        name: "process_data".to_string(), // Same name as base
        kind: SymbolKind::Function,
        start_line: 10,
        end_line: 75, // But different end line (modified)
        is_exported: true,
        behavioral_risk: RiskLevel::High, // And higher risk
        ..Default::default()
    };
    shadowing_index.working.upsert(
        "hash_base_001".to_string(), // Same hash shadows base
        SymbolState::active_at(modified_symbol, PathBuf::from("src/processor.rs")),
    );

    // Now resolving should get the working version
    if let Some(symbol) = shadowing_index.resolve_symbol("hash_base_001") {
        println!("  Resolved 'process_data': end_line={}, risk={}",
            symbol.end_line,
            symbol.behavioral_risk.as_str()
        );
        println!("  (Working layer shadows base - end_line 75 > 50, risk high > medium)");
    }
    println!();

    // =========================================================================
    // Step 7: Show actual file contents
    // =========================================================================
    println!("Step 7: Actual file contents:");
    if let Some(dir) = cache.layer_dir(LayerKind::Base) {
        println!("\n--- base/symbols.jsonl ---");
        let content = std::fs::read_to_string(dir.join("symbols.jsonl"))?;
        println!("{}", content);

        println!("--- base/meta.json ---");
        let content = std::fs::read_to_string(dir.join("meta.json"))?;
        println!("{}", content);
    }

    // =========================================================================
    // Cleanup
    // =========================================================================
    println!("\nStep 8: Cleanup...");
    cache.clear_layers()?;
    println!("  Layers cleared.");

    if !cache.has_cached_layers() {
        println!("  Verified: no cached layers remain.");
    }

    println!("\n=== Demo Complete ===");
    Ok(())
}

fn print_dir_tree(path: &std::path::Path, indent: usize) {
    let prefix = "  ".repeat(indent);
    if path.is_dir() {
        println!("{}{}/", prefix, path.file_name().unwrap_or_default().to_string_lossy());
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                print_dir_tree(&entry.path(), indent + 1);
            }
        }
    } else {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!("{}{} ({} bytes)", prefix, path.file_name().unwrap_or_default().to_string_lossy(), size);
    }
}
