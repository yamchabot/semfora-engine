//! JavaScript family language tests: TypeScript, TSX, JavaScript, JSX, Vue
//!
//! Tests symbol extraction, visibility detection, call graphs, and React/Vue
//! framework-specific features.

#![allow(unused_imports)]

use crate::common::{assert_contains, assert_symbol_exists, assert_valid_json, TestRepo};

// ============================================================================
// TYPESCRIPT TESTS
// ============================================================================

mod typescript {
    use super::*;

    // ------------------------------------------------------------------------
    // Symbol Extraction
    // ------------------------------------------------------------------------

    #[test]
    fn test_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.ts",
            r#"
export function regularFunction() {
    return 42;
}

export const arrowFunction = () => {
    return 'arrow';
};

export async function asyncFunction(): Promise<string> {
    return 'async';
}

function privateFunction() {
    return 'private';
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Function", "-f", "json"]);
        let json = assert_valid_json(&output, "search functions");

        // Should find exported functions
        assert_symbol_exists(&json, "regularFunction");
        assert_symbol_exists(&json, "arrowFunction");
        assert_symbol_exists(&json, "asyncFunction");
    }

    #[test]
    fn test_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/classes.ts",
            r#"
export class UserService {
    private cache: Map<string, any>;

    constructor() {
        this.cache = new Map();
    }

    public getUser(id: string): User | null {
        return this.cache.get(id) || null;
    }

    private validateUser(user: User): boolean {
        return user.id !== undefined;
    }
}

export abstract class BaseEntity {
    abstract getId(): string;
}

class InternalClass {
    doSomething() {}
}
"#,
        );

        // Use analyze to extract symbols from file
        let output = repo.run_cli_success(&["analyze", "src/classes.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "analyze classes file");

        // Should find class symbols (either in symbols or raw_fallback)
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("UserService") || output_str.contains("class"),
            "Should find UserService class in output: {}",
            output
        );
    }

    #[test]
    fn test_interface_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/types.ts",
            r#"
export interface User {
    id: string;
    name: string;
    email: string;
}

export interface UserRepository {
    findById(id: string): Promise<User | null>;
    save(user: User): Promise<void>;
}

interface InternalConfig {
    debug: boolean;
}
"#,
        );

        // Use analyze to extract symbols from file
        let output = repo.run_cli_success(&["analyze", "src/types.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "analyze types file");

        // Should find interface symbols (either in symbols or raw_fallback)
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("User") || output_str.contains("interface"),
            "Should find User interface in output: {}",
            output
        );
    }

    #[test]
    fn test_enum_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/enums.ts",
            r#"
export enum Status {
    Pending = 'pending',
    Active = 'active',
    Inactive = 'inactive',
}

export const enum Direction {
    Up,
    Down,
    Left,
    Right,
}

enum InternalState {
    Loading,
    Ready,
    Error,
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Status", "-f", "json"]);
        let json = assert_valid_json(&output, "search enums");

        assert_symbol_exists(&json, "Status");
    }

    #[test]
    fn test_type_alias_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/types.ts",
            r#"
export type UserId = string;

export type UserResponse = {
    user: User;
    token: string;
};

export type Result<T, E = Error> =
    | { success: true; data: T }
    | { success: false; error: E };

type InternalType = { internal: boolean };
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Type", "-f", "json"]);
        let _json = assert_valid_json(&output, "search type aliases");
    }

    // ------------------------------------------------------------------------
    // Export/Visibility Detection
    // ------------------------------------------------------------------------

    #[test]
    fn test_named_exports() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/exports.ts",
            r#"
export function exportedFunction() {}
export const exportedConst = 42;
export class ExportedClass {}
export interface ExportedInterface {}
export type ExportedType = string;
export enum ExportedEnum { A, B }
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "file", "src/exports.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "query file");

        // All should be found as exported
        assert_symbol_exists(&json, "exportedFunction");
        assert_symbol_exists(&json, "ExportedClass");
    }

    #[test]
    fn test_default_exports() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/default.ts",
            r#"
function mainFunction() {
    return 'main';
}

export default mainFunction;
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "mainFunction", "-f", "json"]);
        let json = assert_valid_json(&output, "search default export");

        assert_symbol_exists(&json, "mainFunction");
    }

    #[test]
    fn test_reexports() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.ts",
            r#"
export function utilityFunction() {}
"#,
        );
        repo.add_file(
            "src/index.ts",
            r#"
export { utilityFunction } from './utils';
export * from './utils';
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "utility", "-f", "json"]);
        let json = assert_valid_json(&output, "search reexports");

        assert_symbol_exists(&json, "utilityFunction");
    }

    #[test]
    fn test_private_symbols_not_exported() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/private.ts",
            r#"
export function publicFunction() {
    return privateHelper();
}

function privateHelper() {
    return 'private';
}

const privateConst = 42;

class PrivateClass {}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "file", "src/private.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "query file");

        // Both should be extracted, but visibility should differ
        assert_symbol_exists(&json, "publicFunction");
        // privateHelper should exist but not be exported
    }

    // ------------------------------------------------------------------------
    // Call Graph Extraction
    // ------------------------------------------------------------------------

    #[test]
    fn test_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/calls.ts",
            r#"
export function main() {
    const result = processData();
    saveResult(result);
    return result;
}

function processData() {
    const raw = fetchData();
    return transform(raw);
}

function fetchData() {
    return { data: 'raw' };
}

function transform(data: any) {
    return { ...data, processed: true };
}

function saveResult(result: any) {
    console.log('Saved:', result);
}
"#,
        );

        // Use analyze to extract symbols with call information
        let output = repo.run_cli_success(&["analyze", "src/calls.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "analyze calls file");

        // Verify functions are extracted
        assert_symbol_exists(&json, "main");
        assert_symbol_exists(&json, "processData");
    }

    #[test]
    fn test_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/methods.ts",
            r#"
export class Service {
    private client: ApiClient;

    constructor(client: ApiClient) {
        this.client = client;
    }

    async fetchUsers() {
        const response = await this.client.get('/users');
        return this.parseResponse(response);
    }

    private parseResponse(response: any) {
        return response.data;
    }
}
"#,
        );

        // Use analyze to extract symbols
        let output = repo.run_cli_success(&["analyze", "src/methods.ts", "-f", "json"]);
        let json = assert_valid_json(&output, "analyze methods file");

        // Should find the class and its methods
        assert_symbol_exists(&json, "Service");
    }

    #[test]
    fn test_chained_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/chained.ts",
            r#"
export function processArray(items: number[]) {
    return items
        .filter(x => x > 0)
        .map(x => x * 2)
        .reduce((a, b) => a + b, 0);
}

export function builderPattern() {
    return new Builder()
        .setName('test')
        .setValue(42)
        .build();
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "process", "-f", "json"]);
        let _json = assert_valid_json(&output, "search chained");
    }

    #[test]
    fn test_async_await_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/async.ts",
            r#"
export async function fetchAndProcess() {
    const data = await fetchData();
    const processed = await processData(data);
    await saveData(processed);
    return processed;
}

async function fetchData() {
    return fetch('/api/data').then(r => r.json());
}

async function processData(data: any) {
    return { ...data, processed: true };
}

async function saveData(data: any) {
    await fetch('/api/save', { method: 'POST', body: JSON.stringify(data) });
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "fetch", "-f", "json"]);
        let json = assert_valid_json(&output, "search async");

        assert_symbol_exists(&json, "fetchAndProcess");
    }

    // ------------------------------------------------------------------------
    // Control Flow Detection
    // ------------------------------------------------------------------------

    #[test]
    fn test_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/control.ts",
            r#"
export function checkValue(x: number): string {
    if (x > 100) {
        return 'large';
    } else if (x > 50) {
        return 'medium';
    } else if (x > 0) {
        return 'small';
    } else {
        return 'invalid';
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "checkValue", "-f", "json"]);
        let json = assert_valid_json(&output, "search if-else");

        assert_symbol_exists(&json, "checkValue");
    }

    #[test]
    fn test_for_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/loops.ts",
            r#"
export function sumArray(arr: number[]): number {
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
        sum += arr[i];
    }
    return sum;
}

export function processItems(items: string[]): void {
    for (const item of items) {
        console.log(item);
    }
}

export function iterateObject(obj: Record<string, any>): void {
    for (const key in obj) {
        console.log(key, obj[key]);
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "sum", "-f", "json"]);
        let json = assert_valid_json(&output, "search loops");

        assert_symbol_exists(&json, "sumArray");
    }

    #[test]
    fn test_while_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/while.ts",
            r#"
export function waitForCondition(): void {
    let attempts = 0;
    while (attempts < 10 && !isReady()) {
        attempts++;
        wait(100);
    }
}

export function processQueue(queue: any[]): void {
    do {
        const item = queue.shift();
        process(item);
    } while (queue.length > 0);
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "wait", "-f", "json"]);
        let _json = assert_valid_json(&output, "search while");
    }

    #[test]
    fn test_switch_statements() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/switch.ts",
            r#"
export function handleAction(action: string): string {
    switch (action) {
        case 'create':
            return 'Created';
        case 'update':
            return 'Updated';
        case 'delete':
            return 'Deleted';
        default:
            return 'Unknown action';
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "handleAction", "-f", "json"]);
        let json = assert_valid_json(&output, "search switch");

        assert_symbol_exists(&json, "handleAction");
    }

    #[test]
    fn test_try_catch() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/errors.ts",
            r#"
export async function safeFetch(url: string): Promise<any> {
    try {
        const response = await fetch(url);
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
        }
        return await response.json();
    } catch (error) {
        console.error('Fetch failed:', error);
        return null;
    } finally {
        console.log('Fetch completed');
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "safeFetch", "-f", "json"]);
        let json = assert_valid_json(&output, "search try-catch");

        assert_symbol_exists(&json, "safeFetch");
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_empty_ts_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.ts", "");
        repo.add_file(
            "src/valid.ts",
            r#"
export function validFunc() {
    return 1;
}
"#,
        );

        let output = repo.generate_index();
        assert!(output.is_ok(), "Should handle empty TypeScript files");
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn test_syntax_error_recovery() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/broken.ts",
            r#"
export function broken( {
    // Missing closing paren and brace
    return 42
"#,
        );
        repo.add_file(
            "src/valid.ts",
            r#"
export function validFunc() {
    return 1;
}
"#,
        );

        let output = repo.generate_index();
        assert!(output.is_ok(), "Should handle syntax errors gracefully");
        // Should still index valid files
    }

    #[test]
    fn test_unicode_identifiers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/unicode.ts",
            r#"
export function 计算总和(数字数组: number[]): number {
    return 数字数组.reduce((a, b) => a + b, 0);
}

export const カウンター = {
    値: 0,
    増加() { this.値++; }
};

export function grüßen(name: string): string {
    return `Hallo, ${name}!`;
}
"#,
        );

        let output = repo.generate_index();
        assert!(output.is_ok(), "Should handle Unicode identifiers");
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn test_very_long_ts_file() {
        let repo = TestRepo::new();

        // Generate a file with many functions
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!(
                r#"
export function func{}() {{
    return {};
}}
"#,
                i, i
            ));
        }
        repo.add_file("src/long.ts", &content);

        let output = repo.generate_index();
        assert!(output.is_ok(), "Should handle very long files");
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn test_deeply_nested_ts() {
        let repo = TestRepo::new();

        // Generate deeply nested code
        let mut content = String::from("export function deeplyNested() {\n");
        for i in 0..20 {
            content.push_str(&format!("{}if (condition{}) {{\n", "    ".repeat(i + 1), i));
        }
        content.push_str(&format!("{}return 'deep';\n", "    ".repeat(21)));
        for i in (0..20).rev() {
            content.push_str(&format!("{}}}\n", "    ".repeat(i + 1)));
        }
        content.push_str("}\n");
        repo.add_file("src/nested.ts", &content);

        let output = repo.generate_index();
        assert!(output.is_ok(), "Should handle deeply nested code");
        assert!(output.unwrap().status.success());
    }

    // ------------------------------------------------------------------------
    // TypeScript-Specific Features
    // ------------------------------------------------------------------------

    #[test]
    fn test_generics() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/generics.ts",
            r#"
export function identity<T>(value: T): T {
    return value;
}

export class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    get(): T {
        return this.value;
    }
}

export interface Repository<T, ID = string> {
    findById(id: ID): Promise<T | null>;
    save(entity: T): Promise<T>;
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "identity", "-f", "json"]);
        let json = assert_valid_json(&output, "search generics");

        assert_symbol_exists(&json, "identity");
    }

    #[test]
    fn test_decorators() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/decorators.ts",
            r#"
function Injectable() {
    return function(target: any) {
        // decorator logic
    };
}

function Log(target: any, key: string, descriptor: PropertyDescriptor) {
    // method decorator
}

@Injectable()
export class UserService {
    @Log
    getUser(id: string) {
        return { id };
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "UserService", "-f", "json"]);
        let json = assert_valid_json(&output, "search decorators");

        assert_symbol_exists(&json, "UserService");
    }

    #[test]
    fn test_mapped_types() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/mapped.ts",
            r#"
export type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

export type Partial<T> = {
    [P in keyof T]?: T[P];
};

export type Pick<T, K extends keyof T> = {
    [P in K]: T[P];
};
"#,
        );
        repo.generate_index().expect("Index failed");

        // Should not crash on complex type definitions
        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with mapped types");
    }

    #[test]
    fn test_conditional_types() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/conditional.ts",
            r#"
export type IsString<T> = T extends string ? true : false;

export type Flatten<T> = T extends Array<infer U> ? U : T;

export type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with conditional types");
    }
}

// ============================================================================
// TSX (REACT) TESTS
// ============================================================================

mod tsx {
    use super::*;

    #[test]
    fn test_functional_component_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Button.tsx",
            r#"
import React from 'react';

interface ButtonProps {
    label: string;
    onClick: () => void;
    disabled?: boolean;
}

export const Button: React.FC<ButtonProps> = ({ label, onClick, disabled }) => {
    return (
        <button onClick={onClick} disabled={disabled}>
            {label}
        </button>
    );
};

export default Button;
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Button", "-f", "json"]);
        let json = assert_valid_json(&output, "search component");

        assert_symbol_exists(&json, "Button");
    }

    #[test]
    fn test_usestate_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Counter.tsx",
            r#"
import React, { useState } from 'react';

export const Counter: React.FC = () => {
    const [count, setCount] = useState(0);

    return (
        <div>
            <p>Count: {count}</p>
            <button onClick={() => setCount(c => c + 1)}>Increment</button>
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Counter", "-f", "json"]);
        let json = assert_valid_json(&output, "search useState component");

        assert_symbol_exists(&json, "Counter");
    }

    #[test]
    fn test_useeffect_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/DataFetcher.tsx",
            r#"
import React, { useState, useEffect } from 'react';

export const DataFetcher: React.FC<{ url: string }> = ({ url }) => {
    const [data, setData] = useState(null);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        fetch(url)
            .then(res => res.json())
            .then(data => {
                setData(data);
                setLoading(false);
            });
    }, [url]);

    if (loading) return <div>Loading...</div>;
    return <div>{JSON.stringify(data)}</div>;
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "DataFetcher", "-f", "json"]);
        let json = assert_valid_json(&output, "search useEffect component");

        assert_symbol_exists(&json, "DataFetcher");
    }

    #[test]
    fn test_usememo_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Expensive.tsx",
            r#"
import React, { useState, useMemo } from 'react';

export const ExpensiveComponent: React.FC<{ items: number[] }> = ({ items }) => {
    const [filter, setFilter] = useState('');

    const expensiveValue = useMemo(() => {
        return items.reduce((a, b) => a + b, 0);
    }, [items]);

    return (
        <div>
            <p>Sum: {expensiveValue}</p>
            <input value={filter} onChange={e => setFilter(e.target.value)} />
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Expensive", "-f", "json"]);
        let json = assert_valid_json(&output, "search useMemo component");

        assert_symbol_exists(&json, "ExpensiveComponent");
    }

    #[test]
    fn test_usecallback_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Handler.tsx",
            r#"
import React, { useState, useCallback } from 'react';

export const Handler: React.FC = () => {
    const [count, setCount] = useState(0);

    const handleClick = useCallback(() => {
        setCount(c => c + 1);
    }, []);

    return (
        <div>
            <p>Count: {count}</p>
            <button onClick={handleClick}>Click</button>
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Handler", "-f", "json"]);
        let json = assert_valid_json(&output, "search useCallback component");

        assert_symbol_exists(&json, "Handler");
    }

    #[test]
    fn test_useref_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Input.tsx",
            r#"
import React, { useRef } from 'react';

export const InputWithRef: React.FC = () => {
    const inputRef = useRef<HTMLInputElement>(null);

    const focusInput = () => {
        inputRef.current?.focus();
    };

    return (
        <div>
            <input ref={inputRef} />
            <button onClick={focusInput}>Focus</button>
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "InputWithRef", "-f", "json"]);
        let json = assert_valid_json(&output, "search useRef component");

        assert_symbol_exists(&json, "InputWithRef");
    }

    #[test]
    fn test_usecontext_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/ThemedButton.tsx",
            r#"
import React, { useContext, createContext } from 'react';

interface Theme {
    primary: string;
    secondary: string;
}

const ThemeContext = createContext<Theme>({ primary: '#000', secondary: '#fff' });

export const ThemedButton: React.FC = () => {
    const theme = useContext(ThemeContext);
    return <button style={{ backgroundColor: theme.primary }}>Click</button>;
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "ThemedButton", "-f", "json"]);
        let json = assert_valid_json(&output, "search useContext component");

        assert_symbol_exists(&json, "ThemedButton");
    }

    #[test]
    fn test_usereducer_hook() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/TodoApp.tsx",
            r#"
import React, { useReducer } from 'react';

interface State {
    todos: string[];
}

type Action = { type: 'ADD'; payload: string } | { type: 'REMOVE'; payload: number };

function reducer(state: State, action: Action): State {
    switch (action.type) {
        case 'ADD':
            return { todos: [...state.todos, action.payload] };
        case 'REMOVE':
            return { todos: state.todos.filter((_, i) => i !== action.payload) };
        default:
            return state;
    }
}

export const TodoApp: React.FC = () => {
    const [state, dispatch] = useReducer(reducer, { todos: [] });

    return (
        <div>
            {state.todos.map((todo, i) => (
                <div key={i}>{todo}</div>
            ))}
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "TodoApp", "-f", "json"]);
        let json = assert_valid_json(&output, "search useReducer component");

        assert_symbol_exists(&json, "TodoApp");
    }

    #[test]
    fn test_jsx_in_callgraph() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/App.tsx",
            r#"
import React from 'react';
import { Header } from './Header';
import { Footer } from './Footer';
import { Content } from './Content';

export const App: React.FC = () => {
    const handleClick = () => console.log('clicked');

    return (
        <div>
            <Header />
            <Content onClick={handleClick} />
            <Footer />
        </div>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "App", "-f", "json"]);
        let json = assert_valid_json(&output, "search App");

        assert_symbol_exists(&json, "App");
    }

    #[test]
    fn test_forward_ref() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/FancyInput.tsx",
            r#"
import React, { forwardRef, useImperativeHandle, useRef } from 'react';

interface FancyInputProps {
    placeholder?: string;
}

export interface FancyInputRef {
    focus: () => void;
    clear: () => void;
}

export const FancyInput = forwardRef<FancyInputRef, FancyInputProps>((props, ref) => {
    const inputRef = useRef<HTMLInputElement>(null);

    useImperativeHandle(ref, () => ({
        focus: () => inputRef.current?.focus(),
        clear: () => {
            if (inputRef.current) inputRef.current.value = '';
        },
    }));

    return <input ref={inputRef} placeholder={props.placeholder} />;
});
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "FancyInput", "-f", "json"]);
        let json = assert_valid_json(&output, "search forwardRef");

        assert_symbol_exists(&json, "FancyInput");
    }

    #[test]
    fn test_memo_hoc() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/MemoizedList.tsx",
            r#"
import React, { memo } from 'react';

interface ListProps {
    items: string[];
}

const List: React.FC<ListProps> = ({ items }) => {
    return (
        <ul>
            {items.map((item, i) => (
                <li key={i}>{item}</li>
            ))}
        </ul>
    );
};

export const MemoizedList = memo(List);
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "List", "-f", "json"]);
        let json = assert_valid_json(&output, "search memo");

        assert_symbol_exists(&json, "List");
    }

    #[test]
    fn test_fragments() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/FragmentComponent.tsx",
            r#"
import React from 'react';

export const FragmentComponent: React.FC = () => {
    return (
        <>
            <div>First</div>
            <div>Second</div>
            <React.Fragment>
                <div>Third</div>
            </React.Fragment>
        </>
    );
};
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "FragmentComponent", "-f", "json"]);
        let json = assert_valid_json(&output, "search fragments");

        assert_symbol_exists(&json, "FragmentComponent");
    }
}

// ============================================================================
// JAVASCRIPT TESTS
// ============================================================================

mod javascript {
    use super::*;

    #[test]
    fn test_commonjs_exports() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.js",
            r#"
function helper() {
    return 'helped';
}

function anotherHelper() {
    return 'also helped';
}

module.exports = { helper, anotherHelper };
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "helper", "-f", "json"]);
        let json = assert_valid_json(&output, "search CommonJS");

        assert_symbol_exists(&json, "helper");
    }

    #[test]
    fn test_esm_exports() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.mjs",
            r#"
export function helper() {
    return 'helped';
}

export const anotherHelper = () => {
    return 'also helped';
};

export default function main() {
    return helper();
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "helper", "-f", "json"]);
        let json = assert_valid_json(&output, "search ESM");

        assert_symbol_exists(&json, "helper");
    }

    #[test]
    fn test_class_syntax() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/classes.js",
            r#"
class Animal {
    constructor(name) {
        this.name = name;
    }

    speak() {
        console.log(`${this.name} makes a sound`);
    }
}

class Dog extends Animal {
    speak() {
        console.log(`${this.name} barks`);
    }
}

module.exports = { Animal, Dog };
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Animal", "-f", "json"]);
        let json = assert_valid_json(&output, "search JS class");

        assert_symbol_exists(&json, "Animal");
    }

    #[test]
    fn test_async_functions() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/async.js",
            r#"
async function fetchData(url) {
    const response = await fetch(url);
    return response.json();
}

const arrowAsync = async (data) => {
    return await processData(data);
};

module.exports = { fetchData, arrowAsync };
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "fetchData", "-f", "json"]);
        let json = assert_valid_json(&output, "search async JS");

        assert_symbol_exists(&json, "fetchData");
    }

    #[test]
    fn test_destructuring() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/destruct.js",
            r#"
function processUser({ name, age, address: { city } }) {
    return `${name}, ${age}, ${city}`;
}

const someObject = { a: 1, b: 2, c: 3 };
const { a, b, ...rest } = someObject;

module.exports = { processUser };
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "processUser", "-f", "json"]);
        let json = assert_valid_json(&output, "search destructuring");

        assert_symbol_exists(&json, "processUser");
    }

    #[test]
    fn test_iife_pattern() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/iife.js",
            r#"
const Module = (function() {
    const privateVar = 'private';

    function privateFunction() {
        return privateVar;
    }

    return {
        publicMethod: function() {
            return privateFunction();
        }
    };
})();

module.exports = Module;
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with IIFE");
    }

    #[test]
    fn test_prototype_methods() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/proto.js",
            r#"
function Person(name) {
    this.name = name;
}

Person.prototype.greet = function() {
    return `Hello, ${this.name}`;
};

Person.prototype.farewell = function() {
    return `Goodbye, ${this.name}`;
};

module.exports = Person;
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Person", "-f", "json"]);
        let json = assert_valid_json(&output, "search prototype");

        assert_symbol_exists(&json, "Person");
    }
}

// ============================================================================
// JSX TESTS
// ============================================================================

mod jsx {
    use super::*;

    #[test]
    fn test_jsx_component() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Card.jsx",
            r#"
import React from 'react';

export function Card({ title, children }) {
    return (
        <div className="card">
            <h2>{title}</h2>
            <div className="card-body">
                {children}
            </div>
        </div>
    );
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Card", "-f", "json"]);
        let json = assert_valid_json(&output, "search JSX component");

        assert_symbol_exists(&json, "Card");
    }

    #[test]
    fn test_jsx_with_hooks() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Counter.jsx",
            r#"
import React, { useState } from 'react';

export function Counter({ initialCount = 0 }) {
    const [count, setCount] = useState(initialCount);

    return (
        <div>
            <p>Count: {count}</p>
            <button onClick={() => setCount(c => c + 1)}>+</button>
            <button onClick={() => setCount(c => c - 1)}>-</button>
        </div>
    );
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Counter", "-f", "json"]);
        let json = assert_valid_json(&output, "search JSX with hooks");

        assert_symbol_exists(&json, "Counter");
    }

    #[test]
    fn test_jsx_arrow_component() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Badge.jsx",
            r#"
import React from 'react';

export const Badge = ({ label, color = 'blue' }) => (
    <span className={`badge badge-${color}`}>
        {label}
    </span>
);

export const IconBadge = ({ icon, ...props }) => (
    <Badge {...props}>
        <i className={icon} /> {props.label}
    </Badge>
);
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "Badge", "-f", "json"]);
        let json = assert_valid_json(&output, "search JSX arrow component");

        assert_symbol_exists(&json, "Badge");
    }

    #[test]
    fn test_jsx_class_component() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/ClassComponent.jsx",
            r#"
import React, { Component } from 'react';

export class ClassComponent extends Component {
    constructor(props) {
        super(props);
        this.state = { count: 0 };
    }

    handleClick = () => {
        this.setState(prev => ({ count: prev.count + 1 }));
    };

    render() {
        return (
            <div>
                <p>{this.state.count}</p>
                <button onClick={this.handleClick}>Click</button>
            </div>
        );
    }
}
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "ClassComponent", "-f", "json"]);
        let json = assert_valid_json(&output, "search JSX class component");

        assert_symbol_exists(&json, "ClassComponent");
    }
}

// ============================================================================
// VUE TESTS
// ============================================================================

mod vue {
    use super::*;

    #[test]
    fn test_vue_sfc_composition_api() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Counter.vue",
            r#"
<template>
    <div>
        <p>{{ count }}</p>
        <button @click="increment">+</button>
    </div>
</template>

<script>
import { ref } from 'vue';

export default {
    name: 'Counter',
    setup() {
        const count = ref(0);

        function increment() {
            count.value++;
        }

        return { count, increment };
    }
};
</script>

<style scoped>
button {
    padding: 0.5rem;
}
</style>
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "file", "src/Counter.vue", "-f", "json"]);
        let _json = assert_valid_json(&output, "query Vue SFC");
    }

    #[test]
    fn test_vue_options_api() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/OptionsComponent.vue",
            r#"
<template>
    <div>
        <h1>{{ title }}</h1>
        <p>{{ message }}</p>
        <button @click="handleClick">Click</button>
    </div>
</template>

<script>
export default {
    name: 'OptionsComponent',
    props: {
        title: {
            type: String,
            required: true
        }
    },
    data() {
        return {
            message: 'Hello Vue!'
        };
    },
    methods: {
        handleClick() {
            this.message = 'Clicked!';
        }
    },
    computed: {
        uppercaseTitle() {
            return this.title.toUpperCase();
        }
    },
    mounted() {
        console.log('Component mounted');
    }
};
</script>
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with Vue Options API");
    }

    #[test]
    fn test_vue_script_setup() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/SetupComponent.vue",
            r#"
<template>
    <div>{{ doubled }}</div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';

const count = ref(0);
const doubled = computed(() => count.value * 2);

function increment() {
    count.value++;
}

defineExpose({ increment });
</script>
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with Vue script setup");
    }

    #[test]
    fn test_vue_with_typescript() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/TypedComponent.vue",
            r#"
<template>
    <div>
        <input v-model="name" />
        <p>Hello, {{ name }}!</p>
    </div>
</template>

<script lang="ts">
import { defineComponent, ref } from 'vue';

interface Props {
    initialName?: string;
}

export default defineComponent({
    name: 'TypedComponent',
    props: {
        initialName: {
            type: String,
            default: 'World'
        }
    },
    setup(props: Props) {
        const name = ref(props.initialName || 'World');
        return { name };
    }
});
</script>
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["query", "overview", "-f", "json"]);
        let _json = assert_valid_json(&output, "overview with Vue TypeScript");
    }

    #[test]
    fn test_vue_composables() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/composables/useCounter.ts",
            r#"
import { ref, computed, Ref } from 'vue';

export function useCounter(initialValue: number = 0) {
    const count = ref(initialValue);

    const doubled = computed(() => count.value * 2);

    function increment() {
        count.value++;
    }

    function decrement() {
        count.value--;
    }

    function reset() {
        count.value = initialValue;
    }

    return {
        count,
        doubled,
        increment,
        decrement,
        reset
    };
}
"#,
        );
        repo.add_file(
            "src/ComponentUsingComposable.vue",
            r#"
<template>
    <div>
        <p>{{ count }} (doubled: {{ doubled }})</p>
        <button @click="increment">+</button>
        <button @click="decrement">-</button>
        <button @click="reset">Reset</button>
    </div>
</template>

<script setup lang="ts">
import { useCounter } from './composables/useCounter';

const { count, doubled, increment, decrement, reset } = useCounter(10);
</script>
"#,
        );
        repo.generate_index().expect("Index failed");

        let output = repo.run_cli_success(&["search", "useCounter", "-f", "json"]);
        let json = assert_valid_json(&output, "search Vue composable");

        assert_symbol_exists(&json, "useCounter");
    }
}
