//! Virtual Machine / Interpreter for jq expressions
//!
//! This module implements execution of jq filter expressions.
//! It uses an AST-walking interpreter approach for simplicity.

mod interpreter;
mod context;

pub use interpreter::{Interpreter, interpret};
pub use context::Context;
