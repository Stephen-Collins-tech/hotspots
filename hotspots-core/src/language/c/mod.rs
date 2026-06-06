//! C language support
//!
//! Parses C source files using tree-sitter-c. C has a flat AST (no classes),
//! so function discovery is a single-level walk over `function_definition` nodes.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::CCfgBuilder;
pub use parser::CParser;
