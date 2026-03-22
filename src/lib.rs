//! jq-rust: A Rust implementation of jq
//!
//! jq is a lightweight and flexible command-line JSON processor.
//! This crate provides a Rust implementation of jq's functionality.

pub mod jv;
pub mod error;
pub mod parser;
pub mod vm;
pub mod builtins;
pub mod testing;
pub mod module;
pub mod regex_helper;
pub mod intern;

pub use error::{JqError, Result};
pub use jv::Jv;
pub use parser::{parse, parse_program_full, Expr, ExprKind};
pub use vm::{interpret, Interpreter, Context};
pub use module::{ModuleLoader, set_module_search_path, get_module_search_path};
