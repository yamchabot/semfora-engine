//! Error types and exit codes for semfora-engine

use std::process::ExitCode;
use thiserror::Error;

/// Main error type for semfora-engine operations
#[derive(Error, Debug)]
pub enum McpDiffError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Unsupported language for extension: {extension}")]
    UnsupportedLanguage { extension: String },

    #[error("Failed to parse file: {message}")]
    ParseFailure { message: String },

    #[error("Semantic extraction failed: {message}")]
    ExtractionFailure { message: String },

    #[error("Query error: {message}")]
    QueryError { message: String },

    #[error("Git error: {message}")]
    GitError { message: String },

    #[error("Not a git repository")]
    NotGitRepo,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl McpDiffError {
    /// Convert error to appropriate exit code per spec:
    /// - 0: Success
    /// - 1: File not found / IO error
    /// - 2: Unsupported language
    /// - 3: Parse failure
    /// - 4: Internal semantic extraction failure
    /// - 5: Git error
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::FileNotFound { .. } => ExitCode::from(1),
            Self::UnsupportedLanguage { .. } => ExitCode::from(2),
            Self::ParseFailure { .. } => ExitCode::from(3),
            Self::ExtractionFailure { .. } => ExitCode::from(4),
            Self::QueryError { .. } => ExitCode::from(4),
            Self::GitError { .. } => ExitCode::from(5),
            Self::NotGitRepo => ExitCode::from(5),
            Self::Io(_) => ExitCode::from(1),
        }
    }
}

/// Result type alias for semfora-engine operations
pub type Result<T> = std::result::Result<T, McpDiffError>;
