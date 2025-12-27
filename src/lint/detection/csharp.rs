//! C#/.NET linter detection (dotnet format, Roslyn analyzers, StyleCop).

use std::path::Path;

use crate::lint::types::{DetectedLinter, LintCapabilities, LintCommand, Linter};
use crate::lint::version::{
    get_dotnet_format_version, get_roslyn_version, get_stylecop_version, has_dotnet_project,
    has_nuget_package, is_command_available,
};

/// Check if directory has .editorconfig with C# formatting rules
fn has_editorconfig_csharp(dir: &Path) -> bool {
    let editorconfig = dir.join(".editorconfig");
    if !editorconfig.exists() {
        return false;
    }

    std::fs::read_to_string(editorconfig)
        .map(|content| {
            // Check for C# specific sections or dotnet naming conventions
            content.contains("[*.cs]")
                || content.contains("[*.{cs,vb}]")
                || content.contains("dotnet_")
                || content.contains("csharp_")
        })
        .unwrap_or(false)
}

/// Check if Directory.Build.props enables specific analyzers
fn has_analyzers_in_directory_build_props(dir: &Path, analyzer_id: &str) -> bool {
    let props_file = dir.join("Directory.Build.props");
    if !props_file.exists() {
        return false;
    }

    std::fs::read_to_string(props_file)
        .map(|content| content.contains(analyzer_id))
        .unwrap_or(false)
}

/// Detect C#/.NET linters (dotnet format, Roslyn analyzers, StyleCop)
pub fn detect_csharp_linters(dir: &Path) -> Vec<DetectedLinter> {
    let mut linters = Vec::new();

    // Check if dotnet is available
    let dotnet_available = is_command_available("dotnet");

    // dotnet format - built into .NET SDK 6+
    // Detectable via: .editorconfig with C# rules, or any .NET project
    let has_editorconfig = has_editorconfig_csharp(dir);
    let has_project = has_dotnet_project(dir);

    if dotnet_available && (has_editorconfig || has_project) {
        linters.push(DetectedLinter {
            linter: Linter::DotnetFormat,
            config_path: if has_editorconfig {
                Some(dir.join(".editorconfig"))
            } else {
                None
            },
            version: get_dotnet_format_version(),
            available: true,
            run_command: LintCommand {
                program: "dotnet".to_string(),
                args: vec![
                    "format".to_string(),
                    "--verify-no-changes".to_string(),
                    "--verbosity".to_string(),
                    "diagnostic".to_string(),
                ],
                fix_args: Some(vec!["format".to_string()]),
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: true,
                can_format: true,
                can_typecheck: false,
            },
        });
    }

    // Roslyn Analyzers - enabled via <EnableNETAnalyzers>true</EnableNETAnalyzers>
    // or automatically in .NET 5+ SDK-style projects
    // We detect via dotnet build with /warnaserror
    if dotnet_available && has_project {
        // Check for explicit analyzer configuration
        let has_analyzers_config =
            has_analyzers_in_directory_build_props(dir, "EnableNETAnalyzers")
                || has_analyzers_in_directory_build_props(dir, "AnalysisLevel")
                || has_analyzers_in_directory_build_props(dir, "AnalysisMode");

        // Roslyn analyzers are enabled by default in .NET 5+, so always add if project exists
        linters.push(DetectedLinter {
            linter: Linter::RoslynAnalyzers,
            config_path: if dir.join("Directory.Build.props").exists() {
                Some(dir.join("Directory.Build.props"))
            } else if dir.join(".editorconfig").exists() {
                Some(dir.join(".editorconfig"))
            } else {
                None
            },
            version: get_roslyn_version(),
            available: true,
            run_command: LintCommand {
                program: "dotnet".to_string(),
                args: vec![
                    "build".to_string(),
                    "--no-restore".to_string(),
                    "-warnaserror".to_string(),
                    "/p:TreatWarningsAsErrors=true".to_string(),
                ],
                fix_args: None, // Roslyn can suggest fixes but doesn't auto-apply
                cwd: None,
            },
            capabilities: LintCapabilities {
                can_fix: false,
                can_format: false,
                can_typecheck: true,
            },
        });

        // Check if analyzers are explicitly configured for better messaging
        if has_analyzers_config {
            // Already added above, just note the explicit config
        }
    }

    // StyleCop.Analyzers - NuGet package
    let has_stylecop = has_nuget_package(dir, "StyleCop.Analyzers")
        || has_analyzers_in_directory_build_props(dir, "StyleCop.Analyzers")
        || dir.join("stylecop.json").exists();

    if dotnet_available && has_stylecop {
        linters.push(DetectedLinter {
            linter: Linter::StyleCop,
            config_path: if dir.join("stylecop.json").exists() {
                Some(dir.join("stylecop.json"))
            } else {
                None
            },
            version: get_stylecop_version(),
            available: true,
            run_command: LintCommand {
                program: "dotnet".to_string(),
                args: vec![
                    "build".to_string(),
                    "--no-restore".to_string(),
                    "-warnaserror".to_string(),
                ],
                fix_args: None, // StyleCop reports issues but doesn't auto-fix
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
