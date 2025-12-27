//! Version detection for linting tools.
//!
//! This module provides functions to detect the installed version of each linter.

use std::process::Command;

/// Check if a command is available in PATH
pub fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get clippy version
pub fn get_clippy_version() -> Option<String> {
    Command::new("cargo")
        .args(["clippy", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        })
}

/// Get rustfmt version
pub fn get_rustfmt_version() -> Option<String> {
    Command::new("rustfmt")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        })
}

/// Get ESLint version
pub fn get_eslint_version() -> Option<String> {
    Command::new("npx")
        .args(["eslint", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Prettier version
pub fn get_prettier_version() -> Option<String> {
    Command::new("npx")
        .args(["prettier", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Biome version
pub fn get_biome_version() -> Option<String> {
    Command::new("npx")
        .args(["biome", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok().map(|s| {
                    // Biome outputs "Version: X.Y.Z", strip the prefix
                    s.trim()
                        .strip_prefix("Version: ")
                        .or_else(|| s.trim().strip_prefix("Version "))
                        .unwrap_or(s.trim())
                        .to_string()
                })
            } else {
                None
            }
        })
}

/// Get TypeScript version
pub fn get_tsc_version() -> Option<String> {
    Command::new("npx")
        .args(["tsc", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok().map(|s| {
                    // TSC outputs "Version X.Y.Z", strip the prefix
                    s.trim()
                        .strip_prefix("Version ")
                        .unwrap_or(s.trim())
                        .to_string()
                })
            } else {
                None
            }
        })
}

/// Get Ruff version
pub fn get_ruff_version() -> Option<String> {
    Command::new("ruff")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Black version
pub fn get_black_version() -> Option<String> {
    Command::new("black")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.lines().next().unwrap_or("").trim().to_string())
            } else {
                None
            }
        })
}

/// Get mypy version
pub fn get_mypy_version() -> Option<String> {
    Command::new("mypy")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get golangci-lint version
pub fn get_golangci_version() -> Option<String> {
    Command::new("golangci-lint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            } else {
                None
            }
        })
}

/// Get Go version (for gofmt and go vet)
pub fn get_go_version() -> Option<String> {
    Command::new("go")
        .arg("version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Checkstyle version
pub fn get_checkstyle_version() -> Option<String> {
    Command::new("checkstyle")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            // Checkstyle outputs "Checkstyle version: X.Y.Z"
            String::from_utf8(out.stdout)
                .ok()
                .or_else(|| String::from_utf8(out.stderr).ok())
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.contains("version"))
                        .map(|l| l.trim().to_string())
                })
        })
}

/// Get SpotBugs version
pub fn get_spotbugs_version() -> Option<String> {
    Command::new("spotbugs")
        .arg("-version")
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        })
}

/// Get PMD version
pub fn get_pmd_version() -> Option<String> {
    Command::new("pmd")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            // PMD outputs "PMD X.Y.Z"
            String::from_utf8(out.stdout)
                .ok()
                .or_else(|| String::from_utf8(out.stderr).ok())
                .map(|s| s.trim().to_string())
        })
}

/// Get detekt version
pub fn get_detekt_version() -> Option<String> {
    Command::new("detekt")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get ktlint version
pub fn get_ktlint_version() -> Option<String> {
    Command::new("ktlint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get clang-tidy version
pub fn get_clang_tidy_version() -> Option<String> {
    Command::new("clang-tidy")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok().and_then(|s| {
                    // clang-tidy outputs multiple lines, extract version from first line
                    // Format: "LLVM (http://llvm.org/):\n  LLVM version X.Y.Z\n..."
                    // or: "clang-tidy version X.Y.Z"
                    s.lines()
                        .find(|l| l.contains("version"))
                        .map(|l| l.trim().to_string())
                })
            } else {
                None
            }
        })
}

/// Get cppcheck version
pub fn get_cppcheck_version() -> Option<String> {
    Command::new("cppcheck")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // cppcheck outputs "Cppcheck X.Y.Z" or "Cppcheck X.Y"
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get cpplint version
pub fn get_cpplint_version() -> Option<String> {
    Command::new("cpplint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            // cpplint outputs version to stderr
            String::from_utf8(out.stderr)
                .ok()
                .or_else(|| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_string())
        })
}

/// Get Oxlint version (fast JS/TS linter written in Rust)
pub fn get_oxlint_version() -> Option<String> {
    Command::new("npx")
        .args(["oxlint", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // oxlint outputs just the version number
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Pylint version
pub fn get_pylint_version() -> Option<String> {
    Command::new("pylint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // pylint outputs "pylint X.Y.Z" and more info
                String::from_utf8(out.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            } else {
                None
            }
        })
}

/// Check if pyproject.toml has a specific section
pub fn has_pyproject_section(dir: &std::path::Path, section: &str) -> bool {
    let pyproject = dir.join("pyproject.toml");
    if !pyproject.exists() {
        return false;
    }

    std::fs::read_to_string(pyproject)
        .map(|content| content.contains(&format!("[{}]", section)))
        .unwrap_or(false)
}

/// Get .NET SDK version (for dotnet format)
pub fn get_dotnet_version() -> Option<String> {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get dotnet format version (uses SDK version)
pub fn get_dotnet_format_version() -> Option<String> {
    // dotnet format is built into the SDK since .NET 6
    // Return the SDK version
    get_dotnet_version()
}

/// Get Roslyn analyzers version from project
/// Note: Roslyn analyzers are built into the SDK, version matches SDK
pub fn get_roslyn_version() -> Option<String> {
    // Roslyn is built into the SDK
    get_dotnet_version()
}

/// Get StyleCop version from project file
/// StyleCop.Analyzers is a NuGet package, so we check for its presence
pub fn get_stylecop_version() -> Option<String> {
    // StyleCop is a NuGet package, version comes from .csproj
    // Return None as version is project-specific
    None
}

/// Check if a .csproj file contains a specific package reference
pub fn has_nuget_package(dir: &std::path::Path, package_name: &str) -> bool {
    // Check all .csproj files in the directory
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "csproj") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Check for PackageReference with the package name
                    if content.contains(&format!("Include=\"{}\"", package_name))
                        || content.contains(&format!("Include='{}'", package_name))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if directory contains .NET project files
pub fn has_dotnet_project(dir: &std::path::Path) -> bool {
    // Check for .sln, .csproj, or .fsproj files
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "sln" || ext == "csproj" || ext == "fsproj" || ext == "vbproj" {
                    return true;
                }
            }
        }
    }
    false
}

/// Get HTMLHint version
pub fn get_htmlhint_version() -> Option<String> {
    Command::new("npx")
        .args(["htmlhint", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get html-validate version
pub fn get_html_validate_version() -> Option<String> {
    Command::new("npx")
        .args(["html-validate", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get Stylelint version
pub fn get_stylelint_version() -> Option<String> {
    Command::new("npx")
        .args(["stylelint", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Check if directory contains HTML files
pub fn has_html_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "html" || ext == "htm" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains CSS/SCSS/SASS files
pub fn has_css_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "css" || ext == "scss" || ext == "sass" || ext == "less" {
                    return true;
                }
            }
        }
    }
    false
}

/// Get jsonlint version (via npx)
pub fn get_jsonlint_version() -> Option<String> {
    Command::new("npx")
        .args(["jsonlint", "--version"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get yamllint version
pub fn get_yamllint_version() -> Option<String> {
    Command::new("yamllint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // yamllint outputs "yamllint X.Y.Z"
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get taplo version (TOML toolkit)
pub fn get_taplo_version() -> Option<String> {
    Command::new("taplo")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // taplo outputs "taplo X.Y.Z"
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get xmllint version
pub fn get_xmllint_version() -> Option<String> {
    Command::new("xmllint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            // xmllint outputs version info to stderr
            String::from_utf8(out.stderr)
                .ok()
                .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        })
}

/// Check if directory contains JSON files
pub fn has_json_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "json" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains YAML files
pub fn has_yaml_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "yaml" || ext == "yml" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains TOML files
pub fn has_toml_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "toml" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains XML files
pub fn has_xml_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "xml" {
                    return true;
                }
            }
        }
    }
    false
}

// ============================================================================
// Infrastructure Linters (Terraform, Shell)
// ============================================================================

/// Get TFLint version
pub fn get_tflint_version() -> Option<String> {
    Command::new("tflint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // tflint outputs "TFLint version X.Y.Z"
                String::from_utf8(out.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            } else {
                None
            }
        })
}

/// Get Terraform version (for terraform validate and terraform fmt)
pub fn get_terraform_version() -> Option<String> {
    Command::new("terraform")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // terraform outputs "Terraform vX.Y.Z"
                String::from_utf8(out.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            } else {
                None
            }
        })
}

/// Get ShellCheck version
pub fn get_shellcheck_version() -> Option<String> {
    Command::new("shellcheck")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // shellcheck outputs multiple lines, find the version line
                String::from_utf8(out.stdout).ok().and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("version:"))
                        .map(|l| l.trim().to_string())
                })
            } else {
                None
            }
        })
}

/// Get shfmt version
pub fn get_shfmt_version() -> Option<String> {
    Command::new("shfmt")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // shfmt outputs just the version like "v3.7.0"
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Check if directory contains Terraform files
pub fn has_terraform_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "tf" || ext == "tfvars" {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if directory contains shell script files
pub fn has_shell_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "sh" || ext == "bash" || ext == "zsh" || ext == "ksh" {
                    return true;
                }
            }
        }
    }
    false
}

// ============================================================================
// Documentation Linters (Markdown)
// ============================================================================

/// Get markdownlint version (CLI or CLI2)
pub fn get_markdownlint_version() -> Option<String> {
    // Try markdownlint-cli2 first (newer)
    Command::new("npx")
        .args(["markdownlint-cli2", "--help"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                // markdownlint-cli2 outputs help text, extract version from first line
                String::from_utf8(out.stdout).ok().and_then(|s| {
                    s.lines()
                        .next()
                        .map(|l| l.trim().to_string())
                        .or(Some("markdownlint-cli2".to_string()))
                })
            } else {
                None
            }
        })
        .or_else(|| {
            // Fall back to markdownlint (older CLI)
            Command::new("npx")
                .args(["markdownlint", "--version"])
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        String::from_utf8(out.stdout)
                            .ok()
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
        })
}

/// Check if directory contains Markdown files
pub fn has_markdown_files(dir: &std::path::Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "md" || ext == "markdown" {
                    return true;
                }
            }
        }
    }
    false
}
