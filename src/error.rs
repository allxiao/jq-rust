//! Error types for jq-rust

use thiserror::Error;

/// Main error type for jq operations
#[derive(Error, Debug, Clone)]
pub enum JqError {
    /// JSON parsing error
    #[error("parse error: {0}")]
    Parse(String),

    /// Type error during execution
    #[error("type error: {0}")]
    Type(String),

    /// Index out of bounds
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(String),

    /// Key not found in object
    #[error("key not found: {0}")]
    KeyNotFound(String),

    /// Division by zero
    #[error("division by zero")]
    DivisionByZero,

    /// Invalid path expression
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// Compilation error
    #[error("compile error: {0}")]
    Compile(String),

    /// Runtime error
    #[error("runtime error: {0}")]
    Runtime(String),

    /// Module/import error
    #[error("module error: {0}")]
    Module(String),

    /// Generic error with message
    #[error("{0}")]
    Custom(String),
}

impl JqError {
    pub fn type_error(expected: &str, got: &str) -> Self {
        JqError::Type(format!("expected {}, got {}", expected, got))
    }
}

/// Result type alias for jq operations
pub type Result<T> = std::result::Result<T, JqError>;
