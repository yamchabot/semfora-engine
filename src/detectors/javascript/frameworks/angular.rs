//! Angular Framework Detector
//!
//! Specialized extraction for Angular applications including:
//! - Component decorators (@Component)
//! - Service/Injectable decorators (@Injectable)
//! - Module decorators (@NgModule)
//! - Directive decorators (@Directive)
//! - Pipe decorators (@Pipe)
//! - Input/Output property decorators
//! - Lifecycle hooks

use tree_sitter::Node;

use crate::detectors::common::{get_node_text, push_unique_insertion, visit_all};
use crate::schema::SemanticSummary;

/// Enhance semantic summary with Angular-specific information
///
/// This is called when Angular is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    // Extract decorator information
    extract_decorators(summary, root, source);

    // Detect lifecycle hooks
    detect_lifecycle_hooks(summary, source);

    // Detect dependency injection
    detect_dependency_injection(summary, root, source);

    // Detect common patterns
    detect_common_patterns(summary, source);
}

// =============================================================================
// Decorator Detection
// =============================================================================

/// Angular decorators and their semantic meaning
const ANGULAR_DECORATORS: &[(&str, &str)] = &[
    ("@Component", "Angular component"),
    ("@Injectable", "Angular service"),
    ("@NgModule", "Angular module"),
    ("@Directive", "Angular directive"),
    ("@Pipe", "Angular pipe"),
    ("@Input", "component input"),
    ("@Output", "component output"),
    ("@ViewChild", "view child query"),
    ("@ViewChildren", "view children query"),
    ("@ContentChild", "content child query"),
    ("@ContentChildren", "content children query"),
    ("@HostBinding", "host binding"),
    ("@HostListener", "host listener"),
];

/// Extract Angular decorators
pub fn extract_decorators(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut found_decorators: Vec<String> = Vec::new();

    visit_all(root, |node| {
        if node.kind() == "decorator" {
            let decorator_text = get_node_text(node, source);

            for (decorator, description) in ANGULAR_DECORATORS {
                if decorator_text.starts_with(decorator) {
                    found_decorators.push(description.to_string());

                    // Extract additional info for specific decorators
                    if *decorator == "@Component" {
                        extract_component_metadata(summary, node, source);
                    } else if *decorator == "@NgModule" {
                        extract_module_metadata(summary, node, source);
                    }
                }
            }
        }
    });

    // Add unique decorator insertions
    for description in found_decorators {
        push_unique_insertion(&mut summary.insertions, description.clone(), &description);
    }
}

/// Extract @Component decorator metadata
fn extract_component_metadata(summary: &mut SemanticSummary, node: &Node, source: &str) {
    let decorator_text = get_node_text(node, source);

    // Extract selector
    if let Some(selector) = extract_decorator_property(&decorator_text, "selector") {
        push_unique_insertion(
            &mut summary.insertions,
            format!("selector: {}", selector),
            "selector",
        );
    }

    // Check for standalone
    if decorator_text.contains("standalone: true") {
        push_unique_insertion(
            &mut summary.insertions,
            "standalone component".to_string(),
            "standalone",
        );
    }

    // Check for template type
    if decorator_text.contains("templateUrl") {
        push_unique_insertion(
            &mut summary.insertions,
            "external template".to_string(),
            "templateUrl",
        );
    } else if decorator_text.contains("template:") {
        push_unique_insertion(
            &mut summary.insertions,
            "inline template".to_string(),
            "inline template",
        );
    }

    // Check for styles
    if decorator_text.contains("styleUrls") || decorator_text.contains("styleUrl") {
        push_unique_insertion(
            &mut summary.insertions,
            "external styles".to_string(),
            "styleUrls",
        );
    } else if decorator_text.contains("styles:") {
        push_unique_insertion(
            &mut summary.insertions,
            "inline styles".to_string(),
            "inline styles",
        );
    }

    // Check for change detection
    if decorator_text.contains("ChangeDetectionStrategy.OnPush") {
        push_unique_insertion(
            &mut summary.insertions,
            "OnPush change detection".to_string(),
            "OnPush",
        );
    }
}

/// Extract @NgModule decorator metadata
fn extract_module_metadata(summary: &mut SemanticSummary, node: &Node, source: &str) {
    let decorator_text = get_node_text(node, source);

    // Count declarations
    if let Some(declarations_count) = count_array_items(&decorator_text, "declarations") {
        if declarations_count > 0 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("{} declarations", declarations_count),
                "declarations",
            );
        }
    }

    // Count imports
    if let Some(imports_count) = count_array_items(&decorator_text, "imports") {
        if imports_count > 0 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("{} module imports", imports_count),
                "imports",
            );
        }
    }

    // Count exports
    if let Some(exports_count) = count_array_items(&decorator_text, "exports") {
        if exports_count > 0 {
            push_unique_insertion(
                &mut summary.insertions,
                format!("{} exports", exports_count),
                "exports",
            );
        }
    }

    // Check for providers
    if decorator_text.contains("providers:") {
        push_unique_insertion(
            &mut summary.insertions,
            "module-level providers".to_string(),
            "providers",
        );
    }

    // Check for bootstrap (root module)
    if decorator_text.contains("bootstrap:") {
        push_unique_insertion(
            &mut summary.insertions,
            "root NgModule".to_string(),
            "bootstrap",
        );
    }
}

/// Extract a property value from decorator text
fn extract_decorator_property(text: &str, property: &str) -> Option<String> {
    let pattern = format!("{}:", property);
    if let Some(start) = text.find(&pattern) {
        let after_colon = &text[start + pattern.len()..];
        let trimmed = after_colon.trim_start();

        // Find the value (could be quoted string)
        if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            let quote = trimmed.chars().next().unwrap();
            if let Some(end) = trimmed[1..].find(quote) {
                return Some(trimmed[1..end + 1].to_string());
            }
        }
    }
    None
}

/// Count items in an array property
fn count_array_items(text: &str, property: &str) -> Option<usize> {
    let pattern = format!("{}:", property);
    if let Some(start) = text.find(&pattern) {
        let after_colon = &text[start + pattern.len()..];
        if let Some(bracket_start) = after_colon.find('[') {
            // Find matching closing bracket
            let mut depth = 0;
            let mut count = 0;
            let mut in_string = false;
            let mut string_char = ' ';

            for c in after_colon[bracket_start..].chars() {
                if !in_string {
                    match c {
                        '\'' | '"' => {
                            in_string = true;
                            string_char = c;
                        }
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        ',' if depth == 1 => count += 1,
                        _ => {}
                    }
                } else if c == string_char {
                    in_string = false;
                }
            }

            // Count is number of commas + 1 (if there's content)
            // Check for empty array: [] or [ ]
            let array_content = &after_colon[bracket_start..];
            let is_empty = array_content
                .trim_start_matches('[')
                .trim()
                .starts_with(']');

            if is_empty {
                return Some(0);
            }
            if count > 0 {
                return Some(count + 1);
            }
            // Single item (no commas but content exists)
            return Some(1);
        }
    }
    None
}

// =============================================================================
// Lifecycle Hook Detection
// =============================================================================

/// Angular lifecycle hooks
const LIFECYCLE_HOOKS: &[(&str, &str)] = &[
    ("ngOnInit", "initialization logic"),
    ("ngOnDestroy", "cleanup logic"),
    ("ngOnChanges", "input change handling"),
    ("ngAfterViewInit", "view initialization"),
    ("ngAfterContentInit", "content initialization"),
    ("ngDoCheck", "custom change detection"),
];

/// Detect lifecycle hooks
fn detect_lifecycle_hooks(summary: &mut SemanticSummary, source: &str) {
    for (hook, description) in LIFECYCLE_HOOKS {
        if source.contains(&format!("{}(", hook)) || source.contains(&format!("{} (", hook)) {
            push_unique_insertion(
                &mut summary.insertions,
                format!("lifecycle: {}", description),
                hook,
            );
        }
    }
}

// =============================================================================
// Dependency Injection Detection
// =============================================================================

/// Detect dependency injection patterns
fn detect_dependency_injection(summary: &mut SemanticSummary, root: &Node, source: &str) {
    let mut injected_count = 0;

    // Count constructor parameters (constructor injection)
    visit_all(root, |node| {
        if node.kind() == "method_definition" {
            if let Some(name) = node.child_by_field_name("name") {
                if get_node_text(&name, source) == "constructor" {
                    if let Some(params) = node.child_by_field_name("parameters") {
                        injected_count += count_constructor_params(&params);
                    }
                }
            }
        }
    });

    // Check for inject() function (newer pattern)
    if source.contains("inject(") {
        push_unique_insertion(
            &mut summary.insertions,
            "inject() function DI".to_string(),
            "inject()",
        );
    }

    if injected_count > 0 {
        push_unique_insertion(
            &mut summary.insertions,
            format!("{} injected dependencies", injected_count),
            "DI",
        );
    }
}

/// Count constructor parameters
fn count_constructor_params(params: &Node) -> usize {
    let mut count = 0;
    let mut cursor = params.walk();

    for child in params.children(&mut cursor) {
        // Count parameters (skip punctuation)
        if child.kind() == "required_parameter" || child.kind() == "optional_parameter" {
            count += 1;
        }
    }

    count
}

// =============================================================================
// Common Pattern Detection
// =============================================================================

/// Detect common Angular patterns
fn detect_common_patterns(summary: &mut SemanticSummary, source: &str) {
    // RxJS observables
    if source.contains("Observable<")
        || source.contains("Subject<")
        || source.contains("BehaviorSubject<")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "RxJS observables".to_string(),
            "Observable",
        );
    }

    // HTTP client
    if source.contains("HttpClient") {
        push_unique_insertion(
            &mut summary.insertions,
            "HTTP client usage".to_string(),
            "HttpClient",
        );
    }

    // Router
    if source.contains("Router") || source.contains("ActivatedRoute") {
        push_unique_insertion(&mut summary.insertions, "routing".to_string(), "Router");
    }

    // Forms
    if source.contains("FormGroup")
        || source.contains("FormControl")
        || source.contains("FormBuilder")
    {
        push_unique_insertion(
            &mut summary.insertions,
            "reactive forms".to_string(),
            "FormGroup",
        );
    }

    if source.contains("ngModel") {
        push_unique_insertion(
            &mut summary.insertions,
            "template-driven forms".to_string(),
            "ngModel",
        );
    }

    // ViewEncapsulation
    if source.contains("ViewEncapsulation.None") {
        push_unique_insertion(
            &mut summary.insertions,
            "no style encapsulation".to_string(),
            "ViewEncapsulation.None",
        );
    } else if source.contains("ViewEncapsulation.ShadowDom") {
        push_unique_insertion(
            &mut summary.insertions,
            "Shadow DOM encapsulation".to_string(),
            "ShadowDom",
        );
    }

    // Signals (Angular 16+)
    if source.contains("signal(") || source.contains("computed(") || source.contains("effect(") {
        push_unique_insertion(
            &mut summary.insertions,
            "Angular signals".to_string(),
            "signals",
        );
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Check if file is an Angular component
pub fn is_component(source: &str) -> bool {
    source.contains("@Component(")
}

/// Check if file is an Angular service
pub fn is_service(source: &str) -> bool {
    source.contains("@Injectable(") && !source.contains("@Component(")
}

/// Check if file is an Angular module
pub fn is_module(source: &str) -> bool {
    source.contains("@NgModule(")
}

/// Check if file is an Angular directive
pub fn is_directive(source: &str) -> bool {
    source.contains("@Directive(")
}

/// Check if file is an Angular pipe
pub fn is_pipe(source: &str) -> bool {
    source.contains("@Pipe(")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_component() {
        assert!(is_component("@Component({ selector: 'app-root' })"));
        assert!(!is_component("@Injectable()"));
    }

    #[test]
    fn test_is_service() {
        assert!(is_service("@Injectable({ providedIn: 'root' })"));
        assert!(!is_service("@Component({})"));
    }

    #[test]
    fn test_extract_decorator_property() {
        assert_eq!(
            extract_decorator_property("@Component({ selector: 'app-root' })", "selector"),
            Some("app-root".to_string())
        );
    }

    #[test]
    fn test_count_array_items() {
        assert_eq!(
            count_array_items("@NgModule({ declarations: [A, B, C] })", "declarations"),
            Some(3)
        );
        assert_eq!(
            count_array_items("@NgModule({ declarations: [] })", "declarations"),
            Some(0)
        );
    }
}
