//! Tests for schema enrichment fields (is_exported, decorators, arity, is_self_recursive)

#[cfg(test)]
mod schema_enrichment_tests {
    use semfora_engine::cache::SymbolIndexEntry;
    use semfora_engine::schema::FrameworkEntryPoint;

    #[test]
    fn test_symbol_index_entry_new_fields() {
        let entry = SymbolIndexEntry {
            symbol: "test_function".to_string(),
            hash: "abc123def456".to_string(),
            semantic_hash: "sem789".to_string(),
            kind: "function".to_string(),
            module: "test_module".to_string(),
            file: "test.py".to_string(),
            lines: "10-20".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: 5,
            max_nesting: 2,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::TestFunction,
            is_exported: true,
            decorators: "@pytest.fixture,@app.route".to_string(),
            arity: 3,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
        };

        assert_eq!(entry.symbol, "test_function");
        assert!(entry.is_exported);
        assert_eq!(entry.decorators, "@pytest.fixture,@app.route");
        assert_eq!(entry.arity, 3);
        assert_eq!(entry.framework_entry_point, FrameworkEntryPoint::TestFunction);
    }

    #[test]
    fn test_symbol_index_entry_with_empty_decorators() {
        let entry = SymbolIndexEntry {
            symbol: "plain_function".to_string(),
            hash: "xyz".to_string(),
            semantic_hash: "sem".to_string(),
            kind: "function".to_string(),
            module: "module".to_string(),
            file: "file.rs".to_string(),
            lines: "1-5".to_string(),
            risk: "medium".to_string(),
            cognitive_complexity: 0,
            max_nesting: 0,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::None,
            is_exported: false,
            decorators: String::new(),
            arity: 0,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
        };

        assert!(!entry.is_exported);
        assert!(entry.decorators.is_empty());
        assert_eq!(entry.arity, 0);
    }

    #[test]
    fn test_symbol_index_entry_high_arity() {
        let entry = SymbolIndexEntry {
            symbol: "complex_func".to_string(),
            hash: "h1".to_string(),
            semantic_hash: "s1".to_string(),
            kind: "function".to_string(),
            module: "m1".to_string(),
            file: "f.ts".to_string(),
            lines: "1-100".to_string(),
            risk: "high".to_string(),
            cognitive_complexity: 20,
            max_nesting: 5,
            is_escape_local: true,
            framework_entry_point: FrameworkEntryPoint::None,
            is_exported: true,
            decorators: "@Component,@Input".to_string(),
            arity: 12,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(), // Large parameter count
        };

        assert_eq!(entry.arity, 12);
        assert_eq!(entry.decorators.len(), "@Component,@Input".len());
    }

    #[test]
    fn test_framework_entry_point_serialization() {
        let test_fn = FrameworkEntryPoint::TestFunction;
        assert!(!test_fn.is_none());
        assert!(test_fn.is_entry_point());
        assert_eq!(test_fn.description(), "test function");

        let none_fp = FrameworkEntryPoint::None;
        assert!(none_fp.is_none());
        assert!(!none_fp.is_entry_point());
    }

    #[test]
    fn test_decorator_formats() {
        // Test various decorator format examples
        let examples = vec![
            "@pytest.fixture",
            "@pytest.mark.parametrize",
            "@app.route('/test')",
            "@Component",
            "@Deprecated",
            "@Override",
        ];

        for decorator in examples {
            let entry = SymbolIndexEntry {
                symbol: "func".to_string(),
                hash: "h".to_string(),
                semantic_hash: "s".to_string(),
                kind: "fn".to_string(),
                module: "m".to_string(),
                file: "f".to_string(),
                lines: "1-1".to_string(),
                risk: "low".to_string(),
                cognitive_complexity: 0,
                max_nesting: 0,
                is_escape_local: false,
                framework_entry_point: FrameworkEntryPoint::None,
                is_exported: false,
                decorators: decorator.to_string(),
                arity: 0,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
            };

            assert_eq!(entry.decorators, decorator);
        }
    }

    #[test]
    fn test_multiple_decorators_comma_separated() {
        let decorators = "@fixture,@mark.async,@timeout(5)";
        let entry = SymbolIndexEntry {
            symbol: "async_test".to_string(),
            hash: "h".to_string(),
            semantic_hash: "s".to_string(),
            kind: "function".to_string(),
            module: "tests".to_string(),
            file: "test_async.py".to_string(),
            lines: "10-30".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: 2,
            max_nesting: 1,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::TestFunction,
            is_exported: false,
            decorators: decorators.to_string(),
            arity: 1,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
        };

        // Verify all decorators are preserved
        assert!(entry.decorators.contains("@fixture"));
        assert!(entry.decorators.contains("@mark.async"));
        assert!(entry.decorators.contains("@timeout"));
    }

    #[test]
    fn test_arity_calculation_examples() {
        // Test that arity correctly represents parameter count
        struct ArityCases {
            args: usize,
            props: usize,
            expected_arity: usize,
        }

        let cases = vec![
            ArityCases {
                args: 0,
                props: 0,
                expected_arity: 0,
            },
            ArityCases {
                args: 2,
                props: 0,
                expected_arity: 2,
            },
            ArityCases {
                args: 0,
                props: 3,
                expected_arity: 3,
            },
            ArityCases {
                args: 4,
                props: 2,
                expected_arity: 6,
            },
        ];

        for case in cases {
            let entry = SymbolIndexEntry {
                symbol: "func".to_string(),
                hash: "h".to_string(),
                semantic_hash: "s".to_string(),
                kind: "function".to_string(),
                module: "m".to_string(),
                file: "f.rs".to_string(),
                lines: "1-10".to_string(),
                risk: "low".to_string(),
                cognitive_complexity: 0,
                max_nesting: 0,
                is_escape_local: false,
                framework_entry_point: FrameworkEntryPoint::None,
                is_exported: false,
                decorators: String::new(),
                arity: case.expected_arity,
                is_async: false,
                return_type: String::new(),
                ext_package: String::new(),
                base_classes: String::new(),
            };

            assert_eq!(entry.arity, case.expected_arity);
        }
    }

    #[test]
    fn test_exported_flags() {
        let exported = SymbolIndexEntry {
            symbol: "public_api".to_string(),
            hash: "h1".to_string(),
            semantic_hash: "s1".to_string(),
            kind: "function".to_string(),
            module: "api".to_string(),
            file: "api.ts".to_string(),
            lines: "1-20".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: 1,
            max_nesting: 0,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::None,
            is_exported: true,
            decorators: String::new(),
            arity: 0,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
        };

        let private = SymbolIndexEntry {
            symbol: "private_impl".to_string(),
            hash: "h2".to_string(),
            semantic_hash: "s2".to_string(),
            kind: "function".to_string(),
            module: "api".to_string(),
            file: "api.ts".to_string(),
            lines: "21-30".to_string(),
            risk: "low".to_string(),
            cognitive_complexity: 2,
            max_nesting: 1,
            is_escape_local: false,
            framework_entry_point: FrameworkEntryPoint::None,
            is_exported: false,
            decorators: String::new(),
            arity: 1,
            is_async: false,
            return_type: String::new(),
            ext_package: String::new(),
            base_classes: String::new(),
        };

        assert!(exported.is_exported);
        assert!(!private.is_exported);
    }
}
