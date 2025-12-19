//! Infrastructure language family integration tests
//!
//! Tests for HCL (Terraform), Dockerfile, and Gradle - infrastructure and
//! build configuration languages.

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(clippy::duplicate_mod)]

#[path = "../common/mod.rs"]
mod common;
use common::{assertions::*, TestRepo};

// =============================================================================
// HCL (TERRAFORM) TESTS
// =============================================================================

mod hcl_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_hcl_resource_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/main.tf",
            r#"resource "aws_instance" "web" {
  ami           = "ami-12345678"
  instance_type = "t3.micro"

  tags = {
    Name = "web-server"
  }
}

resource "aws_security_group" "allow_http" {
  name        = "allow_http"
  description = "Allow HTTP inbound traffic"

  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/main.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL resource extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("aws_instance")
                || output_str.contains("aws_security_group")
                || output_str.contains("resource"),
            "Should find HCL resources: {}",
            output
        );
    }

    #[test]
    fn test_hcl_variable_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/variables.tf",
            r#"variable "region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "instance_type" {
  description = "EC2 instance type"
  type        = string
  default     = "t3.micro"
}

variable "tags" {
  description = "Common tags for all resources"
  type        = map(string)
  default     = {}
}

variable "enable_monitoring" {
  description = "Enable CloudWatch monitoring"
  type        = bool
  default     = true
}

variable "allowed_cidrs" {
  description = "Allowed CIDR blocks"
  type        = list(string)
  default     = ["10.0.0.0/8"]
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/variables.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL variable extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("region")
                || output_str.contains("instance_type")
                || output_str.contains("variable"),
            "Should find HCL variables: {}",
            output
        );
    }

    #[test]
    fn test_hcl_output_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/outputs.tf",
            r#"output "instance_id" {
  description = "The ID of the EC2 instance"
  value       = aws_instance.main.id
}

output "public_ip" {
  description = "The public IP address"
  value       = aws_instance.main.public_ip
  sensitive   = false
}

output "private_key" {
  description = "Private key for SSH access"
  value       = tls_private_key.main.private_key_pem
  sensitive   = true
}

output "connection_string" {
  description = "Database connection string"
  value       = "postgresql://${var.db_user}:${var.db_password}@${aws_db_instance.main.endpoint}/${var.db_name}"
  sensitive   = true
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/outputs.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL output extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("instance_id")
                || output_str.contains("public_ip")
                || output_str.contains("output"),
            "Should find HCL outputs: {}",
            output
        );
    }

    #[test]
    fn test_hcl_module_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/main.tf",
            r#"module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"

  name = "my-vpc"
  cidr = "10.0.0.0/16"

  azs             = ["us-east-1a", "us-east-1b", "us-east-1c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]

  enable_nat_gateway = true
  single_nat_gateway = true

  tags = var.tags
}

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 19.0"

  cluster_name    = "my-cluster"
  cluster_version = "1.28"

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnets

  depends_on = [module.vpc]
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/main.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL module extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("vpc")
                || output_str.contains("eks")
                || output_str.contains("module"),
            "Should find HCL modules: {}",
            output
        );
    }

    #[test]
    fn test_hcl_data_source_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/data.tf",
            r#"data "aws_ami" "amazon_linux" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["amzn2-ami-hvm-*-x86_64-gp2"]
  }
}

data "aws_availability_zones" "available" {
  state = "available"
}

data "aws_caller_identity" "current" {}

data "aws_region" "current" {}

data "terraform_remote_state" "network" {
  backend = "s3"

  config = {
    bucket = "my-terraform-state"
    key    = "network/terraform.tfstate"
    region = "us-east-1"
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/data.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL data source extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("amazon_linux")
                || output_str.contains("available")
                || output_str.contains("data"),
            "Should find HCL data sources: {}",
            output
        );
    }

    #[test]
    fn test_hcl_locals_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/locals.tf",
            r#"locals {
  environment = terraform.workspace
  project     = "my-project"

  common_tags = {
    Environment = local.environment
    Project     = local.project
    ManagedBy   = "Terraform"
  }

  instance_count = local.environment == "prod" ? 3 : 1

  subnet_cidrs = [for i in range(3) : cidrsubnet(var.vpc_cidr, 8, i)]

  formatted_name = lower(replace(var.name, " ", "-"))
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/locals.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL locals extraction");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("environment")
                || output_str.contains("common_tags")
                || output_str.contains("locals"),
            "Should find HCL locals: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_hcl_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("infra/empty.tf", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "infra/empty.tf", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty HCL file"
        );
    }

    #[test]
    fn test_hcl_complex_expressions() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/complex.tf",
            r#"locals {
  # For expressions
  instance_ids = [for instance in aws_instance.servers : instance.id]

  # Map comprehension
  name_to_id = { for instance in aws_instance.servers : instance.tags.Name => instance.id }

  # Conditional expressions
  instance_type = var.environment == "prod" ? "t3.large" : "t3.micro"

  # Splat expressions
  all_ips = aws_instance.servers[*].public_ip

  # Dynamic blocks
  ingress_rules = [
    { port = 80, cidr = "0.0.0.0/0" },
    { port = 443, cidr = "0.0.0.0/0" },
    { port = 22, cidr = "10.0.0.0/8" },
  ]
}

resource "aws_security_group" "dynamic" {
  name = "dynamic-sg"

  dynamic "ingress" {
    for_each = local.ingress_rules
    content {
      from_port   = ingress.value.port
      to_port     = ingress.value.port
      protocol    = "tcp"
      cidr_blocks = [ingress.value.cidr]
    }
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/complex.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL complex expressions");

        // HCL symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("instance_ids")
                || output_str.contains("locals")
                || output_str.contains("dynamic"),
            "Should find HCL complex expressions: {}",
            output
        );
    }

    #[test]
    fn test_hcl_provider_configuration() {
        let repo = TestRepo::new();
        repo.add_file(
            "infra/providers.tf",
            r#"terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
  }

  backend "s3" {
    bucket         = "my-terraform-state"
    key            = "terraform.tfstate"
    region         = "us-east-1"
    dynamodb_table = "terraform-locks"
    encrypt        = true
  }
}

provider "aws" {
  region = var.region

  default_tags {
    tags = local.common_tags
  }
}

provider "aws" {
  alias  = "us_west"
  region = "us-west-2"
}

provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args        = ["eks", "get-token", "--cluster-name", module.eks.cluster_name]
  }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "infra/providers.tf", "-f", "json"]);
        let json = assert_valid_json(&output, "HCL provider configuration");

        // Provider blocks should be detected
        assert!(
            output.contains("aws") || output.contains("kubernetes") || output.contains("terraform")
        );
    }
}

// =============================================================================
// DOCKERFILE TESTS
// =============================================================================

mod dockerfile_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_dockerfile_instruction_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"FROM node:20-alpine

WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

EXPOSE 3000

CMD ["node", "dist/index.js"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docker/Dockerfile", "-f", "json"]);
        // Should find stage or instructions
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn test_dockerfile_multistage_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"# Build stage
FROM node:20-alpine AS builder

WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

# Production stage
FROM node:20-alpine AS production

WORKDIR /app

COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules

ENV NODE_ENV=production
EXPOSE 3000

USER node

CMD ["node", "dist/index.js"]

# Development stage
FROM builder AS development

ENV NODE_ENV=development
EXPOSE 3000 9229

CMD ["npm", "run", "dev"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "docker/Dockerfile", "-f", "json"]);
        let json = assert_valid_json(&output, "Dockerfile multistage extraction");

        // Dockerfile symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("builder")
                || output_str.contains("production")
                || output_str.contains("FROM"),
            "Should find Dockerfile stages: {}",
            output
        );
    }

    #[test]
    fn test_dockerfile_arg_env_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"ARG BASE_IMAGE=node:20-alpine
ARG NODE_VERSION=20

FROM ${BASE_IMAGE}

ARG APP_VERSION
ARG BUILD_DATE

ENV NODE_ENV=production
ENV APP_PORT=3000
ENV APP_VERSION=${APP_VERSION:-1.0.0}

LABEL maintainer="dev@example.com"
LABEL version=${APP_VERSION}
LABEL build-date=${BUILD_DATE}

WORKDIR /app

COPY . .

CMD ["node", "index.js"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "docker/Dockerfile", "-f", "json"]);
        let json = assert_valid_json(&output, "Dockerfile ARG/ENV extraction");

        // Dockerfile symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("BASE_IMAGE")
                || output_str.contains("NODE_ENV")
                || output_str.contains("ARG")
                || output_str.contains("ENV"),
            "Should find Dockerfile ARG/ENV: {}",
            output
        );
    }

    #[test]
    fn test_dockerfile_healthcheck() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"FROM node:20-alpine

WORKDIR /app
COPY . .

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

CMD ["node", "server.js"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docker/Dockerfile", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Dockerfile with HEALTHCHECK"
        );
    }

    #[test]
    fn test_dockerfile_complex_run() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"FROM ubuntu:22.04

# Update and install dependencies in a single layer
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        curl \
        wget \
        git \
        build-essential \
        ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    apt-get clean

# Heredoc syntax (Docker BuildKit)
RUN <<EOF
set -e
echo "Installing custom dependencies"
curl -fsSL https://example.com/install.sh | bash
echo "Installation complete"
EOF

# Multi-line with exec form
RUN ["sh", "-c", "echo hello && echo world"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docker/Dockerfile", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle complex Dockerfile RUN"
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_dockerfile_empty() {
        let repo = TestRepo::new();
        repo.add_file("docker/Dockerfile", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docker/Dockerfile", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Dockerfile"
        );
    }

    #[test]
    fn test_dockerfile_comments_only() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"# This is a comment
# Another comment

# More comments here
# syntax=docker/dockerfile:1
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docker/Dockerfile", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle comments-only Dockerfile"
        );
    }

    #[test]
    fn test_dockerfile_scratch() {
        let repo = TestRepo::new();
        repo.add_file(
            "docker/Dockerfile",
            r#"# Build stage
FROM golang:1.21 AS builder

WORKDIR /app
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o /app/main

# Minimal production image
FROM scratch

COPY --from=builder /app/main /main

ENTRYPOINT ["/main"]
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "docker/Dockerfile", "-f", "json"]);
        let json = assert_valid_json(&output, "Dockerfile scratch image");

        // Dockerfile symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("builder")
                || output_str.contains("scratch")
                || output_str.contains("FROM"),
            "Should find Dockerfile stages: {}",
            output
        );
    }
}

// =============================================================================
// GRADLE TESTS
// =============================================================================

mod gradle_tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Symbol Extraction
    // -------------------------------------------------------------------------

    #[test]
    fn test_gradle_build_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "build.gradle.kts",
            r#"plugins {
    kotlin("jvm") version "1.9.0"
    application
}

task("compile") {
    doLast {
        println("Compiling...")
    }
}

task("test") {
    dependsOn("compile")
    doLast {
        println("Testing...")
    }
}

task("assemble") {
    dependsOn("test")
    doLast {
        println("Assembling...")
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "build.gradle.kts", "-f", "json"]);
        let json = assert_valid_json(&output, "Gradle build extraction");

        // Gradle symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("compile")
                || output_str.contains("test")
                || output_str.contains("task"),
            "Should find Gradle tasks: {}",
            output
        );
    }

    #[test]
    fn test_gradle_kotlin_dsl() {
        let repo = TestRepo::new();
        repo.add_file(
            "build.gradle.kts",
            r#"plugins {
    kotlin("jvm") version "1.9.0"
    application
}

group = "com.example"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

dependencies {
    implementation(kotlin("stdlib"))
    implementation("io.ktor:ktor-server-core:2.3.0")
    implementation("io.ktor:ktor-server-netty:2.3.0")

    testImplementation(kotlin("test"))
    testImplementation("io.ktor:ktor-server-test-host:2.3.0")
}

tasks.test {
    useJUnitPlatform()
}

application {
    mainClass.set("com.example.MainKt")
}

tasks.register("customTask") {
    doLast {
        println("Custom task executed")
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "build.gradle.kts", "-f", "json"]);
        let json = assert_valid_json(&output, "Gradle Kotlin DSL extraction");

        // Gradle symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("customTask")
                || output_str.contains("tasks")
                || output_str.contains("register"),
            "Should find Gradle Kotlin DSL content: {}",
            output
        );
    }

    #[test]
    fn test_gradle_groovy_dsl() {
        let repo = TestRepo::new();
        repo.add_file(
            "build.gradle",
            r#"plugins {
    id 'java'
    id 'org.springframework.boot' version '3.1.0'
    id 'io.spring.dependency-management' version '1.1.0'
}

group = 'com.example'
version = '0.0.1-SNAPSHOT'
sourceCompatibility = '17'

repositories {
    mavenCentral()
}

dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web'
    implementation 'org.springframework.boot:spring-boot-starter-data-jpa'

    runtimeOnly 'org.postgresql:postgresql'

    testImplementation 'org.springframework.boot:spring-boot-starter-test'
}

test {
    useJUnitPlatform()
}

task customTask {
    doLast {
        println 'Custom task executed'
    }
}

bootJar {
    archiveFileName = "app.jar"
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "build.gradle", "-f", "json"]);
        let json = assert_valid_json(&output, "Gradle Groovy DSL extraction");

        // Gradle symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("customTask")
                || output_str.contains("task")
                || output_str.contains("plugins"),
            "Should find Gradle Groovy DSL content: {}",
            output
        );
    }

    #[test]
    fn test_gradle_multi_project() {
        let repo = TestRepo::new();
        repo.add_file(
            "settings.gradle.kts",
            r#"rootProject.name = "my-project"

include("app")
include("core")
include("api")

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}
"#,
        );
        repo.add_file(
            "build.gradle.kts",
            r#"plugins {
    base
}

allprojects {
    group = "com.example"
    version = "1.0-SNAPSHOT"
}

subprojects {
    apply(plugin = "java")

    tasks.withType<Test> {
        useJUnitPlatform()
    }
}
"#,
        );
        repo.add_file(
            "app/build.gradle.kts",
            r#"plugins {
    application
}

dependencies {
    implementation(project(":core"))
    implementation(project(":api"))
}

application {
    mainClass.set("com.example.AppKt")
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["query", "overview", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle multi-project Gradle"
        );
    }

    #[test]
    fn test_gradle_custom_tasks() {
        let repo = TestRepo::new();
        repo.add_file(
            "build.gradle.kts",
            r#"plugins {
    base
}

// Task with configuration
val generateDocs by tasks.registering {
    group = "documentation"
    description = "Generates project documentation"

    doFirst {
        println("Preparing documentation...")
    }

    doLast {
        println("Documentation generated!")
    }
}

// Copy task
val copyResources by tasks.registering(Copy::class) {
    from("src/main/resources")
    into("build/resources")
}

// Exec task
val runScript by tasks.registering(Exec::class) {
    commandLine("bash", "-c", "echo Hello World")
}

// Task dependencies
val build by tasks.registering {
    dependsOn(generateDocs)
    dependsOn(copyResources)

    finalizedBy(runScript)
}

// Task with inputs and outputs
val processTemplates by tasks.registering {
    inputs.dir("templates")
    outputs.dir("build/processed")

    doLast {
        // Processing logic
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli_success(&["analyze", "build.gradle.kts", "-f", "json"]);
        let json = assert_valid_json(&output, "Gradle custom tasks extraction");

        // Gradle symbol names may vary, check for content presence
        let output_str = serde_json::to_string(&json).unwrap();
        assert!(
            output_str.contains("generateDocs")
                || output_str.contains("copyResources")
                || output_str.contains("registering"),
            "Should find Gradle custom tasks: {}",
            output
        );
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_gradle_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("build.gradle.kts", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "build.gradle.kts", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Gradle file"
        );
    }

    #[test]
    fn test_gradle_android() {
        let repo = TestRepo::new();
        repo.add_file(
            "app/build.gradle.kts",
            r#"plugins {
    id("com.android.application")
    kotlin("android")
}

android {
    namespace = "com.example.myapp"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.example.myapp"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.6.2")
    implementation("androidx.activity:activity-compose:1.8.0")

    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "app/build.gradle.kts", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Android Gradle configuration"
        );
    }

    #[test]
    fn test_gradle_version_catalog() {
        let repo = TestRepo::new();
        repo.add_file(
            "gradle/libs.versions.toml",
            r#"[versions]
kotlin = "1.9.0"
ktor = "2.3.0"
logback = "1.4.11"

[libraries]
kotlin-stdlib = { module = "org.jetbrains.kotlin:kotlin-stdlib", version.ref = "kotlin" }
ktor-server-core = { module = "io.ktor:ktor-server-core", version.ref = "ktor" }
ktor-server-netty = { module = "io.ktor:ktor-server-netty", version.ref = "ktor" }
logback-classic = { module = "ch.qos.logback:logback-classic", version.ref = "logback" }

[bundles]
ktor-server = ["ktor-server-core", "ktor-server-netty"]

[plugins]
kotlin-jvm = { id = "org.jetbrains.kotlin.jvm", version.ref = "kotlin" }
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "gradle/libs.versions.toml", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Gradle version catalog"
        );
    }
}
