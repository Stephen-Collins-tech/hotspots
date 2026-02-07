//! Go language support
//!
//! This module provides Go language parsing, function discovery, and CFG building
//! using the tree-sitter-go parser.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::GoCfgBuilder;
pub use parser::GoParser;
