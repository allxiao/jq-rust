//! jq-rust: A Rust implementation of jq
//!
//! jq is a lightweight and flexible command-line JSON processor.
//! This crate provides a Rust implementation of jq's functionality.

pub mod jv;
pub mod error;
pub mod parser;

// Future modules (commented until implemented):
// pub mod compiler;
// pub mod vm;
// pub mod builtins;

pub use error::{JqError, Result};
pub use jv::Jv;
pub use parser::{parse, Expr, ExprKind};
