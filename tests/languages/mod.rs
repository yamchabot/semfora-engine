//! Language-specific integration tests for semfora-engine
//!
//! Tests symbol extraction, visibility detection, call graphs, and edge cases
//! for all 27 supported languages, organized by language family.
//!
//! ## Test Categories per Language
//!
//! 1. **Symbol Extraction** - functions, classes, interfaces, enums, types
//! 2. **Visibility Detection** - public/private per language convention
//! 3. **Call Graph** - function calls, method calls, chained calls
//! 4. **Control Flow** - if/for/while/try-catch detection
//! 5. **Edge Cases** - empty files, syntax errors, Unicode, long files

pub mod config_family;
pub mod dotnet_family;
pub mod infrastructure_family;
pub mod javascript_family;
pub mod jvm_family;
pub mod markup_family;
pub mod scripting_family;
pub mod systems_family;
