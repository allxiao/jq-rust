//! jq-rust: A Rust implementation of jq
//!
//! jq is a lightweight and flexible command-line JSON processor.
//! This crate provides a Rust implementation of jq's functionality.

pub mod jv;
pub mod error;

// Future modules (commented until implemented):
// pub mod parser;
// pub mod compiler;
// pub mod vm;
// pub mod builtins;
// pub mod io;

pub use error::{JqError, Result};
pub use jv::Jv;
