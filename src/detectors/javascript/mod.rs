//! JavaScript/TypeScript/JSX/TSX/Vue language detector
//!
//! This module provides semantic extraction for the JavaScript family of languages,
//! with specialized support for popular frameworks and libraries.
//!
//! # Architecture
//!
//! The JavaScript detector uses a layered approach:
//!
//! 1. **Core extraction** (`core.rs`): Language-agnostic JS/TS extraction
//!    - Symbol detection (functions, classes, arrow functions)
//!    - Import/export handling
//!    - Control flow extraction
//!    - Function call detection
//!
//! 2. **Framework detection** (`frameworks/`): Specialized extractors for:
//!    - React (JSX, hooks, forwardRef/memo, styled-components)
//!    - Next.js (API routes, layouts, pages, app router)
//!    - Express (route handlers, middleware)
//!    - Angular (decorators, services, components)
//!    - Vue (composition API, defineComponent, SFC support)
//!
//! # Supported File Extensions
//!
//! - `.js`, `.mjs`, `.cjs` - JavaScript
//! - `.jsx` - JavaScript with JSX
//! - `.ts`, `.mts`, `.cts` - TypeScript
//! - `.tsx` - TypeScript with JSX
//! - `.vue` - Vue Single File Components
//!
//! # Framework Detection
//!
//! Frameworks are detected based on:
//! - Import statements (e.g., `import React from 'react'`)
//! - File path patterns (e.g., `/app/api/route.ts` for Next.js)
//! - Decorator patterns (e.g., `@Component` for Angular)
//! - Function call patterns (e.g., `app.get()` for Express)
//! - File extension (e.g., `.vue` for Vue SFCs)

pub mod core;
pub mod frameworks;

use tree_sitter::{Parser, Tree};

use crate::detectors::common::push_unique_insertion;
use crate::error::Result;
use crate::lang::Lang;
use crate::schema::SemanticSummary;

pub use frameworks::{detect_frameworks, FrameworkContext};

/// Extract semantic information from a JavaScript/TypeScript file
///
/// This is the main entry point for JS/TS extraction. It:
/// 1. Runs core extraction for symbols, imports, control flow, calls
/// 2. Detects which frameworks are in use
/// 3. Applies framework-specific enhancements
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree, lang: Lang) -> Result<()> {
    let root = tree.root_node();

    // Phase 1: Core JavaScript/TypeScript extraction
    core::extract_core(summary, &root, source, lang)?;

    // Phase 2: Detect frameworks from imports and patterns
    let frameworks = detect_frameworks(summary, source);

    // Phase 3: Apply framework-specific enhancements
    if frameworks.is_react {
        frameworks::react::enhance(summary, &root, source);
    }

    if frameworks.is_nextjs {
        frameworks::nextjs::enhance(summary, source);
    }

    if frameworks.is_express {
        frameworks::express::enhance(summary, &root, source);
    }

    if frameworks.is_angular {
        frameworks::angular::enhance(summary, &root, source);
    }

    if frameworks.is_vue {
        frameworks::vue::enhance(summary, &root, source);
    }

    Ok(())
}

/// Extract semantic information from a Vue Single File Component
///
/// Vue SFCs are handled specially:
/// 1. Parse the SFC to extract the `<script>` section
/// 2. Determine the script language (ts, tsx, js, jsx)
/// 3. Parse the script content with the appropriate grammar
/// 4. Run standard JS/TS extraction on the script
/// 5. Apply Vue-specific enhancements
pub fn extract_vue_sfc(summary: &mut SemanticSummary, source: &str) -> Result<()> {
    // Extract the script section from the SFC
    let Some(sfc_script) = frameworks::vue::extract_sfc_script(source) else {
        // No script section - this is a template-only component
        push_unique_insertion(
            &mut summary.insertions,
            "Vue template-only component".to_string(),
            "template-only",
        );
        return Ok(());
    };

    // Mark as Vue SFC
    push_unique_insertion(&mut summary.insertions, "Vue SFC".to_string(), "Vue SFC");

    if sfc_script.is_setup {
        push_unique_insertion(
            &mut summary.insertions,
            "script setup".to_string(),
            "script setup",
        );
    }

    // Parse the script content with the detected language's grammar
    let mut parser = Parser::new();
    parser
        .set_language(&sfc_script.lang.tree_sitter_language())
        .expect("Error loading grammar");

    let Some(tree) = parser.parse(&sfc_script.content, None) else {
        // Failed to parse script - still process what we can
        return Ok(());
    };

    // Run standard extraction on the script content
    let root = tree.root_node();
    core::extract_core(summary, &root, &sfc_script.content, sfc_script.lang)?;

    // Detect frameworks in the script
    let frameworks = detect_frameworks(summary, &sfc_script.content);

    // Always apply Vue enhancements for .vue files
    frameworks::vue::enhance(summary, &root, &sfc_script.content);

    // Apply other framework enhancements if detected (e.g., Pinia, Vue Router)
    if frameworks.is_react {
        // Unlikely but possible in Vue files with JSX
        frameworks::react::enhance(summary, &root, &sfc_script.content);
    }

    Ok(())
}
