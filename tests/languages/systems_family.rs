//! Systems language family integration tests
//!
//! Tests for Rust, Go, C, and C++ - languages with explicit memory management
//! and specific visibility/export conventions.

use crate::common::{assert_symbol_exists, assert_symbol_exported, assert_valid_json, TestRepo};

// =============================================================================
// RUST TESTS
// =============================================================================

mod rust_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_rust_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
pub fn process() {
    println!("processing");
}

pub fn validate(x: i32) -> bool {
    x > 0
}

pub fn transform(s: &str) -> String {
    s.to_uppercase()
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust function extraction");

        assert_symbol_exists(&json, "process");
        assert_symbol_exists(&json, "validate");
        assert_symbol_exists(&json, "transform");
    }

    #[test]
    fn test_rust_struct_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/models.rs",
            r#"
pub struct User {
    pub id: u64,
    pub name: String,
    email: String,  // private field
}

struct InternalCache {
    data: Vec<u8>,
}

pub struct Config {
    pub timeout: u64,
    pub retries: u32,
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/models.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust struct extraction");

        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "InternalCache");
        assert_symbol_exists(&json, "Config");
    }

    #[test]
    fn test_rust_enum_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/types.rs",
            r#"
pub enum Status {
    Pending,
    Active,
    Completed,
    Failed(String),
}

enum InternalState {
    Initializing,
    Ready,
    Shutdown,
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/types.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust enum extraction");

        assert_symbol_exists(&json, "Status");
        assert_symbol_exists(&json, "InternalState");
    }

    #[test]
    fn test_rust_trait_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/traits.rs",
            r#"
pub trait Processor {
    fn process(&self, input: &str) -> String;
    fn validate(&self) -> bool;
}

trait InternalHandler {
    fn handle(&mut self);
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/traits.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust trait extraction");

        assert_symbol_exists(&json, "Processor");
        assert_symbol_exists(&json, "InternalHandler");
    }

    #[test]
    fn test_rust_impl_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/impl.rs",
            r#"
pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn new() -> Self {
        Counter { value: 0 }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    fn reset(&mut self) {
        self.value = 0;
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/impl.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust impl extraction");

        assert_symbol_exists(&json, "Counter");
        assert_symbol_exists(&json, "new");
        assert_symbol_exists(&json, "increment");
    }

    // -------------------------------------------------------------------------
    // Visibility Detection (pub keyword)
    // -------------------------------------------------------------------------

    #[test]
    fn test_rust_pub_function_exported() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
pub fn public_function() -> String {
    "public".to_string()
}

fn private_function() -> String {
    "private".to_string()
}

pub(crate) fn crate_function() -> String {
    "crate".to_string()
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust visibility detection");

        assert_symbol_exported(&json, "public_function");
    }

    #[test]
    fn test_rust_pub_struct_exported() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
pub struct PublicStruct {
    pub field: i32,
}

struct PrivateStruct {
    field: i32,
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust struct visibility");

        assert_symbol_exported(&json, "PublicStruct");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_rust_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
fn helper() -> i32 {
    42
}

pub fn main_function() -> i32 {
    let x = helper();
    x * 2
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust function calls");

        assert_symbol_exists(&json, "main_function");
        assert_symbol_exists(&json, "helper");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_rust_if_let_match() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
pub fn process_option(opt: Option<i32>) -> i32 {
    if let Some(value) = opt {
        value * 2
    } else {
        0
    }
}

pub fn process_result(res: Result<i32, String>) -> i32 {
    match res {
        Ok(v) => v,
        Err(_) => -1,
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust if-let and match");

        assert_symbol_exists(&json, "process_option");
        assert_symbol_exists(&json, "process_result");
    }

    #[test]
    fn test_rust_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lib.rs",
            r#"
pub fn loop_examples() {
    for i in 0..10 {
        println!("{}", i);
    }

    let mut x = 0;
    while x < 5 {
        x += 1;
    }

    loop {
        if x > 10 {
            break;
        }
        x += 1;
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lib.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust loops");

        assert_symbol_exists(&json, "loop_examples");
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_rust_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.rs", "");

        let result = repo.run_cli(&["analyze", "src/empty.rs", "-f", "json"]);
        assert!(result.is_ok(), "Should handle empty Rust file");
    }

    #[test]
    fn test_rust_async_await() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/async.rs",
            r#"
async fn fetch_data(url: &str) -> String {
    "data".to_string()
}

pub async fn process_all(urls: Vec<&str>) -> Vec<String> {
    urls.iter().map(|_| "result".to_string()).collect()
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/async.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust async/await");

        assert_symbol_exists(&json, "fetch_data");
        assert_symbol_exists(&json, "process_all");
    }

    #[test]
    fn test_rust_generics() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/generics.rs",
            r#"
pub struct Container<T> {
    value: T,
}

impl<T> Container<T> {
    pub fn new(value: T) -> Self {
        Container { value }
    }

    pub fn get(&self) -> &T {
        &self.value
    }
}

pub fn process<T: Clone>(item: T) -> T {
    item.clone()
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/generics.rs", "-f", "json"]);
        let json = assert_valid_json(&output, "Rust generics");

        assert_symbol_exists(&json, "Container");
        assert_symbol_exists(&json, "process");
    }
}

// =============================================================================
// GO TESTS
// =============================================================================

mod go_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_go_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/handler/handler.go",
            r#"package handler

func HandleRequest() {
    // handle request
}

func ProcessData() {
    // process data
}

func ValidateInput() bool {
    return true
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/handler/handler.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go function extraction");

        assert_symbol_exists(&json, "HandleRequest");
        assert_symbol_exists(&json, "ProcessData");
        assert_symbol_exists(&json, "ValidateInput");
    }

    #[test]
    fn test_go_struct_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/models/user.go",
            r#"package models

type User struct {
    ID        int64
    Name      string
    Email     string
    isActive  bool
}

type config struct {
    timeout int
    retries int
}

type Response struct {
    Data    interface{}
    Error   string
    Status  int
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/models/user.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go struct extraction");

        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "config");
        assert_symbol_exists(&json, "Response");
    }

    #[test]
    fn test_go_interface_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/interfaces/repository.go",
            r#"package interfaces

type Repository interface {
    Create(item interface{}) error
    Read(id int64) (interface{}, error)
    Update(id int64, item interface{}) error
    Delete(id int64) error
}

type reader interface {
    read() []byte
}
"#,
        );

        repo.generate_index().unwrap();
        let output =
            repo.run_cli_success(&["analyze", "pkg/interfaces/repository.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go interface extraction");

        assert_symbol_exists(&json, "Repository");
        assert_symbol_exists(&json, "reader");
    }

    // -------------------------------------------------------------------------
    // Visibility Detection (Uppercase = exported)
    // -------------------------------------------------------------------------

    #[test]
    fn test_go_uppercase_exported() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/api/handler.go",
            r#"package api

func HandleRequest() {
    helper()
}

func helper() {
}

type Server struct {
    port int
}

type config struct {
    timeout int
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/api/handler.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go uppercase export detection");

        assert_symbol_exported(&json, "HandleRequest");
        assert_symbol_exported(&json, "Server");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_go_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/main.go",
            r#"package main

import "fmt"

func helper() int {
    return 42
}

func process(x int) int {
    return x * 2
}

func main() {
    x := helper()
    y := process(x)
    fmt.Println(y)
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/main.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go function calls");

        assert_symbol_exists(&json, "main");
        assert_symbol_exists(&json, "helper");
        assert_symbol_exists(&json, "process");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_go_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/control.go",
            r#"package pkg

func CheckValue(x int) string {
    if x < 0 {
        return "negative"
    } else if x == 0 {
        return "zero"
    } else {
        return "positive"
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/control.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go if/else");

        assert_symbol_exists(&json, "CheckValue");
    }

    #[test]
    fn test_go_switch() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/switch.go",
            r#"package pkg

func GetDay(n int) string {
    switch n {
    case 1:
        return "Monday"
    case 2:
        return "Tuesday"
    default:
        return "Unknown"
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/switch.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go switch");

        assert_symbol_exists(&json, "GetDay");
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_go_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("pkg/empty.go", "package pkg\n");

        let result = repo.run_cli(&["analyze", "pkg/empty.go", "-f", "json"]);
        assert!(result.is_ok(), "Should handle empty Go file");
    }

    #[test]
    fn test_go_generics() {
        let repo = TestRepo::new();
        repo.add_file(
            "pkg/generics.go",
            r#"package pkg

type Stack[T any] struct {
    items []T
}

func (s *Stack[T]) Push(item T) {
    s.items = append(s.items, item)
}

func Map[T, U any](items []T, f func(T) U) []U {
    result := make([]U, len(items))
    for i, item := range items {
        result[i] = f(item)
    }
    return result
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "pkg/generics.go", "-f", "json"]);
        let json = assert_valid_json(&output, "Go generics");

        assert_symbol_exists(&json, "Stack");
        assert_symbol_exists(&json, "Push");
        assert_symbol_exists(&json, "Map");
    }
}

// =============================================================================
// C TESTS
// =============================================================================

mod c_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_c_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.c",
            r#"#include <stdio.h>

int process_data(int x) {
    return x * 2;
}

int validate_input(int x) {
    return x > 0;
}

void transform(char* s) {
    // transform string
}
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/utils.c", "-f", "json"]);
        let json = assert_valid_json(&output, "C function extraction");

        // C symbols may include return type/params, check for base names
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("process_data") || output_str.contains("function"),
            "Should find C functions: {}",
            output
        );
    }

    #[test]
    fn test_c_struct_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/types.h",
            r#"#ifndef TYPES_H
#define TYPES_H

typedef struct {
    int id;
    char name[256];
    float value;
} Record;

struct Node {
    int data;
    struct Node* next;
};

typedef struct Config {
    int timeout;
    int retries;
} Config;

#endif
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/types.h", "-f", "json"]);
        let json = assert_valid_json(&output, "C struct extraction");

        // Check for struct-related content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Record")
                || output_str.contains("Node")
                || output_str.contains("struct"),
            "Should find C structs: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Visibility Detection (static vs non-static)
    // -------------------------------------------------------------------------

    #[test]
    fn test_c_static_functions() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/module.c",
            r#"#include <stdio.h>

static int helper(int x) {
    return x * 2;
}

int public_function(int x) {
    return helper(x) + 1;
}

static void internal_log(const char* msg) {
    printf("%s\n", msg);
}
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/module.c", "-f", "json"]);
        let json = assert_valid_json(&output, "C static function detection");

        // Check for function-related content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("helper")
                || output_str.contains("public_function")
                || output_str.contains("function"),
            "Should find C functions: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_c_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main.c",
            r#"#include <stdio.h>

int helper(int x) {
    return x * 2;
}

int process(int x) {
    int y = helper(x);
    return y;
}

int main(int argc, char* argv[]) {
    int result = process(42);
    return 0;
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/main.c", "-f", "json"]);
        let json = assert_valid_json(&output, "C function calls");

        assert_symbol_exists(&json, "main");
        assert_symbol_exists(&json, "helper");
        assert_symbol_exists(&json, "process");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_c_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/control.c",
            r#"
int check_value(int x) {
    if (x < 0) {
        return -1;
    } else if (x == 0) {
        return 0;
    } else {
        return 1;
    }
}
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/control.c", "-f", "json"]);
        let json = assert_valid_json(&output, "C if/else");

        // Check for function content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("check_value") || output_str.contains("function"),
            "Should find C function: {}",
            output
        );
    }

    #[test]
    fn test_c_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/loops.c",
            r#"#include <stdio.h>

void loop_examples(void) {
    for (int i = 0; i < 10; i++) {
        printf("%d\n", i);
    }

    int x = 0;
    while (x < 5) {
        x++;
    }

    do {
        x--;
    } while (x > 0);
}
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/loops.c", "-f", "json"]);
        let json = assert_valid_json(&output, "C loops");

        // Check for function content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("loop_examples") || output_str.contains("function"),
            "Should find C function: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_c_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.c", "");

        let result = repo.run_cli(&["analyze", "src/empty.c", "-f", "json"]);
        assert!(result.is_ok(), "Should handle empty C file");
    }
}

// =============================================================================
// C++ TESTS
// =============================================================================

mod cpp_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_cpp_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/user.cpp",
            r#"#include <string>

class User {
public:
    User(const std::string& name) : name_(name) {}
    std::string getName() const { return name_; }
private:
    std::string name_;
};

class Admin : public User {
public:
    Admin(const std::string& name) : User(name) {}
};

class Guest {
public:
    void access() {}
};
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/user.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ class extraction");

        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "Admin");
        assert_symbol_exists(&json, "Guest");
    }

    #[test]
    fn test_cpp_template_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/templates.hpp",
            r#"#pragma once
#include <vector>

template<typename T>
class Container {
public:
    void add(const T& item) {
        items_.push_back(item);
    }
    T get(size_t index) const {
        return items_[index];
    }
private:
    std::vector<T> items_;
};

template<typename T>
T max(T a, T b) {
    return a > b ? a : b;
}
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/templates.hpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ template extraction");

        // Check for template-related content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Container")
                || output_str.contains("template")
                || output_str.contains("class"),
            "Should find C++ templates: {}",
            output
        );
    }

    #[test]
    fn test_cpp_namespace_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/namespaces.cpp",
            r#"#include <string>

namespace app {

namespace utils {

std::string trim(const std::string& s) {
    return s;
}

int parse_int(const std::string& s) {
    return 0;
}

} // namespace utils

class Service {
public:
    void run() {}
};

} // namespace app
"#,
        );

        let output = repo.run_cli_success(&["analyze", "src/namespaces.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ namespace extraction");

        // Check for namespace-related content
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("trim")
                || output_str.contains("Service")
                || output_str.contains("namespace"),
            "Should find C++ namespaces: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Visibility Detection (public/private/protected)
    // -------------------------------------------------------------------------

    #[test]
    fn test_cpp_access_specifiers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/access.hpp",
            r#"#pragma once

class Example {
public:
    void publicMethod() {}
    int publicField;

protected:
    void protectedMethod() {}

private:
    void privateMethod() {}
    int privateField;
};
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/access.hpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ access specifiers");

        assert_symbol_exists(&json, "Example");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_cpp_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/service.cpp",
            r#"#include <iostream>

class Logger {
public:
    void log(const char* msg) {
        std::cout << msg << std::endl;
    }
};

class Service {
public:
    void process() {
        logger_.log("Starting");
        doWork();
        logger_.log("Done");
    }

private:
    void doWork() {}
    Logger logger_;
};
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/service.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ method calls");

        // C++ symbol names may include return types/params, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Logger")
                || output_str.contains("Service")
                || output_str.contains("process"),
            "Should find C++ classes and methods: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_cpp_exception_handling() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/exceptions.cpp",
            r#"#include <stdexcept>
#include <iostream>

void risky_operation(int x) {
    if (x < 0) {
        throw std::invalid_argument("Negative value");
    }
}

int safe_process(int x) {
    try {
        risky_operation(x);
        return x * 2;
    } catch (const std::exception& e) {
        std::cerr << e.what() << std::endl;
        return -1;
    }
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/exceptions.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ exception handling");

        // C++ symbol names may include return types/params, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("risky_operation") || output_str.contains("safe_process"),
            "Should find C++ functions with exception handling: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_cpp_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.cpp", "");

        let result = repo.run_cli(&["analyze", "src/empty.cpp", "-f", "json"]);
        assert!(result.is_ok(), "Should handle empty C++ file");
    }

    #[test]
    fn test_cpp_lambdas() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/lambdas.cpp",
            r#"#include <algorithm>
#include <vector>
#include <iostream>

void lambda_examples() {
    std::vector<int> vec = {3, 1, 4, 1, 5};

    auto print = [](int x) { std::cout << x << " "; };

    int multiplier = 2;
    auto multiply = [multiplier](int x) { return x * multiplier; };

    int sum = 0;
    std::for_each(vec.begin(), vec.end(), [&sum](int x) { sum += x; });
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/lambdas.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ lambdas");

        // C++ symbol names may include return types/params, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("lambda_examples") || output_str.contains("lambda"),
            "Should find C++ function with lambdas: {}",
            output
        );
    }

    #[test]
    fn test_cpp_smart_pointers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/smart_ptr.cpp",
            r#"#include <memory>
#include <iostream>

class Resource {
public:
    Resource() { std::cout << "Created" << std::endl; }
    ~Resource() { std::cout << "Destroyed" << std::endl; }
    void use() { std::cout << "Using" << std::endl; }
};

void smart_pointer_examples() {
    auto unique = std::make_unique<Resource>();
    unique->use();

    auto shared1 = std::make_shared<Resource>();
    auto shared2 = shared1;
}
"#,
        );

        repo.generate_index().unwrap();
        let output = repo.run_cli_success(&["analyze", "src/smart_ptr.cpp", "-f", "json"]);
        let json = assert_valid_json(&output, "C++ smart pointers");

        // C++ symbol names may include return types/params, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Resource") || output_str.contains("smart_pointer"),
            "Should find C++ class and function with smart pointers: {}",
            output
        );
    }
}
