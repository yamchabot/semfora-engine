//! C/C++ linter detection (clang-tidy, cppcheck, cpplint).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{
    get_clang_tidy_version, get_cppcheck_version, get_cpplint_version, is_command_available,
};

/// Check if a directory contains C/C++ source files
pub fn has_cpp_sources(dir: &Path) -> bool {
    // Check common C/C++ source directories
    let cpp_dirs = ["src", "source", "lib", "include"];
    let cpp_extensions = ["c", "cc", "cpp", "cxx", "h", "hpp", "hxx"];

    for cpp_dir in cpp_dirs {
        let path = dir.join(cpp_dir);
        if path.exists() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    if entry.path().extension().map_or(false, |ext| {
                        cpp_extensions.contains(&ext.to_str().unwrap_or(""))
                    }) {
                        return true;
                    }
                }
            }
        }
    }

    // Check root directory for C/C++ files
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |ext| {
                cpp_extensions.contains(&ext.to_str().unwrap_or(""))
            }) {
                return true;
            }
        }
    }

    false
}

/// Check if a directory has a C/C++ build system
pub fn has_cpp_build_system(dir: &Path) -> bool {
    // CMake
    if dir.join("CMakeLists.txt").exists() {
        return true;
    }

    // Meson
    if dir.join("meson.build").exists() {
        return true;
    }

    // Makefile
    if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        return true;
    }

    // compile_commands.json (common for clang tools)
    if dir.join("compile_commands.json").exists()
        || dir.join("build/compile_commands.json").exists()
    {
        return true;
    }

    // Bazel (used for C++ projects)
    if dir.join("WORKSPACE").exists() || dir.join("BUILD").exists() {
        return true;
    }

    false
}

/// Detect C/C++ linters (clang-tidy, cppcheck, cpplint)
pub fn detect_cpp_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Check for compile_commands.json (needed for accurate clang-tidy analysis)
    let has_compile_commands = dir.join("compile_commands.json").exists()
        || dir.join("build/compile_commands.json").exists();

    // clang-tidy - static analyzer based on Clang
    let has_clang_tidy_config = dir.join(".clang-tidy").exists();

    if has_clang_tidy_config || is_command_available("clang-tidy") {
        let mut args = vec![];

        // If compile_commands.json exists, use it
        if dir.join("compile_commands.json").exists() {
            args.extend(vec!["-p".to_string(), ".".to_string()]);
        } else if dir.join("build/compile_commands.json").exists() {
            args.extend(vec!["-p".to_string(), "build".to_string()]);
        }

        // Add source patterns
        args.push("src/*.cpp".to_string());

        linters.push(DetectedLinter {
            linter: Linter::ClangTidy,
            config_path: if dir.join(".clang-tidy").exists() {
                Some(dir.join(".clang-tidy"))
            } else {
                None
            },
            version: get_clang_tidy_version(),
            available: is_command_available("clang-tidy"),
            run_command: LintCommand {
                program: "clang-tidy".to_string(),
                args,
                fix_args: Some(vec!["--fix".to_string(), "--fix-errors".to_string()]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // cppcheck - static analysis for C/C++
    let has_cppcheck_config = dir.join(".cppcheck").exists() || dir.join("cppcheck.cfg").exists();

    if has_cppcheck_config || has_cpp_sources(dir) || is_command_available("cppcheck") {
        let mut args = vec![
            "--enable=all".to_string(),
            "--template=gcc".to_string(),
            "--quiet".to_string(),
        ];

        // If compile_commands.json exists, use it for better accuracy
        if has_compile_commands {
            if dir.join("compile_commands.json").exists() {
                args.push("--project=compile_commands.json".to_string());
            } else {
                args.push("--project=build/compile_commands.json".to_string());
            }
        } else {
            args.push(".".to_string());
        }

        linters.push(DetectedLinter {
            linter: Linter::Cppcheck,
            config_path: if dir.join(".cppcheck").exists() {
                Some(dir.join(".cppcheck"))
            } else if dir.join("cppcheck.cfg").exists() {
                Some(dir.join("cppcheck.cfg"))
            } else {
                None
            },
            version: get_cppcheck_version(),
            available: is_command_available("cppcheck"),
            run_command: LintCommand {
                program: "cppcheck".to_string(),
                args,
                fix_args: None, // cppcheck doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    // cpplint - Google C++ style checker
    let has_cpplint_config = dir.join("CPPLINT.cfg").exists() || dir.join(".cpplint").exists();

    if has_cpplint_config || is_command_available("cpplint") {
        linters.push(DetectedLinter {
            linter: Linter::Cpplint,
            config_path: if dir.join("CPPLINT.cfg").exists() {
                Some(dir.join("CPPLINT.cfg"))
            } else if dir.join(".cpplint").exists() {
                Some(dir.join(".cpplint"))
            } else {
                None
            },
            version: get_cpplint_version(),
            available: is_command_available("cpplint"),
            run_command: LintCommand {
                program: "cpplint".to_string(),
                args: vec![
                    "--recursive".to_string(),
                    "--output=eclipse".to_string(),
                    ".".to_string(),
                ],
                fix_args: None, // cpplint doesn't auto-fix
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: false,
            },
        });
    }

    linters
}
