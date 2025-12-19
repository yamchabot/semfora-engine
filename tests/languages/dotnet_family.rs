//! .NET language family integration tests
//!
//! Tests for C# - .NET language with explicit visibility modifiers,
//! namespaces, and modern language features.

#[path = "../common/mod.rs"]
mod common;
use common::assertions::*;
use common::test_repo::TestRepo;

// =============================================================================
// C# TESTS
// =============================================================================

mod csharp_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_csharp_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/User.cs",
            r#"namespace Example;

public class User
{
    private int _id;
    private string _name = "";

    public int GetId() => _id;
    public string GetName() => _name;
    public void SetName(string name) => _name = name;
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/User.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# class extraction");

        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "GetId");
        assert_symbol_exists(&json, "GetName");
    }

    #[test]
    fn test_csharp_interface_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/IRepository.cs",
            r#"namespace Example;

public interface IRepository<T> where T : class
{
    T? GetById(int id);
    IEnumerable<T> GetAll();
    T Add(T entity);
    T Update(T entity);
    bool Delete(int id);
    Task<T?> GetByIdAsync(int id);
    Task<IEnumerable<T>> GetAllAsync();
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/IRepository.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# interface extraction");

        assert_symbol_exists(&json, "IRepository");
        assert_symbol_exists(&json, "GetById");
        assert_symbol_exists(&json, "GetAll");
    }

    #[test]
    fn test_csharp_record_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Records.cs",
            r#"namespace Example;

// Simple record
public record Person(string Name, int Age);

// Record with additional members
public record User(int Id, string Name, string Email)
{
    public string DisplayName => $"{Name} <{Email}>";

    public bool IsAdult => true; // Simplified
}

// Record struct (C# 10)
public readonly record struct Point(double X, double Y);

// Record with init-only properties
public record Config
{
    public string Host { get; init; } = "localhost";
    public int Port { get; init; } = 8080;
    public bool Debug { get; init; } = false;
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Records.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# record extraction");

        assert_symbol_exists(&json, "Person");
        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "Point");
        assert_symbol_exists(&json, "Config");
    }

    #[test]
    fn test_csharp_struct_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Structs.cs",
            r#"namespace Example;

public struct Vector3
{
    public float X { get; set; }
    public float Y { get; set; }
    public float Z { get; set; }

    public Vector3(float x, float y, float z)
    {
        X = x;
        Y = y;
        Z = z;
    }

    public float Magnitude => MathF.Sqrt(X * X + Y * Y + Z * Z);

    public Vector3 Normalized()
    {
        var mag = Magnitude;
        return new Vector3(X / mag, Y / mag, Z / mag);
    }

    public static Vector3 operator +(Vector3 a, Vector3 b)
        => new(a.X + b.X, a.Y + b.Y, a.Z + b.Z);
}

public readonly struct ReadOnlyPoint
{
    public readonly double X;
    public readonly double Y;

    public ReadOnlyPoint(double x, double y) => (X, Y) = (x, y);
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Structs.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# struct extraction");

        assert_symbol_exists(&json, "Vector3");
        assert_symbol_exists(&json, "Magnitude");
        assert_symbol_exists(&json, "Normalized");
    }

    #[test]
    fn test_csharp_enum_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Enums.cs",
            r#"namespace Example;

public enum Status
{
    Pending,
    Active,
    Completed,
    Failed
}

[Flags]
public enum Permissions
{
    None = 0,
    Read = 1,
    Write = 2,
    Execute = 4,
    All = Read | Write | Execute
}

public enum LogLevel : byte
{
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    Critical = 4
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Enums.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# enum extraction");

        assert_symbol_exists(&json, "Status");
        assert_symbol_exists(&json, "Permissions");
        assert_symbol_exists(&json, "LogLevel");
    }

    // -------------------------------------------------------------------------
    // Visibility Detection
    // -------------------------------------------------------------------------

    #[test]
    fn test_csharp_access_modifiers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Visibility.cs",
            r#"namespace Example;

public class PublicClass
{
    public void PublicMethod() { }
    internal void InternalMethod() { }
    protected void ProtectedMethod() { }
    protected internal void ProtectedInternalMethod() { }
    private protected void PrivateProtectedMethod() { }
    private void PrivateMethod() { }

    public string PublicProperty { get; set; } = "";
    private string PrivateProperty { get; set; } = "";
}

internal class InternalClass
{
    public void Method() { }
}

// File-scoped type (C# 11)
file class FileLocalClass
{
    public void Method() { }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Visibility.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# access modifiers");

        assert_symbol_exported(&json, "PublicClass");
        assert_symbol_exported(&json, "PublicMethod");
    }

    #[test]
    fn test_csharp_static_members() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/StaticMembers.cs",
            r#"namespace Example;

public static class Utils
{
    public static string Uppercase(string s) => s.ToUpper();
    public static int Add(int a, int b) => a + b;

    public static readonly string Version = "1.0.0";
    public const int MaxRetries = 3;
}

public class WithStatic
{
    public static int InstanceCount { get; private set; }

    public WithStatic()
    {
        InstanceCount++;
    }

    public static WithStatic Create() => new();
}
"#,
        );
        repo.generate_index().unwrap();

        let output =
            repo.run_cli_success(&["analyze", "src/Example/StaticMembers.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# static members");

        assert_symbol_exported(&json, "Utils");
        assert_symbol_exists(&json, "Uppercase");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_csharp_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Calculator.cs",
            r#"namespace Example;

public class Calculator
{
    private int Helper(int x) => x * 2;

    public int Calculate(int a, int b)
    {
        var x = Helper(a);
        var y = Helper(b);
        return Add(x, y);
    }

    private int Add(int x, int y) => x + y;

    public static void Main(string[] args)
    {
        var calc = new Calculator();
        var result = calc.Calculate(5, 3);
        Console.WriteLine($"Result: {result}");
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Calculator.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# method calls");

        assert_symbol_exists(&json, "Calculator");
        assert_symbol_exists(&json, "Calculate");
        assert_symbol_exists(&json, "Helper");
    }

    #[test]
    fn test_csharp_extension_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Extensions.cs",
            r#"namespace Example;

public static class StringExtensions
{
    public static string Reverse(this string s)
    {
        var arr = s.ToCharArray();
        Array.Reverse(arr);
        return new string(arr);
    }

    public static bool IsNullOrEmpty(this string? s)
        => string.IsNullOrEmpty(s);

    public static string OrDefault(this string? s, string defaultValue)
        => s ?? defaultValue;
}

public class Usage
{
    public void Demo()
    {
        var reversed = "hello".Reverse();
        var isEmpty = "".IsNullOrEmpty();
        var value = ((string?)null).OrDefault("default");
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Extensions.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# extension method calls");

        assert_symbol_exists(&json, "StringExtensions");
        assert_symbol_exists(&json, "Reverse");
    }

    #[test]
    fn test_csharp_linq_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Linq.cs",
            r#"namespace Example;

public class LinqExamples
{
    public void MethodSyntax(List<int> numbers)
    {
        var result = numbers
            .Where(n => n > 0)
            .Select(n => n * 2)
            .OrderByDescending(n => n)
            .ToList();
    }

    public void QuerySyntax(List<Person> people)
    {
        var adults = from p in people
                     where p.Age >= 18
                     orderby p.Name
                     select new { p.Name, p.Age };
    }

    public void Aggregations(List<int> numbers)
    {
        var sum = numbers.Sum();
        var avg = numbers.Average();
        var max = numbers.Max();
        var count = numbers.Count(n => n > 0);
    }
}

public record Person(string Name, int Age);
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Linq.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# LINQ calls");

        assert_symbol_exists(&json, "LinqExamples");
        assert_symbol_exists(&json, "MethodSyntax");
        assert_symbol_exists(&json, "QuerySyntax");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_csharp_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Conditionals.cs",
            r#"namespace Example;

public class Conditionals
{
    public string CheckValue(int x)
    {
        if (x < 0)
        {
            return "negative";
        }
        else if (x == 0)
        {
            return "zero";
        }
        else
        {
            return "positive";
        }
    }

    public string NullCheck(string? s)
    {
        if (s is null)
        {
            return "";
        }
        return s.Trim();
    }

    public int Ternary(int x) => x > 0 ? x : -x;

    public string NullCoalescing(string? s) => s ?? "";

    public int NullCoalescingAssignment(int? x)
    {
        x ??= 0;
        return x.Value;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output =
            repo.run_cli_success(&["analyze", "src/Example/Conditionals.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# if/else");

        assert_symbol_exists(&json, "CheckValue");
        assert_symbol_exists(&json, "Ternary");
    }

    #[test]
    fn test_csharp_switch() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Switches.cs",
            r#"namespace Example;

public class Switches
{
    // Classic switch
    public string GetDay(int n)
    {
        switch (n)
        {
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

    // Switch expression
    public string GetDayExpression(int n) => n switch
    {
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        _ => "Unknown"
    };

    // Pattern matching
    public string Describe(object obj) => obj switch
    {
        int i when i > 0 => $"Positive int: {i}",
        int i => $"Non-positive int: {i}",
        string s => $"String: {s}",
        null => "Null",
        _ => "Unknown"
    };

    // Property pattern
    public string CheckPerson(Person p) => p switch
    {
        { Age: >= 18 } => "Adult",
        { Age: >= 13 } => "Teen",
        _ => "Child"
    };
}

public record Person(string Name, int Age);
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Switches.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# switch");

        assert_symbol_exists(&json, "GetDay");
        assert_symbol_exists(&json, "GetDayExpression");
        assert_symbol_exists(&json, "Describe");
    }

    #[test]
    fn test_csharp_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Loops.cs",
            r#"namespace Example;

public class Loops
{
    public void ForLoop()
    {
        for (int i = 0; i < 10; i++)
        {
            Console.WriteLine(i);
        }
    }

    public void ForeachLoop(List<string> items)
    {
        foreach (var item in items)
        {
            Console.WriteLine(item);
        }
    }

    public void WhileLoop()
    {
        int x = 0;
        while (x < 5)
        {
            x++;
        }
    }

    public void DoWhileLoop()
    {
        int x = 0;
        do
        {
            x++;
        } while (x < 5);
    }

    public async Task AsyncForeach(IAsyncEnumerable<int> stream)
    {
        await foreach (var item in stream)
        {
            Console.WriteLine(item);
        }
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Loops.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# loops");

        assert_symbol_exists(&json, "ForLoop");
        assert_symbol_exists(&json, "ForeachLoop");
        assert_symbol_exists(&json, "AsyncForeach");
    }

    #[test]
    fn test_csharp_exception_handling() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Exceptions.cs",
            r#"namespace Example;

public class Exceptions
{
    public void TryCatch()
    {
        try
        {
            RiskyOperation();
        }
        catch (ArgumentException ex)
        {
            Console.WriteLine($"Argument error: {ex.Message}");
        }
        catch (InvalidOperationException ex)
        {
            Console.WriteLine($"Invalid operation: {ex.Message}");
        }
        catch (Exception ex) when (ex.Message.Contains("critical"))
        {
            Console.WriteLine($"Critical: {ex.Message}");
            throw;
        }
        finally
        {
            Console.WriteLine("Cleanup");
        }
    }

    public void UsingStatement()
    {
        using var resource = new DisposableResource();
        resource.Use();
    }

    public async Task UsingAsync()
    {
        await using var resource = new AsyncDisposableResource();
        await resource.UseAsync();
    }

    private void RiskyOperation()
    {
        throw new InvalidOperationException("Simulated error");
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Exceptions.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# exception handling");

        assert_symbol_exists(&json, "TryCatch");
        assert_symbol_exists(&json, "UsingStatement");
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_csharp_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/Example/Empty.cs", "namespace Example;\n");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/Example/Empty.cs", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty C# file"
        );
    }

    #[test]
    fn test_csharp_async_await() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Async.cs",
            r#"namespace Example;

public class AsyncExamples
{
    public async Task<string> FetchDataAsync(string url)
    {
        using var client = new HttpClient();
        return await client.GetStringAsync(url);
    }

    public async Task<List<string>> FetchAllAsync(IEnumerable<string> urls)
    {
        var tasks = urls.Select(FetchDataAsync);
        var results = await Task.WhenAll(tasks);
        return results.ToList();
    }

    public async ValueTask<int> ProcessAsync(int value)
    {
        await Task.Delay(100);
        return value * 2;
    }

    public async IAsyncEnumerable<int> GenerateAsync()
    {
        for (int i = 0; i < 10; i++)
        {
            await Task.Delay(100);
            yield return i;
        }
    }

    public void FireAndForget()
    {
        _ = Task.Run(async () =>
        {
            await Task.Delay(1000);
            Console.WriteLine("Done");
        });
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Async.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# async/await");

        assert_symbol_exists(&json, "FetchDataAsync");
        assert_symbol_exists(&json, "FetchAllAsync");
        assert_symbol_exists(&json, "GenerateAsync");
    }

    #[test]
    fn test_csharp_generics() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Generics.cs",
            r#"namespace Example;

public class Container<T>
{
    private T _value;

    public Container(T value) => _value = value;

    public T Value => _value;

    public U Map<U>(Func<T, U> mapper) => mapper(_value);
}

public class Repository<T> where T : class, new()
{
    private readonly List<T> _items = new();

    public T Add(T item)
    {
        _items.Add(item);
        return item;
    }

    public T? Find(Func<T, bool> predicate) => _items.FirstOrDefault(predicate);
}

public static class GenericMethods
{
    public static T Max<T>(T a, T b) where T : IComparable<T>
        => a.CompareTo(b) > 0 ? a : b;

    public static (T Min, T Max) MinMax<T>(IEnumerable<T> items) where T : IComparable<T>
    {
        var list = items.ToList();
        return (list.Min()!, list.Max()!);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Generics.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# generics");

        assert_symbol_exists(&json, "Container");
        assert_symbol_exists(&json, "Repository");
        assert_symbol_exists(&json, "Max");
    }

    #[test]
    fn test_csharp_attributes() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Attributes.cs",
            r#"namespace Example;

[AttributeUsage(AttributeTargets.Method | AttributeTargets.Class)]
public class LoggedAttribute : Attribute
{
    public string Category { get; set; } = "Default";
    public LoggedAttribute() { }
    public LoggedAttribute(string category) => Category = category;
}
"#,
        );
        repo.add_file(
            "src/Example/AttributedClass.cs",
            r#"namespace Example;

[Serializable]
[Logged("Service")]
public class AttributedClass
{
    [Obsolete("Use NewMethod instead")]
    public void OldMethod() { }

    [Logged]
    public void NewMethod() { }

    [Required]
    [StringLength(100)]
    public string Name { get; set; } = "";

    [JsonProperty("user_id")]
    public int Id { get; set; }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Attributes.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# attributes");

        assert_symbol_exists(&json, "LoggedAttribute");
    }

    #[test]
    fn test_csharp_lambdas_delegates() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Lambdas.cs",
            r#"namespace Example;

public class Lambdas
{
    // Delegate types
    public delegate int Operation(int x, int y);

    public void DelegateExamples()
    {
        // Anonymous method
        Operation op1 = delegate(int x, int y) { return x + y; };

        // Lambda expression
        Operation op2 = (x, y) => x + y;

        // Statement lambda
        Func<int, int, int> op3 = (x, y) =>
        {
            var result = x * y;
            return result;
        };

        // Action
        Action<string> printer = s => Console.WriteLine(s);

        // Predicate
        Predicate<int> isPositive = n => n > 0;

        // Local function
        int LocalAdd(int a, int b) => a + b;
        var result = LocalAdd(1, 2);

        // Static local function
        static int StaticAdd(int a, int b) => a + b;
    }

    public void LinqLambdas(List<int> numbers)
    {
        var doubled = numbers.Select(n => n * 2);
        var filtered = numbers.Where(n => n > 0);
        var sum = numbers.Aggregate((a, b) => a + b);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Lambdas.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# lambdas and delegates");

        assert_symbol_exists(&json, "Lambdas");
        assert_symbol_exists(&json, "DelegateExamples");
    }

    #[test]
    fn test_csharp_nullable_reference_types() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Nullable.cs",
            r#"#nullable enable

namespace Example;

public class NullableExamples
{
    // Nullable reference type
    public string? NullableString { get; set; }

    // Non-nullable with default
    public string NonNullable { get; set; } = "";

    public string SafeAccess(Person? person)
    {
        // Null-conditional
        var name = person?.Name;

        // Null-forgiving
        var length = person!.Name.Length;

        // Pattern matching
        if (person is { Name: var n })
        {
            return n;
        }

        return "";
    }

    public string HandleNullable(string? input)
    {
        // Null coalescing
        var result = input ?? "default";

        // Null coalescing assignment
        input ??= "default";

        return input;
    }
}

public record Person(string Name, int Age);
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Nullable.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# nullable reference types");

        assert_symbol_exists(&json, "NullableExamples");
        assert_symbol_exists(&json, "SafeAccess");
    }

    #[test]
    fn test_csharp_primary_constructors() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/PrimaryConstructors.cs",
            r#"namespace Example;

// C# 12 primary constructors
public class Service(ILogger logger, IRepository repo)
{
    public void Process()
    {
        logger.Log("Processing");
        var items = repo.GetAll();
    }

    public ILogger Logger => logger;
}

public struct Point(double x, double y)
{
    public double X { get; } = x;
    public double Y { get; } = y;

    public double Distance => Math.Sqrt(X * X + Y * Y);
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&[
            "analyze",
            "src/Example/PrimaryConstructors.cs",
            "-f",
            "json",
        ]);
        let json = assert_valid_json(&output, "C# primary constructors");

        assert_symbol_exists(&json, "Service");
        assert_symbol_exists(&json, "Point");
    }

    #[test]
    fn test_csharp_collection_expressions() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/Example/Collections.cs",
            r#"namespace Example;

public class CollectionExamples
{
    public void CollectionExpressions()
    {
        // C# 12 collection expressions
        int[] array = [1, 2, 3, 4, 5];
        List<string> list = ["a", "b", "c"];
        HashSet<int> set = [1, 2, 3];
        Dictionary<string, int> dict = new() { ["a"] = 1, ["b"] = 2 };

        // Spread operator
        int[] combined = [..array, 6, 7, 8];
        List<string> allItems = [..list, "d", "e"];
    }

    public void IndexAndRange()
    {
        int[] numbers = [1, 2, 3, 4, 5];

        // Index from end
        var last = numbers[^1];
        var secondLast = numbers[^2];

        // Range
        var slice = numbers[1..4];
        var fromStart = numbers[..3];
        var toEnd = numbers[2..];
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/Example/Collections.cs", "-f", "json"]);
        let json = assert_valid_json(&output, "C# collection expressions");

        assert_symbol_exists(&json, "CollectionExamples");
        assert_symbol_exists(&json, "CollectionExpressions");
    }
}
