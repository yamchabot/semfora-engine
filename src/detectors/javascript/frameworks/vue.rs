//! Vue.js Framework Detector
//!
//! Specialized extraction for Vue.js applications including:
//! - Vue Single File Component (.vue) parsing
//! - Composition API (ref, reactive, computed, watch)
//! - Options API (data, methods, computed, watch)
//! - defineComponent and script setup
//! - Lifecycle hooks
//! - Props and emits definitions

use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::lang::Lang;
use crate::schema::SemanticSummary;

// =============================================================================
// Vue SFC Parsing
// =============================================================================

/// Extracted script content from a Vue SFC
#[derive(Debug)]
pub struct VueSfcScript {
    /// The script content (without script tags)
    pub content: String,
    /// The language (ts, tsx, js, jsx)
    pub lang: Lang,
    /// Starting line of the script in the original file
    pub start_line: usize,
    /// Whether this is a script setup block
    pub is_setup: bool,
}

/// Extract the script section from a Vue SFC
///
/// Parses the .vue file to find the `<script>` or `<script setup>` tag,
/// extracts its content and determines the language from the lang attribute.
pub fn extract_sfc_script(source: &str) -> Option<VueSfcScript> {
    // Try to find <script setup> first (preferred in Vue 3.2+)
    if let Some(script) = extract_script_tag(source, true) {
        return Some(script);
    }

    // Fall back to regular <script>
    extract_script_tag(source, false)
}

/// Extract a specific script tag (setup or regular)
fn extract_script_tag(source: &str, setup: bool) -> Option<VueSfcScript> {
    let pattern = if setup { "<script setup" } else { "<script" };

    // Find the opening tag
    let tag_start = source.find(pattern)?;

    // Find the end of the opening tag
    let after_tag = &source[tag_start..];
    let tag_end_offset = after_tag.find('>')?;
    let opening_tag = &after_tag[..tag_end_offset + 1];

    // Detect language from lang attribute
    let lang = detect_script_lang(opening_tag);

    // Find closing </script>
    let content_start = tag_start + tag_end_offset + 1;
    let closing_tag = "</script>";
    let content_end = source[content_start..].find(closing_tag)?;

    let content = source[content_start..content_start + content_end].to_string();

    // Calculate start line
    let start_line = source[..content_start].lines().count();

    Some(VueSfcScript {
        content,
        lang,
        start_line,
        is_setup: setup,
    })
}

/// Detect the script language from the opening tag
fn detect_script_lang(tag: &str) -> Lang {
    // Check for lang="ts" or lang="typescript"
    if tag.contains("lang=\"ts\"")
        || tag.contains("lang='ts'")
        || tag.contains("lang=\"typescript\"")
        || tag.contains("lang='typescript'")
    {
        // Check if also has tsx via setup (common pattern)
        if tag.contains("setup") {
            // Script setup with TypeScript - could be TSX if it has JSX
            return Lang::TypeScript;
        }
        return Lang::TypeScript;
    }

    // Check for lang="tsx"
    if tag.contains("lang=\"tsx\"") || tag.contains("lang='tsx'") {
        return Lang::Tsx;
    }

    // Check for lang="jsx"
    if tag.contains("lang=\"jsx\"") || tag.contains("lang='jsx'") {
        return Lang::Jsx;
    }

    // Check for lang="js" or lang="javascript"
    if tag.contains("lang=\"js\"")
        || tag.contains("lang='js'")
        || tag.contains("lang=\"javascript\"")
        || tag.contains("lang='javascript'")
    {
        return Lang::JavaScript;
    }

    // Default to JavaScript
    Lang::JavaScript
}

/// Check if source contains a Vue SFC structure
pub fn is_vue_sfc(source: &str) -> bool {
    // Must have at least one of template/script/style
    (source.contains("<template") || source.contains("<script") || source.contains("<style"))
        && (source.contains("</template>")
            || source.contains("</script>")
            || source.contains("</style>"))
}

/// Enhance semantic summary with Vue-specific information
///
/// This is called when Vue is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Detect API style
    let api_style = detect_api_style(source);

    match api_style {
        ApiStyle::Composition => {
            push_unique_insertion(
                &mut summary.insertions,
                "Vue Composition API".to_string(),
                "Composition API",
            );
            extract_composition_api(summary, root, source);
        }
        ApiStyle::Options => {
            push_unique_insertion(
                &mut summary.insertions,
                "Vue Options API".to_string(),
                "Options API",
            );
            extract_options_api(summary, source);
        }
        ApiStyle::ScriptSetup => {
            push_unique_insertion(
                &mut summary.insertions,
                "Vue script setup".to_string(),
                "script setup",
            );
            extract_script_setup(summary, root, source);
        }
        ApiStyle::Unknown => {}
    }

    // Detect lifecycle hooks
    detect_lifecycle_hooks(summary, source);

    // Detect common patterns
    detect_common_patterns(summary, source);
}

// =============================================================================
// API Style Detection
// =============================================================================

#[derive(Debug, PartialEq)]
enum ApiStyle {
    Composition,
    Options,
    ScriptSetup,
    Unknown,
}

/// Detect which Vue API style is being used
fn detect_api_style(source: &str) -> ApiStyle {
    // Script setup (Vue 3.2+)
    if source.contains("<script setup>") || source.contains("<script setup lang=\"ts\">") {
        return ApiStyle::ScriptSetup;
    }

    // Composition API indicators
    if source.contains("setup(") && (source.contains("return {") || source.contains("return{")) {
        return ApiStyle::Composition;
    }

    // defineComponent with setup
    if source.contains("defineComponent(") && source.contains("setup(") {
        return ApiStyle::Composition;
    }

    // Options API indicators
    if source.contains("data()") || source.contains("data:") {
        if source.contains("methods:") || source.contains("computed:") {
            return ApiStyle::Options;
        }
    }

    // Standalone composition functions
    if source.contains("ref(") || source.contains("reactive(") || source.contains("computed(") {
        return ApiStyle::Composition;
    }

    ApiStyle::Unknown
}

// =============================================================================
// Composition API Extraction
// =============================================================================

/// Extract Composition API patterns
fn extract_composition_api(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut ref_count = 0;
    let mut reactive_count = 0;
    let mut computed_count = 0;
    let mut watch_count = 0;

    visit_all(root, |node| {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                let func_name = get_node_text(&func, source);
                match func_name.as_str() {
                    "ref" | "shallowRef" => ref_count += 1,
                    "reactive" | "shallowReactive" => reactive_count += 1,
                    "computed" => computed_count += 1,
                    "watch" | "watchEffect" => watch_count += 1,
                    _ => {}
                }
            }
        }
    });

    if ref_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} ref(s)", ref_count),
            "ref",
        );
    }

    if reactive_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} reactive object(s)", reactive_count),
            "reactive",
        );
    }

    if computed_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} computed(s)", computed_count),
            "computed",
        );
    }

    if watch_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} watcher(s)", watch_count),
            "watch",
        );
    }
}

// =============================================================================
// Script Setup Extraction
// =============================================================================

/// Extract script setup patterns (Vue 3.2+)
fn extract_script_setup(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Script setup uses the same composition API
    extract_composition_api(summary, root, source);

    // defineProps
    if source.contains("defineProps") {
        push_unique_insertion(
            &mut summary.insertions,
            "defineProps".to_string(),
            "defineProps",
        );
    }

    // defineEmits
    if source.contains("defineEmits") {
        push_unique_insertion(
            &mut summary.insertions,
            "defineEmits".to_string(),
            "defineEmits",
        );
    }

    // defineExpose
    if source.contains("defineExpose") {
        push_unique_insertion(
            &mut summary.insertions,
            "defineExpose".to_string(),
            "defineExpose",
        );
    }

    // defineSlots (Vue 3.3+)
    if source.contains("defineSlots") {
        push_unique_insertion(
            &mut summary.insertions,
            "defineSlots".to_string(),
            "defineSlots",
        );
    }

    // defineOptions (Vue 3.3+)
    if source.contains("defineOptions") {
        push_unique_insertion(
            &mut summary.insertions,
            "defineOptions".to_string(),
            "defineOptions",
        );
    }
}

// =============================================================================
// Options API Extraction
// =============================================================================

/// Extract Options API patterns
fn extract_options_api(summary: &mut SemanticSummary, source: &str) {
    // Data properties
    if source.contains("data()") || source.contains("data:") {
        push_unique_insertion(
            &mut summary.insertions,
            "data properties".to_string(),
            "data",
        );
    }

    // Methods
    if source.contains("methods:") {
        push_unique_insertion(
            &mut summary.insertions,
            "methods defined".to_string(),
            "methods",
        );
    }

    // Computed properties
    if source.contains("computed:") {
        push_unique_insertion(
            &mut summary.insertions,
            "computed properties".to_string(),
            "computed",
        );
    }

    // Watchers
    if source.contains("watch:") {
        push_unique_insertion(&mut summary.insertions, "watchers".to_string(), "watch");
    }

    // Props
    if source.contains("props:") {
        push_unique_insertion(
            &mut summary.insertions,
            "props defined".to_string(),
            "props",
        );
    }

    // Emits
    if source.contains("emits:") {
        push_unique_insertion(
            &mut summary.insertions,
            "emits defined".to_string(),
            "emits",
        );
    }

    // Components
    if source.contains("components:") {
        push_unique_insertion(
            &mut summary.insertions,
            "child components".to_string(),
            "components",
        );
    }

    // Mixins
    if source.contains("mixins:") {
        push_unique_insertion(&mut summary.insertions, "mixins used".to_string(), "mixins");
    }
}

// =============================================================================
// Lifecycle Hook Detection
// =============================================================================

/// Vue 3 Composition API lifecycle hooks
const VUE3_LIFECYCLE_HOOKS: &[(&str, &str)] = &[
    ("onMounted", "mounted hook"),
    ("onUnmounted", "unmounted hook"),
    ("onBeforeMount", "before mount hook"),
    ("onBeforeUnmount", "before unmount hook"),
    ("onUpdated", "updated hook"),
    ("onBeforeUpdate", "before update hook"),
    ("onActivated", "keep-alive activated"),
    ("onDeactivated", "keep-alive deactivated"),
    ("onErrorCaptured", "error boundary"),
];

/// Vue 2/Options API lifecycle hooks
const VUE2_LIFECYCLE_HOOKS: &[(&str, &str)] = &[
    ("mounted()", "mounted hook"),
    ("created()", "created hook"),
    ("beforeMount()", "before mount hook"),
    ("beforeDestroy()", "before destroy hook"),
    ("destroyed()", "destroyed hook"),
    ("updated()", "updated hook"),
    ("beforeUpdate()", "before update hook"),
];

/// Detect lifecycle hooks
fn detect_lifecycle_hooks(summary: &mut SemanticSummary, source: &str) {
    // Vue 3 Composition API hooks
    for (hook, description) in VUE3_LIFECYCLE_HOOKS {
        if source.contains(&format!("{}(", hook)) {
            push_unique_insertion(
                &mut summary.insertions,
                format!("lifecycle: {}", description),
                hook,
            );
        }
    }

    // Vue 2/Options API hooks
    for (hook, description) in VUE2_LIFECYCLE_HOOKS {
        if source.contains(hook) {
            push_unique_insertion(
                &mut summary.insertions,
                format!("lifecycle: {}", description),
                hook,
            );
        }
    }
}

// =============================================================================
// Common Pattern Detection
// =============================================================================

/// Detect common Vue patterns
fn detect_common_patterns(summary: &mut SemanticSummary, source: &str) {
    // Vue Router
    if source.contains("useRoute") || source.contains("useRouter") || source.contains("$router") {
        push_unique_insertion(&mut summary.insertions, "Vue Router".to_string(), "router");
    }

    // Pinia store
    if source.contains("defineStore") || source.contains("useStore") {
        push_unique_insertion(&mut summary.insertions, "Pinia store".to_string(), "Pinia");
    }

    // Vuex store (legacy)
    if source.contains("mapState") || source.contains("mapGetters") || source.contains("$store") {
        push_unique_insertion(&mut summary.insertions, "Vuex store".to_string(), "Vuex");
    }

    // Composables (use* functions)
    if source.contains("use") && source.contains("export function use") {
        push_unique_insertion(
            &mut summary.insertions,
            "composable export".to_string(),
            "composable",
        );
    }

    // Provide/Inject
    if source.contains("provide(") || source.contains("inject(") {
        push_unique_insertion(
            &mut summary.insertions,
            "provide/inject".to_string(),
            "provide/inject",
        );
    }

    // Teleport
    if source.contains("<Teleport") || source.contains("<teleport") {
        push_unique_insertion(
            &mut summary.insertions,
            "Teleport usage".to_string(),
            "Teleport",
        );
    }

    // Suspense
    if source.contains("<Suspense") || source.contains("<suspense") {
        push_unique_insertion(
            &mut summary.insertions,
            "Suspense usage".to_string(),
            "Suspense",
        );
    }

    // i18n
    if source.contains("useI18n") || source.contains("$t(") {
        push_unique_insertion(
            &mut summary.insertions,
            "internationalization".to_string(),
            "i18n",
        );
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Check if file is a Vue component
pub fn is_component(source: &str) -> bool {
    source.contains("defineComponent(")
        || source.contains("<script setup>")
        || source.contains("export default {")
        || source.contains(".vue")
}

/// Check if file is a composable
pub fn is_composable(source: &str) -> bool {
    source.contains("export function use") || source.contains("export const use")
}

/// Check if file is a Pinia store
pub fn is_pinia_store(source: &str) -> bool {
    source.contains("defineStore(")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_api_style() {
        assert_eq!(detect_api_style("<script setup>"), ApiStyle::ScriptSetup);
        assert_eq!(
            detect_api_style("setup() { return {} }"),
            ApiStyle::Composition
        );
        assert_eq!(
            detect_api_style("data() { return {} }, methods: {}"),
            ApiStyle::Options
        );
    }

    #[test]
    fn test_is_component() {
        assert!(is_component("defineComponent({})"));
        assert!(is_component("<script setup>"));
        assert!(!is_component("const x = 1"));
    }

    #[test]
    fn test_is_composable() {
        assert!(is_composable("export function useCounter() {}"));
        assert!(!is_composable("function counter() {}"));
    }

    #[test]
    fn test_is_pinia_store() {
        assert!(is_pinia_store("defineStore('counter', {})"));
        assert!(!is_pinia_store("const store = {}"));
    }

    #[test]
    fn test_extract_sfc_script_basic() {
        let vue_file = r#"
<template>
  <div>Hello</div>
</template>

<script>
export default {
  data() {
    return { count: 0 }
  }
}
</script>
"#;
        let script = extract_sfc_script(vue_file).unwrap();
        assert_eq!(script.lang, Lang::JavaScript);
        assert!(!script.is_setup);
        assert!(script.content.contains("export default"));
    }

    #[test]
    fn test_extract_sfc_script_typescript() {
        let vue_file = r#"
<script lang="ts">
import { defineComponent } from 'vue';
export default defineComponent({});
</script>
"#;
        let script = extract_sfc_script(vue_file).unwrap();
        assert_eq!(script.lang, Lang::TypeScript);
        assert!(!script.is_setup);
    }

    #[test]
    fn test_extract_sfc_script_setup() {
        let vue_file = r#"
<script setup lang="ts">
import { ref } from 'vue';
const count = ref(0);
</script>
"#;
        let script = extract_sfc_script(vue_file).unwrap();
        assert_eq!(script.lang, Lang::TypeScript);
        assert!(script.is_setup);
        assert!(script.content.contains("ref(0)"));
    }

    #[test]
    fn test_extract_sfc_script_tsx() {
        let vue_file = r#"
<script lang="tsx">
export default class Jsx extends Vue {
  render(): JSX.Element {
    return <div>Hello</div>;
  }
}
</script>
"#;
        let script = extract_sfc_script(vue_file).unwrap();
        assert_eq!(script.lang, Lang::Tsx);
    }

    #[test]
    fn test_is_vue_sfc() {
        assert!(is_vue_sfc("<template><div></div></template>"));
        assert!(is_vue_sfc("<script>export default {}</script>"));
        assert!(!is_vue_sfc("const x = 1;"));
        assert!(!is_vue_sfc("<template>")); // No closing tag
    }

    #[test]
    fn test_detect_script_lang() {
        assert_eq!(detect_script_lang("<script>"), Lang::JavaScript);
        assert_eq!(detect_script_lang("<script lang=\"ts\">"), Lang::TypeScript);
        assert_eq!(detect_script_lang("<script lang='ts'>"), Lang::TypeScript);
        assert_eq!(detect_script_lang("<script lang=\"tsx\">"), Lang::Tsx);
        assert_eq!(detect_script_lang("<script lang=\"jsx\">"), Lang::Jsx);
        assert_eq!(
            detect_script_lang("<script setup lang=\"ts\">"),
            Lang::TypeScript
        );
    }
}
