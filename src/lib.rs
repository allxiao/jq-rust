//! jq-rust: A Rust implementation of jq
//!
//! jq is a lightweight and flexible command-line JSON processor.
//! This crate provides a Rust implementation of jq's functionality.

pub mod builtins;
pub mod error;
pub mod intern;
pub mod jv;
pub mod module;
pub mod parser;
pub mod regex_helper;
pub mod testing;
pub mod vm;

pub use error::{JqError, Result, RuntimeError, SourceInfo};
pub use jv::Jv;
pub use module::{get_module_search_path, set_module_search_path, ModuleLoader};
pub use parser::{parse, parse_program_full, Expr, ExprKind, Span};
pub use vm::{interpret, interpret_with_source, Context, Interpreter};
