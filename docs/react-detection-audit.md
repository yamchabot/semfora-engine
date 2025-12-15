# React.js Detection Code Quality Audit

**Date**: 2025-12-15
**Scope**: `src/detectors/javascript/frameworks/react.rs`, `src/duplicate/boilerplate/javascript.rs`
**Status**: Draft - Pending Review

---

## Executive Summary

The React detection code spans **3 main locations** with **~600 lines** of production code and **~500 lines** of tests. Overall quality is **good** with some areas for improvement.

| Metric | Status | Details |
|--------|--------|---------|
| **Integration** | Verified | `react::enhance()` properly called from main pipeline |
| **Test Coverage** | Good | 33 tests passing, covers major hook types |
| **Code Splitting** | Good | Separate module allows conditional execution |
| **Complexity** | Moderate | 4 functions with high cognitive complexity |
| **Duplication** | Issue | Hook detection logic duplicated in 5+ places |

---

## Architectural Decision: Keep React Logic Separate

**Decision**: Maintain React-specific logic in `react.rs` rather than merging into core JS extraction.

**Rationale**:
- Many JS projects don't use React (Node.js backends, Vue, Angular, vanilla JS)
- Current architecture: `detect_frameworks()` -> conditionally run `react::enhance()`
- Merging would add unnecessary overhead to every non-React file
- Framework-specific modules allow targeted optimization

**Current Flow**:
```
JavaScript file detected
    -> Core extraction (always runs)
    -> detect_frameworks() checks imports/patterns
    -> If React detected: react::enhance() runs
    -> If Next.js detected: nextjs::enhance() runs
    -> etc.
```

---

## Step 1: Verify Integration Pipeline

### Status: VERIFIED

The `react::enhance()` function IS properly integrated into the extraction pipeline.

**Note**: The initial call graph analysis showed 0 callers because it tracks symbol hashes,
and the call uses the module-qualified path `frameworks::react::enhance`. Raw source search
confirmed the integration.

### Integration Points (2 call sites)

**1. Main JS/TS Extraction** (`src/detectors/javascript/mod.rs:69`):
```rust
pub fn extract(summary: &mut SemanticSummary, source: &str, tree: &Tree, lang: Lang) -> Result<()> {
    let root = tree.root_node();

    // Phase 1: Core JavaScript/TypeScript extraction
    core::extract_core(summary, &root, source, lang)?;

    // Phase 2: Detect frameworks from imports and patterns
    let frameworks = detect_frameworks(summary, source);

    // Phase 3: Apply framework-specific enhancements
    if frameworks.is_react {
        frameworks::react::enhance(summary, &root, source);  // <-- CALLED HERE
    }
    // ... other frameworks
}
```

**2. Vue SFC Extraction** (`src/detectors/javascript/mod.rs:150`):
```rust
// Edge case: React detected in Vue SFC (e.g., JSX in Vue)
if frameworks.is_react {
    frameworks::react::enhance(summary, &root, &sfc_script.content);
}
```

### Remaining Tasks

- [x] ~~Trace the JavaScript extraction flow from entry point~~ (Verified)
- [x] ~~Verify `react::enhance()` is called when `FrameworkContext.is_react == true`~~ (Verified)
- [x] ~~Check if the framework enhancement happens~~ (In `mod.rs`, not `core.rs`)
- [ ] Add integration test to verify React enhancement runs end-to-end

---

## Step 2: Centralize Hook Detection Logic

### Problem

Hook detection logic (`name.starts_with("use")` with uppercase check) is duplicated in 5+ locations:

| Location | Pattern Used |
|----------|--------------|
| `schema.rs:898-905` | `Call::check_is_hook()` - canonical implementation |
| `toon.rs:92` | Inline duplicate |
| `javascript.rs:178` | Partial check (missing uppercase) |
| `javascript.rs:185` | Filter with partial check |
| `mod.rs:123` | `dep.starts_with("use") && dep.len() > 3` |
| `react.rs:582-604` | Manual char-by-char parsing |

### Solution

Use `Call::check_is_hook()` from `schema.rs` everywhere:

```rust
// schema.rs - already exists
impl Call {
    pub fn check_is_hook(name: &str) -> bool {
        name.starts_with("use")
            && name.chars().nth(3).map(|c| c.is_uppercase()).unwrap_or(false)
    }
}
```

### Tasks

- [ ] Replace inline hook check in `toon.rs:92` with `Call::check_is_hook()`
- [ ] Replace check in `javascript.rs:178` with `Call::check_is_hook()`
- [ ] Replace filter in `javascript.rs:185` with `Call::check_is_hook()`
- [ ] Replace check in `mod.rs:123` with `Call::check_is_hook()`
- [ ] Refactor `count_custom_hooks` in `react.rs` to use `Call::check_is_hook()`
- [ ] Add re-export in prelude or common module for easier access

### Estimated Impact

- ~30 lines of duplicated logic removed
- Single source of truth for hook detection
- Easier to update if React conventions change

---

## Step 3: Add Missing Test Coverage

### Current Coverage Gaps

| Function | Lines | Complexity | Tests |
|----------|-------|------------|-------|
| `extract_jsx_insertions` | 418-479 | CC=22 | 0 |
| `enhance` | 17-23 | Entry point | 0 |
| `count_custom_hooks` | 582-604 | CC=21 | 1 |
| `extract_hook_state` | 50-79 | CC=21 | Indirect |

### Tests to Add

```rust
// react.rs tests module

#[test]
fn test_extract_jsx_insertions_conditional_render() {
    let source = r#"
        function App() {
            return (
                <div>
                    {isLoggedIn && <UserProfile />}
                    {isAdmin ? <AdminPanel /> : <UserPanel />}
                </div>
            );
        }
    "#;
    // Assert conditional render detected
    // Assert component calls added to summary
}

#[test]
fn test_extract_jsx_insertions_list_pattern() {
    let source = r#"
        function List({ items }) {
            return (
                <ul>
                    {items.map(item => <ListItem key={item.id} />)}
                </ul>
            );
        }
    "#;
    // Assert list pattern detected
}

#[test]
fn test_enhance_full_component() {
    let source = r#"
        import { useState, useEffect, useMemo } from 'react';

        function Counter() {
            const [count, setCount] = useState(0);
            const doubled = useMemo(() => count * 2, [count]);

            useEffect(() => {
                document.title = `Count: ${count}`;
            }, [count]);

            return <button onClick={() => setCount(c => c + 1)}>{doubled}</button>;
        }
    "#;
    // Assert all hooks extracted
    // Assert state changes recorded
    // Assert insertions include hook descriptions
}

#[test]
fn test_nested_hooks_in_callback() {
    // Edge case: hooks should not be inside callbacks
    // But we should handle gracefully if parsed
}
```

### Tasks

- [ ] Add test for `extract_jsx_insertions` with conditional rendering
- [ ] Add test for `extract_jsx_insertions` with list/map patterns
- [ ] Add integration test for `enhance()` entry point
- [ ] Add edge case tests for TypeScript generics in hooks
- [ ] Add test for nested component detection

---

## Step 4: Refactor High-Complexity Functions

### Target Functions

#### 4.1 `count_custom_hooks` (CC=21, Nesting=5)

**Current Implementation** (`react.rs:582-604`):
```rust
pub fn count_custom_hooks(source: &str) -> usize {
    let mut count = 0;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'u' {
            if let Some('s') = chars.peek().copied() {
                chars.next();
                if let Some('e') = chars.peek().copied() {
                    chars.next();
                    if let Some(next) = chars.peek() {
                        if next.is_uppercase() {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}
```

**Proposed Refactor**:
```rust
use regex::Regex;
use once_cell::sync::Lazy;

static HOOK_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\buse[A-Z][a-zA-Z]*").unwrap()
});

pub fn count_custom_hooks(source: &str) -> usize {
    HOOK_PATTERN.find_iter(source).count()
}
```

**Alternative (no regex)**:
```rust
pub fn count_custom_hooks(source: &str) -> usize {
    source
        .split_whitespace()
        .filter(|word| Call::check_is_hook(word))
        .count()
}
```

#### 4.2 `extract_hook_state` (CC=21, Nesting=5)

**Issue**: Deep nesting from chained `if let` statements.

**Proposed Refactor**: Extract helper functions:

```rust
fn extract_hook_state(summary: &mut SemanticSummary, node: &Node, func_name: &str, source: &str) {
    let Some(declarator) = find_parent_declarator(node) else { return };
    let Some(array_pattern) = get_array_pattern(&declarator) else { return };

    if let Some(state_name) = extract_first_identifier(&array_pattern, source) {
        let init = extract_hook_initializer(node, source);
        record_state_change(summary, &state_name, &init, func_name);
    }
}

fn find_parent_declarator(node: &Node) -> Option<Node> {
    node.parent().filter(|p| p.kind() == "variable_declarator")
}

fn get_array_pattern(declarator: &Node) -> Option<Node> {
    declarator
        .child_by_field_name("name")
        .filter(|n| n.kind() == "array_pattern")
}
```

### Tasks

- [ ] Refactor `count_custom_hooks` to use regex or `Call::check_is_hook()`
- [ ] Extract helper functions from `extract_hook_state`
- [ ] Consider extracting helpers from `extract_jsx_insertions`
- [ ] Add benchmarks to ensure refactors don't regress performance

---

## Step 5: Reduce Test Boilerplate

### Problem

94 test functions in `javascript.rs` follow identical patterns:

```rust
#[test]
fn test_react_query_detection() {
    assert!(is_react_query(&make_symbol("useGetUsers", &["useQuery"])));
}

#[test]
fn test_react_query_with_mutation() {
    assert!(is_react_query(&make_symbol("useCreateUser", &["useMutation"])));
}
// ... 92 more similar tests
```

### Solution: Parameterized Tests with `rstest`

```rust
use rstest::rstest;

#[rstest]
#[case("useGetUsers", &["useQuery"], true)]
#[case("useCreateUser", &["useMutation"], true)]
#[case("useData", &["useQuery", "fetch", "process", "transform"], false)] // too many calls
#[case("getData", &["useQuery"], false)] // wrong name
fn test_is_react_query(
    #[case] name: &str,
    #[case] calls: &[&str],
    #[case] expected: bool,
) {
    assert_eq!(is_react_query(&make_symbol(name, calls)), expected);
}
```

### Tasks

- [ ] Add `rstest` to dev-dependencies
- [ ] Convert `is_react_query` tests to parameterized
- [ ] Convert `is_react_hook_wrapper` tests to parameterized
- [ ] Convert `is_react_wrapper` tests to parameterized
- [ ] Estimate: ~300 lines of test code reduced to ~50

---

## Step 6: Add Missing React 18/19 Hooks

### Currently Detected

| Hook | Semantic | Boilerplate |
|------|----------|-------------|
| useState | Yes | Yes |
| useEffect | Yes | Yes |
| useLayoutEffect | Yes | Yes |
| useMemo | Yes | Yes |
| useCallback | Yes | Yes |
| useRef | Yes | Yes |
| useContext | Partial | Yes |
| useReducer | Yes | Yes |

### Missing Hooks to Add

| Hook | React Version | Priority |
|------|---------------|----------|
| useId | 18 | Medium |
| useSyncExternalStore | 18 | Low |
| useInsertionEffect | 18 | Low |
| useTransition | 18 | Medium |
| useDeferredValue | 18 | Medium |
| use | 19 | High |
| useOptimistic | 19 | Medium |
| useFormStatus | 19 | Medium |
| useActionState | 19 | Medium |

### Tasks

- [ ] Add `useId` extraction (simple - just track usage)
- [ ] Add `useTransition` extraction (returns [isPending, startTransition])
- [ ] Add `useDeferredValue` extraction
- [ ] Add `use` hook detection (React 19 - reads promises/context)
- [ ] Add React 19 form hooks when stable
- [ ] Update boilerplate detection for new hooks

---

## Step 7: Documentation Updates

### Tasks

- [ ] Add JSDoc-style comments to public functions in `react.rs`
- [ ] Document the extraction patterns in module-level docs
- [ ] Add examples of expected input/output for each extractor
- [ ] Update CHANGELOG.md with React detection improvements

---

## Summary: Implementation Order

| Step | Priority | Effort | Impact |
|------|----------|--------|--------|
| 1. Verify Integration | Critical | Low | High |
| 2. Centralize Hook Detection | High | Medium | High |
| 3. Add Test Coverage | High | Medium | Medium |
| 4. Refactor Complexity | Medium | Medium | Medium |
| 5. Reduce Test Boilerplate | Low | Low | Low |
| 6. Add React 18/19 Hooks | Medium | Medium | Medium |
| 7. Documentation | Low | Low | Low |

---

## Appendix: File Locations

```
src/detectors/javascript/frameworks/
├── mod.rs              # Framework detection, FrameworkContext
├── react.rs            # React-specific extraction (hooks, JSX)
├── nextjs.rs           # Next.js patterns
├── express.rs          # Express.js patterns
├── vue.rs              # Vue.js patterns
└── angular.rs          # Angular patterns

src/duplicate/boilerplate/
├── mod.rs              # Boilerplate flag struct and setters
├── javascript.rs       # JS/TS boilerplate patterns (React Query, hooks, etc.)
├── rust.rs             # Rust boilerplate patterns
└── csharp.rs           # C# boilerplate patterns

src/
├── schema.rs           # Call::check_is_hook() - canonical hook detection
└── toon.rs             # is_meaningful_call() - output filtering
```
