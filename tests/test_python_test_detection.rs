//! Tests for Python test function detection in the Python detector

#[cfg(test)]
mod python_test_detection {
    use semfora_engine::schema::FrameworkEntryPoint;

    /// Replicate the is_test_file_path logic from python.rs for testing
    fn is_test_file_path(path: &str) -> bool {
        let path_lower = path.replace('\\', "/").to_lowercase();
        // Directory component: .../tests/... or .../test/... (or starts with tests/ or test/)
        if path_lower.contains("/tests/")
            || path_lower.contains("/test/")
            || path_lower.starts_with("tests/")
            || path_lower.starts_with("test/")
        {
            return true;
        }
        // Filename pattern: extract the last segment
        if let Some(filename) = path_lower.split('/').next_back() {
            let stem = filename.strip_suffix(".py").unwrap_or(filename);
            if stem.starts_with("test_") || stem.ends_with("_test") {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_test_file_path_detection() {
        // Patterns that should be detected as test files
        let test_files = vec![
            "test_module.py",
            "module_test.py",
            "tests/test_utils.py",
            "test/test_helpers.py",
            "src/tests/module_test.py",
            "/path/to/tests/something.py",
            "tests/unit/test_core.py",
            "src/test/integration_test.py",
        ];

        for path in test_files {
            assert!(
                is_test_file_path(path),
                "Path '{}' should be detected as test file",
                path
            );
        }
    }

    #[test]
    fn test_non_test_file_path_detection() {
        // Patterns that should NOT be detected as test files
        let normal_files = vec![
            "module.py",
            "utils.py",
            "helpers.py",
            "src/module.py",
            "/path/to/module.py",
            "testing.py", // contains 'test' but not as file pattern
            "protest.py",  // ends with 'test' but not as separate pattern
        ];

        for path in normal_files {
            assert!(
                !is_test_file_path(path),
                "Path '{}' should NOT be detected as test file",
                path
            );
        }
    }

    #[test]
    fn test_test_function_name_patterns() {
        // Function names that should trigger TestFunction framework entry point
        let test_names = vec![
            "test_basic",
            "test_module_initialization",
            "test_edge_cases",
            "TestCase",
            "TestClass",
        ];

        for name in test_names {
            let is_test = name.starts_with("test_") || name.starts_with("Test");
            assert!(
                is_test,
                "Function/class name '{}' should be detected as test",
                name
            );
        }
    }

    #[test]
    fn test_non_test_function_names() {
        // Names that should NOT be test functions
        let normal_names = vec![
            "process_data",
            "calculate_value",
            "main",
            "helper_function",
            "Request",    // starts with capital but not 'Test'
            "testing",    // contains 'test' but not at start
            "retest",     // ends with 'test' but not appropriate
        ];

        for name in normal_names {
            let is_test = name.starts_with("test_") || name.starts_with("Test");
            assert!(
                !is_test,
                "Function/class name '{}' should NOT be detected as test",
                name
            );
        }
    }

    #[test]
    fn test_pytest_decorator_detection() {
        // Decorators that indicate test functions
        let pytest_decorators = vec![
            "@pytest.fixture",
            "@pytest.mark.parametrize",
            "@pytest.mark.skip",
            "@unittest.TestCase",
        ];

        for decorator in pytest_decorators {
            let has_pytest = decorator.to_lowercase().contains("pytest")
                || decorator.to_lowercase().contains("unittest");
            assert!(
                has_pytest,
                "Decorator '{}' should be detected as pytest/unittest",
                decorator
            );
        }
    }

    #[test]
    fn test_framework_entry_point_test_function() {
        let test_fp = FrameworkEntryPoint::TestFunction;
        
        assert!(!test_fp.is_none(), "TestFunction should not be None");
        assert!(test_fp.is_entry_point(), "TestFunction should be an entry point");
        assert_eq!(test_fp.description(), "test function");
    }

    #[test]
    fn test_mixed_test_detection_scenarios() {
        // Scenario 1: Function named test_* in regular file
        let scenario1: (&str, &str, Vec<&str>) = (
            "test_calculation.py",
            "test_addition",
            vec![],
        );
        assert!(is_test_file_path(scenario1.0));
        assert!(scenario1.1.starts_with("test_"));

        // Scenario 2: Regular function in tests/ directory
        let scenario2: (&str, &str, Vec<&str>) = (
            "src/tests/fixtures.py",
            "create_fixture",
            vec![],
        );
        assert!(is_test_file_path(scenario2.0));
        // Function name is not test_*, but file is in tests/ so still test context

        // Scenario 3: Function with pytest decorator in normal file
        let scenario3: (&str, &str, Vec<&str>) = (
            "utils.py",
            "helper",
            vec!["@pytest.fixture"],
        );
        assert!(!is_test_file_path(scenario3.0));
        let has_pytest = scenario3.2.iter().any(|d| d.contains("pytest"));
        assert!(has_pytest);

        // Scenario 4: Test class in test file
        let scenario4: (&str, &str, Vec<&str>) = (
            "test_integration.py",
            "TestIntegration",
            vec![],
        );
        assert!(is_test_file_path(scenario4.0));
        assert!(scenario4.1.starts_with("Test"));
    }

    #[test]
    fn test_windows_path_handling() {
        // Windows-style paths should work after normalization
        let win_paths = vec![
            r"tests\test_module.py",
            r"src\tests\test_utils.py",
            r"C:\project\test\test_integration.py",
        ];

        for path in win_paths {
            assert!(
                is_test_file_path(path),
                "Windows path '{}' should be detected as test file",
                path
            );
        }
    }

    #[test]
    fn test_edge_cases_in_paths() {
        // Edge cases for path detection
        assert!(is_test_file_path("test_a.py")); // Single character name
        assert!(is_test_file_path("_test.py")); // Underscore prefix
        assert!(is_test_file_path("tests/a.py")); // Very short name in tests/
        assert!(!is_test_file_path(".py")); // No real name
    }
}
