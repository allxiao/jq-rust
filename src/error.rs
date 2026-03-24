//! Error types for jq-rust

use std::fmt;
use std::sync::Arc;
use thiserror::Error;

use crate::parser::token::Span;

/// Source information for error formatting.
/// This is shared across errors to avoid copying the source string.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// The complete source string
    pub source: Arc<String>,
    /// Filename or "<top-level>"
    pub filename: String,
}

impl SourceInfo {
    /// Create new source info
    pub fn new(source: String, filename: String) -> Self {
        SourceInfo {
            source: Arc::new(source),
            filename,
        }
    }

    /// Create source info for top-level filter
    pub fn top_level(source: String) -> Self {
        Self::new(source, "<top-level>".to_string())
    }

    /// Compute line number (1-based) from byte position
    pub fn line_number(&self, pos: usize) -> usize {
        let pos = pos.min(self.source.len());
        self.source[..pos].chars().filter(|&c| c == '\n').count() + 1
    }

    /// Compute column number (1-based) from byte position
    pub fn column_number(&self, pos: usize) -> usize {
        let pos = pos.min(self.source.len());
        let line_start = self.source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        pos - line_start + 1
    }

    /// Get the source line containing the position
    pub fn source_line(&self, pos: usize) -> &str {
        let pos = pos.min(self.source.len());
        let line_start = self.source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = self.source[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(self.source.len());
        &self.source[line_start..line_end]
    }

    /// Generate caret pointer line showing error position
    pub fn caret_line(&self, span: Span) -> String {
        let col = self.column_number(span.start);
        let width = span.end.saturating_sub(span.start).max(1);
        format!("{}{}", " ".repeat(col - 1), "^".repeat(width))
    }

    /// Format a complete error message with context.
    /// Returns a multi-line string like:
    /// ```text
    /// <message> at <filename>, line <line>, column <col>:
    ///     <source line>
    ///     <caret pointer>
    /// ```
    pub fn format_error(&self, message: &str, span: Span) -> String {
        format!(
            "{} at {}, line {}, column {}:\n    {}\n    {}",
            message,
            self.filename,
            self.line_number(span.start),
            self.column_number(span.start),
            self.source_line(span.start),
            self.caret_line(span)
        )
    }
}

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

/// A runtime error with optional source location information.
/// Used during interpretation to carry span information until formatting.
#[derive(Debug, Clone)]
pub struct RuntimeError {
    /// The error message
    pub message: String,
    /// Optional span indicating where in the source the error occurred
    pub span: Option<Span>,
}

impl RuntimeError {
    /// Create a new runtime error without location info
    pub fn new(message: impl Into<String>) -> Self {
        RuntimeError {
            message: message.into(),
            span: None,
        }
    }

    /// Create a new runtime error with location info
    pub fn with_span(message: impl Into<String>, span: Span) -> Self {
        RuntimeError {
            message: message.into(),
            span: Some(span),
        }
    }

    /// Format the error, optionally with source context
    pub fn format(&self, source_info: Option<&SourceInfo>) -> String {
        match (self.span, source_info) {
            (Some(span), Some(info)) => info.format_error(&self.message, span),
            _ => self.message.clone(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Without source context, just show the message
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}

impl From<String> for RuntimeError {
    fn from(s: String) -> Self {
        RuntimeError::new(s)
    }
}

impl From<&str> for RuntimeError {
    fn from(s: &str) -> Self {
        RuntimeError::new(s)
    }
}
