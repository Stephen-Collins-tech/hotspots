//! PHP language support
//!
//! This module provides PHP language parsing, function discovery, and CFG building
//! using the tree-sitter-php parser.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::PhpCfgBuilder;
pub use parser::PhpParser;
