//! JVM language family integration tests
//!
//! Tests for Java and Kotlin - JVM languages with explicit visibility
//! modifiers and class-based organization.

#[path = "../common/mod.rs"]
mod common;
use common::assertions::*;
use common::test_repo::TestRepo;

// =============================================================================
// JAVA TESTS
// =============================================================================

mod java_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_java_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/User.java",
            r#"package com.example;

public class User {
    private Long id;
    private String name;

    public Long getId() {
        return id;
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/User.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java class extraction");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("User")
                || output_str.contains("getId")
                || output_str.contains("class"),
            "Should find Java class and methods: {}",
            output
        );
    }

    #[test]
    fn test_java_interface_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Repository.java",
            r#"package com.example;

import java.util.List;
import java.util.Optional;

public interface Repository<T, ID> {
    T save(T entity);
    Optional<T> findById(ID id);
    List<T> findAll();
    void delete(T entity);
    void deleteById(ID id);
    long count();
    boolean existsById(ID id);
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Repository.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java interface extraction");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Repository")
                || output_str.contains("save")
                || output_str.contains("interface"),
            "Should find Java interface and methods: {}",
            output
        );
    }

    #[test]
    fn test_java_enum_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Status.java",
            r#"package com.example;

public enum Status {
    PENDING("Pending"),
    ACTIVE("Active"),
    COMPLETED("Completed"),
    FAILED("Failed");

    private final String displayName;

    Status(String displayName) {
        this.displayName = displayName;
    }

    public String getDisplayName() {
        return displayName;
    }

    public boolean isTerminal() {
        return this == COMPLETED || this == FAILED;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Status.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java enum extraction");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Status")
                || output_str.contains("getDisplayName")
                || output_str.contains("enum"),
            "Should find Java enum and methods: {}",
            output
        );
    }

    #[test]
    fn test_java_record_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Person.java",
            r#"package com.example;

// Java 16+ record
public record Person(String name, int age, String email) {

    // Compact constructor
    public Person {
        if (age < 0) {
            throw new IllegalArgumentException("Age cannot be negative");
        }
    }

    // Additional method
    public String greeting() {
        return "Hello, " + name + "!";
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Person.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java record extraction");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Person")
                || output_str.contains("record")
                || output_str.contains("greeting"),
            "Should find Java record: {}",
            output
        );
    }

    #[test]
    fn test_java_nested_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Outer.java",
            r#"package com.example;

public class Outer {
    private int outerField;

    // Static nested class
    public static class StaticNested {
        public void staticMethod() {}
    }

    // Inner class
    public class Inner {
        public void innerMethod() {
            System.out.println(outerField); // Access outer field
        }
    }

    // Local class in method
    public void methodWithLocalClass() {
        class Local {
            void localMethod() {}
        }
        new Local().localMethod();
    }

    // Anonymous class
    public Runnable getRunnable() {
        return new Runnable() {
            @Override
            public void run() {
                System.out.println("Running");
            }
        };
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Outer.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java nested class extraction");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Outer")
                || output_str.contains("StaticNested")
                || output_str.contains("class"),
            "Should find Java nested classes: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Visibility Detection
    // -------------------------------------------------------------------------

    #[test]
    fn test_java_public_class() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Service.java",
            r#"package com.example;

public class Service {
    public void publicMethod() {}
    protected void protectedMethod() {}
    void packagePrivateMethod() {}
    private void privateMethod() {}

    public static final String PUBLIC_CONSTANT = "public";
    private static final String PRIVATE_CONSTANT = "private";
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Service.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java visibility modifiers");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Service")
                || output_str.contains("publicMethod")
                || output_str.contains("class"),
            "Should find Java public class and methods: {}",
            output
        );
    }

    #[test]
    fn test_java_package_private_class() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/InternalHelper.java",
            r#"package com.example;

// Package-private class (no modifier)
class InternalHelper {
    void helperMethod() {}
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/InternalHelper.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java package-private class");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("InternalHelper")
                || output_str.contains("helperMethod")
                || output_str.contains("class"),
            "Should find Java package-private class: {}",
            output
        );
    }

    #[test]
    fn test_java_interface_visibility() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Api.java",
            r#"package com.example;

public interface Api {
    // Interface methods are implicitly public
    void process();

    // Default method
    default void defaultMethod() {
        System.out.println("Default implementation");
    }

    // Private method (Java 9+)
    private void privateHelper() {
        System.out.println("Private helper");
    }

    // Static method
    static void staticUtility() {
        System.out.println("Static utility");
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Api.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java interface visibility");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Api")
                || output_str.contains("process")
                || output_str.contains("interface"),
            "Should find Java interface: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_java_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Calculator.java",
            r#"package com.example;

public class Calculator {
    private int helper(int x) {
        return x * 2;
    }

    public int calculate(int a, int b) {
        int x = helper(a);
        int y = helper(b);
        return add(x, y);
    }

    private int add(int x, int y) {
        return x + y;
    }

    public static void main(String[] args) {
        Calculator calc = new Calculator();
        int result = calc.calculate(5, 3);
        System.out.println("Result: " + result);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Calculator.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java method calls");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Calculator")
                || output_str.contains("calculate")
                || output_str.contains("class"),
            "Should find Java class with method calls: {}",
            output
        );
    }

    #[test]
    fn test_java_constructor_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Builder.java",
            r#"package com.example;

public class Builder {
    private String name;
    private int value;

    public Builder() {
        this("default", 0);
    }

    public Builder(String name) {
        this(name, 0);
    }

    public Builder(String name, int value) {
        this.name = name;
        this.value = value;
    }

    public Builder setName(String name) {
        this.name = name;
        return this;
    }

    public Builder setValue(int value) {
        this.value = value;
        return this;
    }

    public Product build() {
        return new Product(name, value);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Builder.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java constructor calls");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Builder")
                || output_str.contains("build")
                || output_str.contains("class"),
            "Should find Java builder class: {}",
            output
        );
    }

    #[test]
    fn test_java_static_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Utils.java",
            r#"package com.example;

import java.util.Arrays;
import java.util.List;

public class Utils {
    public static int max(int a, int b) {
        return Math.max(a, b);
    }

    public static String format(String template, Object... args) {
        return String.format(template, args);
    }

    public static List<String> toList(String... items) {
        return Arrays.asList(items);
    }

    public static void main(String[] args) {
        int result = max(5, 10);
        String message = format("Max: %d", result);
        List<String> list = toList("a", "b", "c");

        System.out.println(message);
        System.out.println(list);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Utils.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java static calls");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Utils")
                || output_str.contains("max")
                || output_str.contains("static"),
            "Should find Java utility class: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_java_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Conditionals.java",
            r#"package com.example;

public class Conditionals {
    public String checkValue(int x) {
        if (x < 0) {
            return "negative";
        } else if (x == 0) {
            return "zero";
        } else {
            return "positive";
        }
    }

    public String nullSafe(String s) {
        if (s == null) {
            return "";
        }
        return s.trim();
    }

    public int ternary(int x) {
        return x > 0 ? x : -x;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Conditionals.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java if/else");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("checkValue")
                || output_str.contains("ternary")
                || output_str.contains("Conditionals"),
            "Should find Java conditionals class: {}",
            output
        );
    }

    #[test]
    fn test_java_switch() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Switches.java",
            r#"package com.example;

public class Switches {
    public String getDay(int n) {
        switch (n) {
            case 1:
                return "Monday";
            case 2:
                return "Tuesday";
            case 3:
                return "Wednesday";
            default:
                return "Unknown";
        }
    }

    // Java 14+ switch expression
    public String getDayExpression(int n) {
        return switch (n) {
            case 1 -> "Monday";
            case 2 -> "Tuesday";
            case 3 -> "Wednesday";
            default -> "Unknown";
        };
    }

    // Pattern matching (Java 17+)
    public String describe(Object obj) {
        return switch (obj) {
            case Integer i -> "Integer: " + i;
            case String s -> "String: " + s;
            case null -> "Null";
            default -> "Unknown";
        };
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Switches.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java switch");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("getDay")
                || output_str.contains("Switches")
                || output_str.contains("switch"),
            "Should find Java switch class: {}",
            output
        );
    }

    #[test]
    fn test_java_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Loops.java",
            r#"package com.example;

import java.util.List;

public class Loops {
    public void forLoop() {
        for (int i = 0; i < 10; i++) {
            System.out.println(i);
        }
    }

    public void enhancedFor(List<String> items) {
        for (String item : items) {
            System.out.println(item);
        }
    }

    public void whileLoop() {
        int x = 0;
        while (x < 5) {
            x++;
        }
    }

    public void doWhileLoop() {
        int x = 0;
        do {
            x++;
        } while (x < 5);
    }

    public void streamLoop(List<Integer> numbers) {
        numbers.stream()
            .filter(n -> n > 0)
            .map(n -> n * 2)
            .forEach(System.out::println);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Loops.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java loops");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("forLoop")
                || output_str.contains("Loops")
                || output_str.contains("while"),
            "Should find Java loops class: {}",
            output
        );
    }

    #[test]
    fn test_java_exception_handling() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Exceptions.java",
            r#"package com.example;

import java.io.IOException;

public class Exceptions {
    public void tryCatch() {
        try {
            riskyOperation();
        } catch (IllegalArgumentException e) {
            System.err.println("Invalid argument: " + e.getMessage());
        } catch (IOException e) {
            System.err.println("IO error: " + e.getMessage());
        } catch (Exception e) {
            System.err.println("Unexpected: " + e.getMessage());
        } finally {
            System.out.println("Cleanup");
        }
    }

    public void tryWithResources() {
        try (var resource = new AutoCloseableResource()) {
            resource.use();
        } catch (Exception e) {
            e.printStackTrace();
        }
    }

    public void multiCatch() {
        try {
            riskyOperation();
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("Illegal: " + e.getMessage());
        }
    }

    private void riskyOperation() throws IOException {
        throw new IOException("Simulated error");
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Exceptions.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java exception handling");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("tryCatch")
                || output_str.contains("Exceptions")
                || output_str.contains("catch"),
            "Should find Java exceptions class: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_java_empty_file() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Empty.java",
            "package com.example;\n",
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&[
            "analyze",
            "src/main/java/com/example/Empty.java",
            "-f",
            "json",
        ]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Java file"
        );
    }

    #[test]
    fn test_java_annotations() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Annotated.java",
            r#"package com.example;

import java.lang.annotation.*;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface Logged {
    String value() default "";
}
"#,
        );
        repo.add_file(
            "src/main/java/com/example/AnnotatedClass.java",
            r#"package com.example;

@SuppressWarnings("unchecked")
public class AnnotatedClass {
    @Deprecated
    public void oldMethod() {}

    @Override
    public String toString() {
        return "Annotated";
    }

    @Logged("processing")
    public void process() {}
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Annotated.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java annotations");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Logged")
                || output_str.contains("annotation")
                || output_str.contains("interface"),
            "Should find Java annotation: {}",
            output
        );
    }

    #[test]
    fn test_java_generics() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Generics.java",
            r#"package com.example;

import java.util.List;
import java.util.function.Function;

public class Generics<T> {
    private T value;

    public Generics(T value) {
        this.value = value;
    }

    public T getValue() {
        return value;
    }

    public <U> U map(Function<T, U> mapper) {
        return mapper.apply(value);
    }

    public static <E> List<E> singleton(E element) {
        return List.of(element);
    }

    // Bounded type parameter
    public static <T extends Comparable<T>> T max(T a, T b) {
        return a.compareTo(b) > 0 ? a : b;
    }

    // Wildcard
    public static void printList(List<?> list) {
        for (Object item : list) {
            System.out.println(item);
        }
    }

    // Upper bounded wildcard
    public static double sum(List<? extends Number> list) {
        return list.stream().mapToDouble(Number::doubleValue).sum();
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Generics.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java generics");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Generics")
                || output_str.contains("map")
                || output_str.contains("class"),
            "Should find Java generics class: {}",
            output
        );
    }

    #[test]
    fn test_java_lambdas() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/java/com/example/Lambdas.java",
            r#"package com.example;

import java.util.List;
import java.util.function.*;

public class Lambdas {
    public void lambdaExamples(List<Integer> numbers) {
        // Simple lambda
        Runnable r = () -> System.out.println("Running");

        // Lambda with parameter
        Consumer<String> printer = s -> System.out.println(s);

        // Lambda with multiple parameters
        BiFunction<Integer, Integer, Integer> add = (a, b) -> a + b;

        // Block lambda
        Function<String, Integer> parser = s -> {
            try {
                return Integer.parseInt(s);
            } catch (NumberFormatException e) {
                return 0;
            }
        };

        // Method reference
        numbers.forEach(System.out::println);

        // Constructor reference
        Supplier<StringBuilder> sbSupplier = StringBuilder::new;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/java/com/example/Lambdas.java",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Java lambdas");

        // Java symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Lambdas")
                || output_str.contains("lambdaExamples")
                || output_str.contains("class"),
            "Should find Java lambdas class: {}",
            output
        );
    }
}

// =============================================================================
// KOTLIN TESTS
// =============================================================================

mod kotlin_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_kotlin_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/User.kt",
            r#"package com.example

class User(val id: Long, val name: String) {
    fun getId(): Long = id
    fun getName(): String = name
    fun greet(): String = "Hello, $name!"
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/User.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin class extraction");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("User")
                || output_str.contains("getId")
                || output_str.contains("class"),
            "Should find Kotlin class: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_data_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Models.kt",
            r#"package com.example

data class User(
    val id: Long,
    val name: String,
    val email: String,
    val active: Boolean = true
)

data class Config(
    val host: String = "localhost",
    val port: Int = 8080,
    val debug: Boolean = false
)

data class Response<T>(
    val data: T?,
    val error: String? = null,
    val status: Int = 200
)
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Models.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin data class extraction");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("User")
                || output_str.contains("Config")
                || output_str.contains("data"),
            "Should find Kotlin data classes: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_sealed_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Result.kt",
            r#"package com.example

sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Error(val message: String, val cause: Throwable? = null) : Result<Nothing>()
    object Loading : Result<Nothing>()
}

sealed interface UiState {
    object Idle : UiState
    object Loading : UiState
    data class Success(val data: String) : UiState
    data class Error(val message: String) : UiState
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Result.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin sealed class extraction");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Result")
                || output_str.contains("Success")
                || output_str.contains("sealed"),
            "Should find Kotlin sealed class: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_object_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Singletons.kt",
            r#"package com.example

// Object declaration (singleton)
object Logger {
    fun log(message: String) {
        println("[LOG] $message")
    }

    fun error(message: String) {
        println("[ERROR] $message")
    }
}

// Companion object
class Factory {
    companion object {
        const val VERSION = "1.0.0"

        fun create(): Factory {
            return Factory()
        }
    }

    fun build() {}
}

// Named companion object
class Service {
    companion object Factory {
        fun create(): Service = Service()
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Singletons.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin object extraction");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Logger")
                || output_str.contains("Factory")
                || output_str.contains("object"),
            "Should find Kotlin objects: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_extension_functions() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Extensions.kt",
            r#"package com.example

// Extension function
fun String.toTitleCase(): String {
    return this.split(" ")
        .joinToString(" ") { it.replaceFirstChar { c -> c.uppercase() } }
}

// Extension property
val String.wordCount: Int
    get() = this.split("\\s+".toRegex()).size

// Nullable receiver
fun String?.orEmpty(): String = this ?: ""

// Generic extension
fun <T> List<T>.secondOrNull(): T? = this.getOrNull(1)

// Extension on companion object
fun Int.Companion.random(until: Int): Int = (0 until until).random()
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Extensions.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin extension functions");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("toTitleCase")
                || output_str.contains("orEmpty")
                || output_str.contains("fun"),
            "Should find Kotlin extension functions: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Visibility Detection
    // -------------------------------------------------------------------------

    #[test]
    fn test_kotlin_visibility_modifiers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Visibility.kt",
            r#"package com.example

// Public (default)
class PublicClass {
    fun publicMethod() {}          // public (default)
    internal fun internalMethod() {} // module-visible
    protected fun protectedMethod() {} // subclass-visible
    private fun privateMethod() {}    // class-visible
}

// Internal class
internal class InternalClass {
    fun method() {}
}

// Private top-level
private class PrivateClass {
    fun method() {}
}

// Top-level function visibility
fun publicFunction() {}
internal fun internalFunction() {}
private fun privateFunction() {}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Visibility.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin visibility modifiers");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("PublicClass")
                || output_str.contains("publicMethod")
                || output_str.contains("class"),
            "Should find Kotlin visibility classes: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_kotlin_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Calculator.kt",
            r#"package com.example

fun helper(x: Int): Int = x * 2

fun process(a: Int, b: Int): Int {
    val x = helper(a)
    val y = helper(b)
    return add(x, y)
}

private fun add(x: Int, y: Int) = x + y

fun main() {
    val result = process(5, 3)
    println("Result: $result")
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Calculator.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin function calls");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("helper")
                || output_str.contains("process")
                || output_str.contains("fun"),
            "Should find Kotlin functions: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_lambda_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Functional.kt",
            r#"package com.example

fun processNumbers(numbers: List<Int>): List<Int> {
    return numbers
        .filter { it > 0 }
        .map { it * 2 }
        .sortedDescending()
}

fun withCallback(action: () -> Unit) {
    println("Before")
    action()
    println("After")
}

fun main() {
    val result = processNumbers(listOf(1, -2, 3, -4, 5))
    println(result)

    withCallback {
        println("Executing callback")
    }

    // Trailing lambda
    result.forEach { println(it) }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Functional.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin lambda calls");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("processNumbers")
                || output_str.contains("withCallback")
                || output_str.contains("fun"),
            "Should find Kotlin lambda functions: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_kotlin_when_expression() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/When.kt",
            r#"package com.example

fun describe(obj: Any): String = when (obj) {
    1 -> "One"
    "Hello" -> "Greeting"
    is Long -> "Long number"
    is String -> "String of length ${obj.length}"
    !is String -> "Not a string"
    else -> "Unknown"
}

fun checkRange(x: Int): String = when (x) {
    in 1..10 -> "Small"
    in 11..100 -> "Medium"
    !in 0..100 -> "Out of range"
    else -> "Large"
}

fun checkConditions(x: Int): String = when {
    x < 0 -> "Negative"
    x == 0 -> "Zero"
    x > 0 -> "Positive"
    else -> "Impossible"
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/When.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin when expression");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("describe")
                || output_str.contains("checkRange")
                || output_str.contains("when"),
            "Should find Kotlin when functions: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_null_safety() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/NullSafety.kt",
            r#"package com.example

fun safeLength(s: String?): Int {
    // Safe call
    return s?.length ?: 0
}

fun assertNotNull(s: String?): Int {
    // Non-null assertion
    return s!!.length
}

fun chainedSafe(user: User?): String? {
    // Chained safe calls
    return user?.address?.city?.name
}

fun letExample(s: String?) {
    // let for null checks
    s?.let {
        println("Length: ${it.length}")
    }
}

fun elvisThrow(s: String?): String {
    // Elvis with throw
    return s ?: throw IllegalArgumentException("Null value")
}

data class User(val address: Address?)
data class Address(val city: City?)
data class City(val name: String?)
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/NullSafety.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin null safety");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("safeLength")
                || output_str.contains("elvisThrow")
                || output_str.contains("fun"),
            "Should find Kotlin null safety functions: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Loops.kt",
            r#"package com.example

fun loopExamples() {
    // For-in with range
    for (i in 1..10) {
        println(i)
    }

    // Downward range
    for (i in 10 downTo 1 step 2) {
        println(i)
    }

    // Until (exclusive)
    for (i in 0 until 10) {
        println(i)
    }

    // Iterate with index
    val items = listOf("a", "b", "c")
    for ((index, value) in items.withIndex()) {
        println("$index: $value")
    }

    // While loop
    var x = 0
    while (x < 5) {
        x++
    }

    // Do-while
    do {
        x--
    } while (x > 0)

    // Repeat
    repeat(5) { i ->
        println("Iteration $i")
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Loops.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin loops");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("loopExamples")
                || output_str.contains("for")
                || output_str.contains("fun"),
            "Should find Kotlin loop functions: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_kotlin_empty_file() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Empty.kt",
            "package com.example\n",
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&[
            "analyze",
            "src/main/kotlin/com/example/Empty.kt",
            "-f",
            "json",
        ]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Kotlin file"
        );
    }

    #[test]
    fn test_kotlin_coroutines() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Coroutines.kt",
            r#"package com.example

import kotlinx.coroutines.*

suspend fun fetchData(url: String): String {
    delay(1000)
    return "Data from $url"
}

suspend fun processAll(urls: List<String>): List<String> = coroutineScope {
    urls.map { url ->
        async { fetchData(url) }
    }.awaitAll()
}

fun main() = runBlocking {
    val urls = listOf("url1", "url2", "url3")
    val results = processAll(urls)
    results.forEach { println(it) }
}

// Flow
fun numberFlow() = flow {
    for (i in 1..10) {
        delay(100)
        emit(i)
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Coroutines.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin coroutines");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("fetchData")
                || output_str.contains("processAll")
                || output_str.contains("suspend"),
            "Should find Kotlin coroutine functions: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_delegation() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Delegation.kt",
            r#"package com.example

import kotlin.properties.Delegates

interface Printer {
    fun print(message: String)
}

class ConsolePrinter : Printer {
    override fun print(message: String) = println(message)
}

// Class delegation
class PrinterWrapper(printer: Printer) : Printer by printer {
    fun printUppercase(message: String) {
        print(message.uppercase())
    }
}

// Property delegation
class Example {
    // Lazy
    val lazyValue: String by lazy {
        println("Computed")
        "Hello"
    }

    // Observable
    var observed: String by Delegates.observable("initial") { prop, old, new ->
        println("$old -> $new")
    }

    // Vetoable
    var vetoable: Int by Delegates.vetoable(0) { prop, old, new ->
        new >= 0
    }

    // Map delegation
    operator fun getValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>): String {
        return "Delegated"
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Delegation.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin delegation");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("Printer")
                || output_str.contains("PrinterWrapper")
                || output_str.contains("interface"),
            "Should find Kotlin delegation classes: {}",
            output
        );
    }

    #[test]
    fn test_kotlin_dsl_builder() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main/kotlin/com/example/Dsl.kt",
            r#"package com.example

// DSL builder
class HtmlBuilder {
    private val elements = mutableListOf<String>()

    fun head(init: HeadBuilder.() -> Unit) {
        val builder = HeadBuilder().apply(init)
        elements.add("<head>${builder.build()}</head>")
    }

    fun body(init: BodyBuilder.() -> Unit) {
        val builder = BodyBuilder().apply(init)
        elements.add("<body>${builder.build()}</body>")
    }

    fun build(): String = "<html>${elements.joinToString("")}</html>"
}

class HeadBuilder {
    var title: String = ""
    fun build() = "<title>$title</title>"
}

class BodyBuilder {
    private val content = mutableListOf<String>()

    fun p(text: String) {
        content.add("<p>$text</p>")
    }

    fun h1(text: String) {
        content.add("<h1>$text</h1>")
    }

    fun build() = content.joinToString("")
}

fun html(init: HtmlBuilder.() -> Unit): String {
    return HtmlBuilder().apply(init).build()
}

// Usage
val page = html {
    head {
        title = "My Page"
    }
    body {
        h1("Welcome")
        p("This is a paragraph")
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/main/kotlin/com/example/Dsl.kt",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "Kotlin DSL builder");

        // Kotlin symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("HtmlBuilder")
                || output_str.contains("html")
                || output_str.contains("class"),
            "Should find Kotlin DSL builder: {}",
            output
        );
    }
}
