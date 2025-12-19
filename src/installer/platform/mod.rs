//! Platform detection and path resolution for cross-platform installer support.
//!
//! Supports macOS (ARM64/x86_64), Linux (x86_64/ARM64), and Windows.

mod paths;

pub use paths::*;

use std::env;

/// Detected platform architecture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Arm64,
}

impl Arch {
    /// Detect the current architecture
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Arch::X86_64
        }
        #[cfg(target_arch = "aarch64")]
        {
            Arch::Arm64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // Default to x86_64 for unknown architectures
            Arch::X86_64
        }
    }

    /// Get the architecture name used in binary downloads
    pub fn download_name(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Arm64 => "aarch64",
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::Arm64 => write!(f, "arm64"),
        }
    }
}

/// Detected operating system platform
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    MacOS { arch: Arch },
    Linux { arch: Arch },
    Windows,
}

impl Platform {
    /// Detect the current platform
    #[allow(unused_variables)] // arch is unused on Windows
    pub fn detect() -> Self {
        let arch = Arch::detect();

        #[cfg(target_os = "macos")]
        {
            Platform::MacOS { arch }
        }
        #[cfg(target_os = "linux")]
        {
            Platform::Linux { arch }
        }
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            // Default to Linux for unknown platforms
            Platform::Linux { arch }
        }
    }

    /// Get the platform name used in binary downloads
    pub fn download_name(&self) -> &'static str {
        match self {
            Platform::MacOS { .. } => "darwin",
            Platform::Linux { .. } => "linux",
            Platform::Windows => "windows",
        }
    }

    /// Get the binary extension for this platform
    pub fn binary_extension(&self) -> &'static str {
        match self {
            Platform::Windows => ".exe",
            _ => "",
        }
    }

    /// Check if this is a Unix-like platform
    pub fn is_unix(&self) -> bool {
        matches!(self, Platform::MacOS { .. } | Platform::Linux { .. })
    }

    /// Get the architecture if available
    pub fn arch(&self) -> Option<Arch> {
        match self {
            Platform::MacOS { arch } | Platform::Linux { arch } => Some(*arch),
            Platform::Windows => Some(Arch::detect()),
        }
    }

    /// Get the home directory for this platform
    pub fn home_dir(&self) -> Option<std::path::PathBuf> {
        dirs::home_dir()
    }

    /// Get the config directory for this platform
    pub fn config_dir(&self) -> Option<std::path::PathBuf> {
        dirs::config_dir()
    }

    /// Get the data directory for this platform
    pub fn data_dir(&self) -> Option<std::path::PathBuf> {
        dirs::data_dir()
    }

    /// Get the cache directory for this platform
    pub fn cache_dir(&self) -> Option<std::path::PathBuf> {
        // Respect XDG_CACHE_HOME on Unix
        if let Ok(xdg_cache) = env::var("XDG_CACHE_HOME") {
            return Some(std::path::PathBuf::from(xdg_cache));
        }
        dirs::cache_dir()
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::MacOS { arch } => write!(f, "macOS ({})", arch),
            Platform::Linux { arch } => write!(f, "Linux ({})", arch),
            Platform::Windows => write!(f, "Windows"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::detect();
        // Just ensure it doesn't panic
        println!("Detected platform: {}", platform);
    }

    #[test]
    fn test_arch_detection() {
        let arch = Arch::detect();
        println!("Detected arch: {}", arch);
    }
}
