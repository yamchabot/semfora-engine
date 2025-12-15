# React Patterns Implementation Plan

## Overview

Add comprehensive React pattern detection for both semantic extraction (understanding what code does) and boilerplate filtering (duplicate detection).

---

## Phase 1: Semantic Extraction (react.rs)

Add extraction for React hooks that are currently missing from semantic analysis.

### 1.1 useEffect Extraction

**Goal**: Detect side effects and their dependencies.

**Patterns to detect**:
```jsx
useEffect(() => { fetchData(); }, [id]);        // "effect on [id]"
useEffect(() => { setup(); }, []);              // "effect on mount"
useEffect(() => { update(); });                 // "effect on every render"
useEffect(() => { return () => cleanup(); }, []);  // "effect with cleanup"
```

**Implementation**:
- Add `extract_effect_hooks()` function
- Visit call_expression nodes where function is "useEffect"
- Extract dependency array (2nd argument)
- Detect cleanup function (return statement in callback)
- Add to `summary.insertions`

**Insertion format**:
- `"effect on mount"` - empty deps `[]`
- `"effect on [dep1, dep2]"` - with deps
- `"effect on every render"` - no deps array
- Append `" with cleanup"` if cleanup detected

### 1.2 useMemo Extraction

**Goal**: Detect memoized computations for performance analysis.

**Patterns to detect**:
```jsx
const value = useMemo(() => expensiveCalc(a, b), [a, b]);
const filtered = useMemo(() => items.filter(predicate), [items]);
```

**Implementation**:
- Add `extract_memo_hooks()` function
- Visit call_expression nodes where function is "useMemo"
- Extract variable name from parent variable_declarator
- Extract dependency array
- Add to `summary.insertions`

**Insertion format**:
- `"memoized {varName} on [{deps}]"`

### 1.3 useCallback Extraction

**Goal**: Detect memoized callbacks for performance analysis.

**Patterns to detect**:
```jsx
const handleClick = useCallback(() => onClick(id), [id, onClick]);
const submit = useCallback(async () => { await api.post(); }, []);
```

**Implementation**:
- Add `extract_callback_hooks()` function
- Similar to useMemo but for callbacks
- Extract variable name and dependencies

**Insertion format**:
- `"memoized callback {varName} on [{deps}]"`

### 1.4 useRef Extraction

**Goal**: Detect refs for DOM manipulation and mutable values.

**Patterns to detect**:
```jsx
const inputRef = useRef(null);           // DOM ref
const timerRef = useRef<number>();       // Mutable value
const countRef = useRef(0);              // Mutable counter
```

**Implementation**:
- Add `extract_ref_hooks()` function
- Visit call_expression nodes where function is "useRef"
- Extract variable name from parent
- Infer purpose from initializer (null = DOM ref, value = mutable)

**Insertion format**:
- `"ref: {varName}"` for DOM refs (null initializer)
- `"mutable ref: {varName}"` for value refs

### 1.5 Update enhance() Function

Modify the main `enhance()` function to call all new extractors:

```rust
pub fn enhance(summary: &mut SemanticSummary, root: &Node, source: &str) {
    extract_state_hooks(summary, root, source);      // existing
    extract_effect_hooks(summary, root, source);     // NEW
    extract_memo_hooks(summary, root, source);       // NEW
    extract_callback_hooks(summary, root, source);   // NEW
    extract_ref_hooks(summary, root, source);        // NEW
    extract_jsx_insertions(summary, root, source);   // existing
}
```

---

## Phase 2: Boilerplate Patterns (javascript.rs)

Add new boilerplate categories for common React patterns that appear similar across codebases.

### 2.1 Add BoilerplateCategory Variants (mod.rs)

Add to the enum:
```rust
ContextProvider,      // Context.Provider wrapper components
SimpleContextHook,    // useContext one-liner hooks
HOCWrapper,           // withX higher-order components
LazyComponent,        // React.lazy imports
SuspenseBoundary,     // Suspense/ErrorBoundary wrappers
```

### 2.2 ContextProvider Pattern

**Goal**: Filter context provider components that wrap children.

**Detection criteria**:
- Name contains "Provider" (ThemeProvider, AuthProvider)
- Source contains ".Provider" JSX element
- Source contains "children" prop usage
- Minimal other logic (mostly just wrapping)

**Example matches**:
```jsx
export function ThemeProvider({ children }) {
  const [theme, setTheme] = useState('light');
  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}
```

### 2.3 SimpleContextHook Pattern

**Goal**: Filter trivial useContext wrapper hooks.

**Detection criteria**:
- Name starts with "use"
- Calls contain "useContext"
- Total calls <= 2 (useContext + maybe one other)
- No control flow (no if/for/while)
- Lines <= 5

**Example matches**:
```jsx
export const useTheme = () => useContext(ThemeContext);
export function useAuth() {
  return useContext(AuthContext);
}
```

### 2.4 HOCWrapper Pattern

**Goal**: Filter higher-order component wrappers.

**Detection criteria**:
- Name starts with "with" (withAuth, withRouter, withStyles)
- Returns a function/component
- Has minimal internal logic

**Example matches**:
```jsx
export const withAuth = (Component) => (props) => {
  const auth = useAuth();
  return <Component {...props} auth={auth} />;
};
```

### 2.5 LazyComponent Pattern

**Goal**: Filter React.lazy dynamic import wrappers.

**Detection criteria**:
- Calls contain "lazy"
- Source contains "import("
- Minimal other logic

**Example matches**:
```jsx
const Dashboard = lazy(() => import('./Dashboard'));
const Settings = React.lazy(() => import('./Settings'));
```

### 2.6 SuspenseBoundary Pattern

**Goal**: Filter Suspense/ErrorBoundary wrapper components.

**Detection criteria**:
- Returns JSX containing "Suspense" or "ErrorBoundary"
- Source contains "fallback" prop
- Has "children" usage
- Minimal other logic

**Example matches**:
```jsx
function AsyncBoundary({ children, fallback }) {
  return (
    <ErrorBoundary fallback={<Error />}>
      <Suspense fallback={fallback}>
        {children}
      </Suspense>
    </ErrorBoundary>
  );
}
```

### 2.7 Add Pattern Matchers

Add to PATTERNS array in javascript.rs:
```rust
PatternMatcher {
    category: BoilerplateCategory::ContextProvider,
    languages: &[Lang::JavaScript],
    detector: is_context_provider,
    enabled_by_default: true,
},
// ... etc for each pattern
```

---

## Phase 3: Documentation Updates

### 3.1 README.md

Update boilerplate coverage table:
- JavaScript/TypeScript: 14 → 19 patterns

Add to patterns list:
- ContextProvider, SimpleContextHook, HOCWrapper, LazyComponent, SuspenseBoundary

### 3.2 docs/architecture.md

Update boilerplate section with new patterns.

---

## Implementation Order

1. [x] **Phase 1.1**: Add useEffect extraction to react.rs
2. [x] **Phase 1.2**: Add useMemo extraction to react.rs
3. [x] **Phase 1.3**: Add useCallback extraction to react.rs
4. [x] **Phase 1.4**: Add useRef extraction to react.rs
5. [x] **Phase 1.5**: Update enhance() to call new extractors
6. [x] **Phase 1.6**: Add tests for new extractors
7. [x] **Phase 2.1**: Add BoilerplateCategory variants to mod.rs
8. [x] **Phase 2.2**: Implement is_context_provider()
9. [x] **Phase 2.3**: Implement is_simple_context_hook()
10. [x] **Phase 2.4**: Implement is_hoc_wrapper()
11. [x] **Phase 2.5**: Implement is_lazy_component()
12. [x] **Phase 2.6**: Implement is_suspense_boundary()
13. [x] **Phase 2.7**: Add pattern matchers to PATTERNS array
14. [x] **Phase 2.8**: Add tests for boilerplate patterns
15. [x] **Phase 3.1**: Update README.md
16. [x] **Phase 3.2**: Update docs/architecture.md
17. [x] **Phase 3.3**: Run full test suite
18. [x] **Phase 3.4**: Commit changes

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/detectors/javascript/frameworks/react.rs` | Add 4 new extraction functions + tests |
| `src/duplicate/boilerplate/mod.rs` | Add 5 new BoilerplateCategory variants |
| `src/duplicate/boilerplate/javascript.rs` | Add 5 detection functions + pattern matchers + tests |
| `README.md` | Update pattern counts and lists |
| `docs/architecture.md` | Update boilerplate section |

---

## Testing Strategy

1. **Unit tests**: Each extraction/detection function gets dedicated tests
2. **Integration tests**: Verify patterns work with real-world code snippets
3. **Cargo test**: Run full test suite before committing

---

## Notes

- All new boilerplate patterns use `Lang::JavaScript` which auto-extends to TS/JSX/TSX via `is_lang_compatible()`
- Extraction functions follow existing pattern of visiting AST nodes
- Keep insertions concise for token efficiency
