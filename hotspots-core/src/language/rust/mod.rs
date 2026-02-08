//! Rust language support
//!
//! This module provides parsing and CFG building for Rust source code using the `syn` crate.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::RustCfgBuilder;
pub use parser::RustParser;
