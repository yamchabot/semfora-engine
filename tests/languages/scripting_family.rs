//! Scripting language family integration tests
//!
//! Tests for Python and Bash - dynamic scripting languages with
//! specific visibility and module conventions.

#[path = "../common/mod.rs"]
mod common;
use common::{assertions::*, TestRepo};

// =============================================================================
// PYTHON TESTS
// =============================================================================

mod python_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_python_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/utils.py",
            r#"def process_data(data):
    """Process incoming data."""
    return data.strip().upper()


def validate_input(value):
    """Validate input value."""
    if not value:
        raise ValueError("Value cannot be empty")
    return True


def transform(items):
    """Transform a list of items."""
    return [item.lower() for item in items]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/utils.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python function extraction");

        assert_symbol_exists(&json, "process_data");
        assert_symbol_exists(&json, "validate_input");
        assert_symbol_exists(&json, "transform");
    }

    #[test]
    fn test_python_class_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/models.py",
            r#"class User:
    """A user model."""

    def __init__(self, name: str, email: str):
        self.name = name
        self.email = email

    def greet(self) -> str:
        return f"Hello, {self.name}!"

    def _private_method(self):
        """Private by convention."""
        pass


class Admin(User):
    """Admin user with extra permissions."""

    def __init__(self, name: str, email: str, role: str):
        super().__init__(name, email)
        self.role = role

    def has_permission(self, permission: str) -> bool:
        return True


class _InternalHelper:
    """Internal class, not part of public API."""
    pass
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/models.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python class extraction");

        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "Admin");
        assert_symbol_exists(&json, "_InternalHelper");
    }

    #[test]
    fn test_python_method_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/service.py",
            r#"class Service:
    def __init__(self):
        self._cache = {}

    def process(self, data):
        """Public method."""
        return self._transform(data)

    def _transform(self, data):
        """Private method by convention."""
        return data.upper()

    @staticmethod
    def create():
        """Static factory method."""
        return Service()

    @classmethod
    def from_config(cls, config):
        """Class method factory."""
        instance = cls()
        instance._cache = config.get("cache", {})
        return instance

    @property
    def cache_size(self):
        """Property accessor."""
        return len(self._cache)
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/service.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python method extraction");

        assert_symbol_exists(&json, "Service");
        assert_symbol_exists(&json, "process");
        assert_symbol_exists(&json, "create");
    }

    #[test]
    fn test_python_async_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/async_utils.py",
            r#"import asyncio
from typing import List


async def fetch_data(url: str) -> dict:
    """Async function to fetch data."""
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.json()


async def process_all(urls: List[str]) -> List[dict]:
    """Process multiple URLs concurrently."""
    tasks = [fetch_data(url) for url in urls]
    return await asyncio.gather(*tasks)


async def main():
    urls = ["http://example.com/1", "http://example.com/2"]
    results = await process_all(urls)
    return results
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/async_utils.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python async function extraction");

        assert_symbol_exists(&json, "fetch_data");
        assert_symbol_exists(&json, "process_all");
        assert_symbol_exists(&json, "main");
    }

    #[test]
    fn test_python_dataclass_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/dataclasses_example.py",
            r#"from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class Point:
    x: float
    y: float


@dataclass
class User:
    id: int
    name: str
    email: str
    active: bool = True
    tags: List[str] = field(default_factory=list)


@dataclass(frozen=True)
class Config:
    host: str
    port: int = 8080
    debug: bool = False


@dataclass
class Response:
    status: int
    data: Optional[dict] = None
    error: Optional[str] = None
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/dataclasses_example.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python dataclass extraction");

        assert_symbol_exists(&json, "Point");
        assert_symbol_exists(&json, "User");
        assert_symbol_exists(&json, "Config");
        assert_symbol_exists(&json, "Response");
    }

    // -------------------------------------------------------------------------
    // Visibility Detection (underscore convention)
    // -------------------------------------------------------------------------

    #[test]
    fn test_python_underscore_private() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/visibility.py",
            r#"# Public function (no underscore)
def public_function():
    return "public"


# Private function (single underscore)
def _private_function():
    return "private"


# Name-mangled (double underscore, typically in classes)
def __very_private():
    return "very private"


# Public class
class PublicClass:
    def public_method(self):
        pass

    def _private_method(self):
        pass


# Private class
class _PrivateClass:
    pass


# Public constant
MAX_SIZE = 100

# Private constant
_INTERNAL_CACHE = {}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/visibility.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python underscore visibility");

        assert_symbol_exported(&json, "public_function");
        assert_symbol_exported(&json, "PublicClass");
        // _private_function should not be marked as exported
    }

    #[test]
    fn test_python_dunder_methods() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/dunders.py",
            r#"class Container:
    def __init__(self):
        self._items = []

    def __len__(self):
        return len(self._items)

    def __getitem__(self, index):
        return self._items[index]

    def __setitem__(self, index, value):
        self._items[index] = value

    def __iter__(self):
        return iter(self._items)

    def __repr__(self):
        return f"Container({self._items})"

    def __str__(self):
        return str(self._items)

    def __eq__(self, other):
        return self._items == other._items

    def __hash__(self):
        return hash(tuple(self._items))
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/dunders.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python dunder methods");

        assert_symbol_exists(&json, "Container");
        assert_symbol_exists(&json, "__init__");
        assert_symbol_exists(&json, "__len__");
    }

    #[test]
    fn test_python_all_export() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/module.py",
            r#"# Explicit exports via __all__
__all__ = ["public_api", "PublicClass"]


def public_api():
    """This is exported via __all__."""
    return "public"


def _private_helper():
    """Not in __all__, private."""
    return "private"


def also_public():
    """Public by convention but not in __all__."""
    return "also public"


class PublicClass:
    """Exported via __all__."""
    pass


class _InternalClass:
    """Not exported."""
    pass
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/module.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python __all__ export");

        assert_symbol_exists(&json, "public_api");
        assert_symbol_exists(&json, "PublicClass");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_python_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/main.py",
            r#"def helper(x):
    return x * 2


def process(data):
    result = helper(data)
    return result + 1


def main():
    value = process(10)
    print(f"Result: {value}")
    return value


if __name__ == "__main__":
    main()
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/main.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python function calls");

        assert_symbol_exists(&json, "helper");
        assert_symbol_exists(&json, "process");
        assert_symbol_exists(&json, "main");
    }

    #[test]
    fn test_python_method_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/builder.py",
            r#"class Builder:
    def __init__(self):
        self._parts = []

    def add(self, part):
        self._parts.append(part)
        return self

    def build(self):
        return "".join(self._parts)


def create_string():
    return (
        Builder()
        .add("Hello")
        .add(" ")
        .add("World")
        .build()
    )
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/builder.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python method calls");

        assert_symbol_exists(&json, "Builder");
        assert_symbol_exists(&json, "create_string");
    }

    #[test]
    fn test_python_import_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/imports.py",
            r#"import os
import json
from pathlib import Path
from typing import List, Dict
from collections import defaultdict
import urllib.parse as urlparse


def read_json_file(filepath: str) -> Dict:
    path = Path(filepath)
    if not path.exists():
        raise FileNotFoundError(f"File not found: {filepath}")

    with open(path, "r") as f:
        return json.load(f)


def get_env_vars() -> Dict[str, str]:
    return dict(os.environ)


def parse_url(url: str) -> Dict:
    parsed = urlparse.urlparse(url)
    return {
        "scheme": parsed.scheme,
        "netloc": parsed.netloc,
        "path": parsed.path,
    }
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/imports.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python import calls");

        assert_symbol_exists(&json, "read_json_file");
        assert_symbol_exists(&json, "get_env_vars");
        assert_symbol_exists(&json, "parse_url");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_python_if_elif_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/control.py",
            r#"def check_value(x):
    if x < 0:
        return "negative"
    elif x == 0:
        return "zero"
    else:
        return "positive"


def validate(data):
    if data is None:
        raise ValueError("Data cannot be None")

    if not isinstance(data, dict):
        raise TypeError("Data must be a dict")

    return True


def process_optional(value):
    # Conditional expression (ternary)
    result = value * 2 if value is not None else 0
    return result
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/control.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python if/elif/else");

        assert_symbol_exists(&json, "check_value");
        assert_symbol_exists(&json, "validate");
    }

    #[test]
    fn test_python_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/loops.py",
            r#"def loop_examples():
    # for loop
    for i in range(10):
        print(i)

    # for with enumerate
    items = ["a", "b", "c"]
    for index, item in enumerate(items):
        print(f"{index}: {item}")

    # for with zip
    names = ["Alice", "Bob"]
    ages = [30, 25]
    for name, age in zip(names, ages):
        print(f"{name} is {age}")

    # while loop
    x = 0
    while x < 5:
        x += 1

    # comprehensions
    squares = [x ** 2 for x in range(10)]
    evens = [x for x in range(10) if x % 2 == 0]
    mapping = {k: v for k, v in zip(names, ages)}
    unique = {x for x in [1, 1, 2, 2, 3]}

    return squares, evens, mapping, unique
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/loops.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python loops");

        assert_symbol_exists(&json, "loop_examples");
    }

    #[test]
    fn test_python_exception_handling() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/exceptions.py",
            r#"class CustomError(Exception):
    """Custom exception class."""
    pass


def risky_operation():
    raise CustomError("Something went wrong")


def safe_process():
    try:
        result = risky_operation()
        return result
    except CustomError as e:
        print(f"Custom error: {e}")
        return None
    except ValueError:
        print("Value error")
        return None
    except Exception as e:
        print(f"Unexpected error: {e}")
        raise
    finally:
        print("Cleanup")


def with_context_manager(path):
    try:
        with open(path, "r") as f:
            return f.read()
    except FileNotFoundError:
        return ""
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/exceptions.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python exception handling");

        assert_symbol_exists(&json, "CustomError");
        assert_symbol_exists(&json, "safe_process");
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_python_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.py", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/empty.py", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Python file"
        );
    }

    #[test]
    fn test_python_comments_only() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/comments.py",
            r#"# This is a comment
# Another comment

"""
This is a module-level docstring.
It spans multiple lines.
"""

# More comments
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/comments.py", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle comments-only Python file"
        );
    }

    #[test]
    fn test_python_decorators() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/decorators.py",
            r#"from functools import wraps
import time


def timer(func):
    @wraps(func)
    def wrapper(*args, **kwargs):
        start = time.time()
        result = func(*args, **kwargs)
        end = time.time()
        print(f"{func.__name__} took {end - start:.2f}s")
        return result
    return wrapper


def retry(times=3):
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            for i in range(times):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    if i == times - 1:
                        raise
            return None
        return wrapper
    return decorator


@timer
def slow_function():
    time.sleep(1)


@retry(times=5)
@timer
def flaky_function():
    import random
    if random.random() < 0.5:
        raise Exception("Random failure")
    return "success"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/decorators.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python decorators");

        assert_symbol_exists(&json, "timer");
        assert_symbol_exists(&json, "retry");
        assert_symbol_exists(&json, "slow_function");
    }

    #[test]
    fn test_python_generators() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/generators.py",
            r#"def count_up(n):
    """Generator function."""
    i = 0
    while i < n:
        yield i
        i += 1


def fibonacci():
    """Infinite fibonacci generator."""
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


def read_large_file(path):
    """Generator for reading large files."""
    with open(path, "r") as f:
        for line in f:
            yield line.strip()


# Generator expression
squares_gen = (x ** 2 for x in range(10))


async def async_generator():
    """Async generator (Python 3.6+)."""
    for i in range(10):
        await asyncio.sleep(0.1)
        yield i
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/generators.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python generators");

        assert_symbol_exists(&json, "count_up");
        assert_symbol_exists(&json, "fibonacci");
        assert_symbol_exists(&json, "read_large_file");
    }

    #[test]
    fn test_python_type_hints() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/typed.py",
            r#"from typing import (
    List, Dict, Optional, Union, Tuple,
    Callable, TypeVar, Generic, Protocol
)


T = TypeVar("T")


def process_list(items: List[int]) -> List[int]:
    return [x * 2 for x in items]


def get_or_default(d: Dict[str, T], key: str, default: T) -> T:
    return d.get(key, default)


def maybe_int(x: Optional[int]) -> int:
    return x if x is not None else 0


def mixed(value: Union[int, str]) -> str:
    return str(value)


class Stack(Generic[T]):
    def __init__(self) -> None:
        self._items: List[T] = []

    def push(self, item: T) -> None:
        self._items.append(item)

    def pop(self) -> Optional[T]:
        return self._items.pop() if self._items else None


class Processor(Protocol):
    def process(self, data: str) -> str: ...
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/typed.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python type hints");

        assert_symbol_exists(&json, "process_list");
        assert_symbol_exists(&json, "Stack");
    }

    #[test]
    fn test_python_context_managers() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/context.py",
            r#"from contextlib import contextmanager
import time


class ManagedResource:
    """Class-based context manager."""

    def __init__(self, name):
        self.name = name

    def __enter__(self):
        print(f"Entering {self.name}")
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        print(f"Exiting {self.name}")
        return False  # Don't suppress exceptions

    def do_something(self):
        print(f"Doing something with {self.name}")


@contextmanager
def timer(name):
    """Generator-based context manager."""
    start = time.time()
    try:
        yield
    finally:
        end = time.time()
        print(f"{name} took {end - start:.2f}s")


def use_context_managers():
    with ManagedResource("test") as resource:
        resource.do_something()

    with timer("operation"):
        time.sleep(0.1)
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "src/context.py", "-f", "json"]);
        let json = assert_valid_json(&output, "Python context managers");

        assert_symbol_exists(&json, "ManagedResource");
        assert_symbol_exists(&json, "timer");
    }
}

// =============================================================================
// BASH TESTS
// =============================================================================

mod bash_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_bash_function_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/utils.sh",
            r#"#!/bin/bash

process_file() {
    local file="$1"
    echo "Processing $file"
}

validate_input() {
    local input="$1"
    if [[ -z "$input" ]]; then
        return 1
    fi
    return 0
}

cleanup() {
    echo "Cleaning up..."
    rm -rf /tmp/temp_*
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/utils.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash function extraction");

        assert_symbol_exists(&json, "process_file");
        assert_symbol_exists(&json, "validate_input");
        assert_symbol_exists(&json, "cleanup");
    }

    #[test]
    fn test_bash_function_styles() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/functions.sh",
            r#"#!/bin/bash

# POSIX style
my_function() {
    echo "POSIX style"
}

# Bash function keyword style
function another_function {
    echo "Bash style"
}

# Bash function keyword with parens
function third_function() {
    echo "Bash style with parens"
}

# One-liner
one_liner() { echo "One liner"; }

# Function with local variables
process_data() {
    local input="$1"
    local output=""

    output=$(echo "$input" | tr '[:lower:]' '[:upper:]')
    echo "$output"
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/functions.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash function styles");

        assert_symbol_exists(&json, "my_function");
        assert_symbol_exists(&json, "another_function");
        assert_symbol_exists(&json, "third_function");
        assert_symbol_exists(&json, "process_data");
    }

    #[test]
    fn test_bash_variable_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/variables.sh",
            r#"#!/bin/bash

# Global variables
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/config.ini"
LOG_FILE="/var/log/myapp.log"

# Arrays
declare -a FILES=("file1.txt" "file2.txt" "file3.txt")
declare -A CONFIG=(
    ["host"]="localhost"
    ["port"]="8080"
)

# Readonly
readonly VERSION="1.0.0"
readonly -a SUPPORTED_OS=("linux" "darwin")

# Export
export PATH="$PATH:$SCRIPT_DIR/bin"
export APP_ENV="production"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/variables.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash variable extraction");

        assert_symbol_exists(&json, "SCRIPT_DIR");
        assert_symbol_exists(&json, "VERSION");
    }

    // -------------------------------------------------------------------------
    // Call Graph
    // -------------------------------------------------------------------------

    #[test]
    fn test_bash_function_calls() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/main.sh",
            r#"#!/bin/bash

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

validate() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        log "File not found: $file"
        return 1
    fi
    return 0
}

process() {
    local file="$1"
    validate "$file" || return 1
    log "Processing $file"
    cat "$file"
}

main() {
    log "Starting script"
    for file in "$@"; do
        process "$file"
    done
    log "Done"
}

main "$@"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/main.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash function calls");

        assert_symbol_exists(&json, "log");
        assert_symbol_exists(&json, "validate");
        assert_symbol_exists(&json, "process");
        assert_symbol_exists(&json, "main");
    }

    #[test]
    fn test_bash_command_substitution() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/commands.sh",
            r#"#!/bin/bash

get_date() {
    echo "$(date '+%Y-%m-%d')"
}

get_user() {
    echo "$(whoami)"
}

build_message() {
    local user=$(get_user)
    local date=$(get_date)
    echo "Hello $user, today is $date"
}

# Using backticks (legacy)
HOSTNAME=`hostname`

# Using $()
CURRENT_DIR=$(pwd)
FILE_COUNT=$(ls -1 | wc -l)
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/commands.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash command substitution");

        assert_symbol_exists(&json, "get_date");
        assert_symbol_exists(&json, "build_message");
    }

    // -------------------------------------------------------------------------
    // Control Flow
    // -------------------------------------------------------------------------

    #[test]
    fn test_bash_if_else() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/conditionals.sh",
            r#"#!/bin/bash

check_file() {
    local file="$1"

    if [[ -f "$file" ]]; then
        echo "Regular file"
    elif [[ -d "$file" ]]; then
        echo "Directory"
    elif [[ -L "$file" ]]; then
        echo "Symbolic link"
    else
        echo "Does not exist"
    fi
}

check_number() {
    local n="$1"

    if (( n < 0 )); then
        echo "Negative"
    elif (( n == 0 )); then
        echo "Zero"
    else
        echo "Positive"
    fi
}

check_string() {
    local s="$1"

    if [[ -z "$s" ]]; then
        echo "Empty"
    elif [[ "$s" == "yes" ]]; then
        echo "Affirmative"
    else
        echo "Other"
    fi
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/conditionals.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash if/else");

        assert_symbol_exists(&json, "check_file");
        assert_symbol_exists(&json, "check_number");
        assert_symbol_exists(&json, "check_string");
    }

    #[test]
    fn test_bash_case() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/case.sh",
            r#"#!/bin/bash

handle_option() {
    local opt="$1"

    case "$opt" in
        -h|--help)
            echo "Usage: script [options]"
            ;;
        -v|--version)
            echo "Version 1.0.0"
            ;;
        -f|--file)
            echo "File option"
            ;;
        *)
            echo "Unknown option: $opt"
            return 1
            ;;
    esac
}

get_os() {
    case "$(uname -s)" in
        Linux*)
            echo "linux"
            ;;
        Darwin*)
            echo "macos"
            ;;
        MINGW*|CYGWIN*)
            echo "windows"
            ;;
        *)
            echo "unknown"
            ;;
    esac
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/case.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash case");

        assert_symbol_exists(&json, "handle_option");
        assert_symbol_exists(&json, "get_os");
    }

    #[test]
    fn test_bash_loops() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/loops.sh",
            r#"#!/bin/bash

for_loop_examples() {
    # C-style for
    for ((i = 0; i < 10; i++)); do
        echo "$i"
    done

    # For-in with list
    for item in one two three; do
        echo "$item"
    done

    # For-in with array
    local arr=("a" "b" "c")
    for element in "${arr[@]}"; do
        echo "$element"
    done

    # For-in with glob
    for file in *.txt; do
        echo "$file"
    done

    # For-in with command substitution
    for line in $(cat file.txt); do
        echo "$line"
    done
}

while_loop_examples() {
    local count=0
    while (( count < 5 )); do
        echo "$count"
        ((count++))
    done

    # Read lines from file
    while IFS= read -r line; do
        echo "$line"
    done < file.txt
}

until_loop_example() {
    local x=0
    until (( x >= 5 )); do
        echo "$x"
        ((x++))
    done
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/loops.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash loops");

        assert_symbol_exists(&json, "for_loop_examples");
        assert_symbol_exists(&json, "while_loop_examples");
        assert_symbol_exists(&json, "until_loop_example");
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_bash_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("scripts/empty.sh", "#!/bin/bash\n");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "scripts/empty.sh", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Bash file"
        );
    }

    #[test]
    fn test_bash_comments_only() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/comments.sh",
            r#"#!/bin/bash
# This is a comment
# Another comment

: '
This is a multi-line comment
using the colon operator
'

# More comments
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "scripts/comments.sh", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle comments-only Bash file"
        );
    }

    #[test]
    fn test_bash_heredoc() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/heredoc.sh",
            r#"#!/bin/bash

generate_config() {
    cat <<EOF
[database]
host=localhost
port=5432
EOF
}

generate_json() {
    local name="$1"
    cat <<-JSON
	{
	    "name": "$name",
	    "version": "1.0.0"
	}
	JSON
}

# Heredoc with no variable expansion
print_literal() {
    cat <<'END'
$HOME is literally $HOME
Variables are not expanded
END
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/heredoc.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash heredoc");

        assert_symbol_exists(&json, "generate_config");
        assert_symbol_exists(&json, "generate_json");
    }

    #[test]
    fn test_bash_error_handling() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/errors.sh",
            r#"#!/bin/bash
set -euo pipefail

cleanup() {
    echo "Cleaning up..."
}

trap cleanup EXIT

die() {
    echo "Error: $1" >&2
    exit 1
}

require_command() {
    local cmd="$1"
    command -v "$cmd" >/dev/null 2>&1 || die "Required command not found: $cmd"
}

safe_cd() {
    local dir="$1"
    cd "$dir" || die "Failed to cd to $dir"
}

main() {
    require_command "git"
    require_command "curl"

    safe_cd "/tmp"

    # This will exit on error due to set -e
    false && echo "This won't print"

    echo "Done"
}

main "$@"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/errors.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash error handling");

        assert_symbol_exists(&json, "cleanup");
        assert_symbol_exists(&json, "die");
        assert_symbol_exists(&json, "require_command");
    }

    #[test]
    fn test_bash_argument_parsing() {
        let repo = TestRepo::new();
        repo.add_file(
            "scripts/args.sh",
            r#"#!/bin/bash

show_usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] FILE...

Options:
    -h, --help      Show this help
    -v, --verbose   Verbose output
    -o, --output    Output file
EOF
}

parse_args() {
    local verbose=false
    local output=""
    local files=()

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -h|--help)
                show_usage
                exit 0
                ;;
            -v|--verbose)
                verbose=true
                shift
                ;;
            -o|--output)
                output="$2"
                shift 2
                ;;
            --)
                shift
                files+=("$@")
                break
                ;;
            -*)
                echo "Unknown option: $1" >&2
                exit 1
                ;;
            *)
                files+=("$1")
                shift
                ;;
        esac
    done

    echo "Verbose: $verbose"
    echo "Output: $output"
    echo "Files: ${files[*]}"
}

main() {
    parse_args "$@"
}

main "$@"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "scripts/args.sh", "-f", "json"]);
        let json = assert_valid_json(&output, "Bash argument parsing");

        assert_symbol_exists(&json, "show_usage");
        assert_symbol_exists(&json, "parse_args");
    }
}
