//! Python language support
//!
//! This module provides Python language parsing, function discovery, and CFG building
//! using the tree-sitter-python parser.

pub mod parser;

pub use parser::PythonParser;
