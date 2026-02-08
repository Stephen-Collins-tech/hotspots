//! Python language support
//!
//! This module provides Python language parsing, function discovery, and CFG building
//! using the tree-sitter-python parser.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::PythonCfgBuilder;
pub use parser::PythonParser;
