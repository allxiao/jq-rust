//! Virtual Machine / Interpreter for jq expressions
//!
//! This module implements execution of jq filter expressions.
//! It uses an AST-walking interpreter approach for simplicity.

mod context;
mod interpreter;

pub use context::Context;
pub use interpreter::{interpret, interpret_with_source, Interpreter};
