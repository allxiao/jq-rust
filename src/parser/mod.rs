//! Parser module for jq filter language
//!
//! This module implements parsing of jq filter expressions.

#![allow(clippy::module_inception)]

mod ast;
mod lexer;
mod parser;
pub mod token;

pub use ast::*;
pub use lexer::Lexer;
pub use parser::{parse, parse_program_full, ParseError, Parser};
pub use token::{Span, Token, TokenKind};
