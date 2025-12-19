//! Configuration file format integration tests
//!
//! Tests for JSON, YAML, TOML, and XML - structured data formats
//! commonly used for configuration and data exchange.

#![allow(unused_imports)]
#![allow(clippy::duplicate_mod)]

#[path = "../common/mod.rs"]
mod common;
use common::{assertions::*, TestRepo};

// =============================================================================
// JSON TESTS
// =============================================================================

mod json_tests {
    use super::*;

    #[test]
    fn test_json_object_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/settings.json",
            r#"{
  "name": "my-app",
  "version": "1.0.0",
  "description": "A sample application",
  "main": "index.js",
  "scripts": {
    "start": "node index.js",
    "test": "jest",
    "build": "tsc"
  },
  "dependencies": {
    "express": "^4.18.0",
    "lodash": "^4.17.21"
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/settings.json", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle JSON config file"
        );
    }

    #[test]
    fn test_json_array_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/data.json",
            r#"[
  {
    "id": 1,
    "name": "Alice",
    "email": "alice@example.com"
  },
  {
    "id": 2,
    "name": "Bob",
    "email": "bob@example.com"
  },
  {
    "id": 3,
    "name": "Charlie",
    "email": "charlie@example.com"
  }
]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/data.json", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle JSON array file"
        );
    }

    #[test]
    fn test_json_nested_structure() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/complex.json",
            r#"{
  "database": {
    "primary": {
      "host": "localhost",
      "port": 5432,
      "credentials": {
        "username": "admin",
        "password": "secret"
      }
    },
    "replica": {
      "hosts": ["replica1.example.com", "replica2.example.com"],
      "port": 5432
    }
  },
  "cache": {
    "enabled": true,
    "ttl": 3600,
    "backends": [
      { "type": "redis", "host": "cache.example.com" },
      { "type": "memcached", "host": "memcache.example.com" }
    ]
  },
  "features": {
    "darkMode": false,
    "experimental": null
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/complex.json", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle nested JSON"
        );
    }

    #[test]
    fn test_json_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("config/empty.json", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/empty.json", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty JSON file"
        );
    }

    #[test]
    fn test_json_special_values() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/special.json",
            r#"{
  "nullValue": null,
  "boolTrue": true,
  "boolFalse": false,
  "integer": 42,
  "float": 3.14159,
  "negative": -100,
  "scientific": 1.23e10,
  "emptyString": "",
  "emptyArray": [],
  "emptyObject": {},
  "unicode": "Hello \u0057\u006f\u0072\u006c\u0064"
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/special.json", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle JSON special values"
        );
    }
}

// =============================================================================
// YAML TESTS
// =============================================================================

mod yaml_tests {
    use super::*;

    #[test]
    fn test_yaml_basic_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/settings.yaml",
            r#"name: my-app
version: 1.0.0
description: A sample application

server:
  host: localhost
  port: 8080

database:
  host: db.example.com
  port: 5432
  name: mydb
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/settings.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle YAML config file"
        );
    }

    #[test]
    fn test_yaml_complex_types() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/complex.yaml",
            r#"# Lists
simple_list:
  - item1
  - item2
  - item3

# Nested list
nested_list:
  - name: Alice
    age: 30
  - name: Bob
    age: 25

# Anchors and aliases
defaults: &defaults
  adapter: postgres
  pool: 5

development:
  <<: *defaults
  database: dev_db

production:
  <<: *defaults
  database: prod_db
  pool: 20

# Multi-line strings
literal_block: |
  This is a multi-line string
  that preserves newlines
  exactly as written.

folded_block: >
  This is a folded string
  that joins lines with
  spaces instead.
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/complex.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle complex YAML"
        );
    }

    #[test]
    fn test_yaml_kubernetes() {
        let repo = TestRepo::new();
        repo.add_file(
            "k8s/deployment.yaml",
            r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  labels:
    app: my-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: my-app
  template:
    metadata:
      labels:
        app: my-app
    spec:
      containers:
        - name: my-app
          image: my-app:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: db-secret
                  key: url
          resources:
            limits:
              cpu: "500m"
              memory: "256Mi"
            requests:
              cpu: "250m"
              memory: "128Mi"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 30
            periodSeconds: 10
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "k8s/deployment.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Kubernetes YAML"
        );
    }

    #[test]
    fn test_yaml_github_actions() {
        let repo = TestRepo::new();
        repo.add_file(
            ".github/workflows/ci.yaml",
            r#"name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          override: true

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build
        run: cargo build --release

      - name: Test
        run: cargo test --verbose

  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo fmt -- --check
      - run: cargo clippy -- -D warnings
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", ".github/workflows/ci.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle GitHub Actions YAML"
        );
    }

    #[test]
    fn test_yaml_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("config/empty.yaml", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/empty.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty YAML file"
        );
    }

    #[test]
    fn test_yaml_multi_document() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/multi.yaml",
            r#"---
name: document1
value: 100
---
name: document2
value: 200
---
name: document3
value: 300
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/multi.yaml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle multi-document YAML"
        );
    }
}

// =============================================================================
// TOML TESTS
// =============================================================================

mod toml_tests {
    use super::*;

    #[test]
    fn test_toml_cargo_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "Cargo.toml",
            r#"[package]
name = "my-project"
version = "0.1.0"
edition = "2021"
authors = ["Dev <dev@example.com>"]
description = "A sample Rust project"
license = "MIT"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"

[dev-dependencies]
criterion = "0.5"

[features]
default = ["std"]
std = []
async = ["tokio"]

[[bin]]
name = "my-app"
path = "src/main.rs"

[profile.release]
lto = true
codegen-units = 1
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "Cargo.toml", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle Cargo.toml");
    }

    #[test]
    fn test_toml_pyproject_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "pyproject.toml",
            r#"[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "my-package"
version = "0.1.0"
description = "A sample Python package"
readme = "README.md"
requires-python = ">=3.9"
license = "MIT"
authors = [
    { name = "Dev", email = "dev@example.com" }
]
classifiers = [
    "Development Status :: 3 - Alpha",
    "Programming Language :: Python :: 3.9",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
]
dependencies = [
    "requests>=2.28.0",
    "click>=8.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0.0",
    "black>=23.0.0",
    "ruff>=0.0.270",
]

[project.scripts]
my-cli = "my_package.cli:main"

[tool.black]
line-length = 88
target-version = ["py39", "py310", "py311"]

[tool.ruff]
line-length = 88
select = ["E", "F", "I"]

[tool.pytest.ini_options]
testpaths = ["tests"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "pyproject.toml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle pyproject.toml"
        );
    }

    #[test]
    fn test_toml_complex_types() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/settings.toml",
            r#"# This is a TOML configuration file

title = "My Application"

[owner]
name = "Dev"
dob = 1990-01-15T07:30:00-05:00

[database]
enabled = true
ports = [8000, 8001, 8002]
data = [["delta", "phi"], [3.14]]

[servers]

[servers.alpha]
ip = "10.0.0.1"
role = "frontend"

[servers.beta]
ip = "10.0.0.2"
role = "backend"

# Inline tables
point = { x = 1, y = 2 }

# Arrays of tables
[[products]]
name = "Hammer"
sku = 738594937

[[products]]
name = "Nail"
sku = 284758393
color = "gray"
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/settings.toml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle complex TOML"
        );
    }

    #[test]
    fn test_toml_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("config/empty.toml", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/empty.toml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty TOML file"
        );
    }
}

// =============================================================================
// XML TESTS
// =============================================================================

mod xml_tests {
    use super::*;

    #[test]
    fn test_xml_basic_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/settings.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<configuration>
    <appSettings>
        <add key="name" value="my-app"/>
        <add key="version" value="1.0.0"/>
    </appSettings>

    <connectionStrings>
        <add name="default"
             connectionString="Server=localhost;Database=mydb"
             providerName="System.Data.SqlClient"/>
    </connectionStrings>

    <system.web>
        <compilation debug="true" targetFramework="4.8"/>
        <httpRuntime targetFramework="4.8"/>
    </system.web>
</configuration>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/settings.xml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle XML config file"
        );
    }

    #[test]
    fn test_xml_maven_pom() {
        let repo = TestRepo::new();
        repo.add_file(
            "pom.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
                             http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
    <artifactId>my-project</artifactId>
    <version>1.0-SNAPSHOT</version>
    <packaging>jar</packaging>

    <properties>
        <maven.compiler.source>17</maven.compiler.source>
        <maven.compiler.target>17</maven.compiler.target>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-web</artifactId>
            <version>3.1.0</version>
        </dependency>

        <dependency>
            <groupId>junit</groupId>
            <artifactId>junit</artifactId>
            <version>4.13.2</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-maven-plugin</artifactId>
            </plugin>
        </plugins>
    </build>
</project>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "pom.xml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Maven POM XML"
        );
    }

    #[test]
    fn test_xml_with_namespaces() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/namespaced.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="http://example.com/default"
      xmlns:app="http://example.com/app"
      xmlns:db="http://example.com/database">

    <app:config>
        <app:setting name="debug" value="true"/>
        <app:setting name="logLevel" value="INFO"/>
    </app:config>

    <db:connection>
        <db:host>localhost</db:host>
        <db:port>5432</db:port>
        <db:name>mydb</db:name>
    </db:connection>

</root>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/namespaced.xml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle XML with namespaces"
        );
    }

    #[test]
    fn test_xml_cdata() {
        let repo = TestRepo::new();
        repo.add_file(
            "config/cdata.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<document>
    <script><![CDATA[
        function test() {
            if (x < 10 && y > 5) {
                return true;
            }
            return false;
        }
    ]]></script>

    <html><![CDATA[
        <div class="container">
            <p>Hello &amp; World</p>
        </div>
    ]]></html>
</document>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/cdata.xml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle XML with CDATA"
        );
    }

    #[test]
    fn test_xml_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("config/empty.xml", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "config/empty.xml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty XML file"
        );
    }

    #[test]
    fn test_xml_svg() {
        let repo = TestRepo::new();
        repo.add_file(
            "assets/icon.svg",
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"
     width="100"
     height="100"
     viewBox="0 0 100 100">
    <defs>
        <linearGradient id="grad1" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" style="stop-color:rgb(255,255,0);stop-opacity:1"/>
            <stop offset="100%" style="stop-color:rgb(255,0,0);stop-opacity:1"/>
        </linearGradient>
    </defs>

    <circle cx="50" cy="50" r="40" fill="url(#grad1)"/>

    <text x="50" y="55" text-anchor="middle" font-size="20">Icon</text>
</svg>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "assets/icon.svg", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle SVG XML");
    }
}
